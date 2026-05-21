// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;
use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Error;
use crate::git::Git;
use crate::metadata::AgentboxContainerKind;
use crate::paths::canonicalize_utf8_path;
use crate::podman::PodmanContainerMount;

use super::endpoint::AttachEndpointReport;
use super::labels::SessionLabelReport;
use super::mounts::has_volume_mount_destination;
use super::{REQUIRED_NIX_CACHE_MOUNT_DESTINATION, record::SessionRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Orphaned,
    Duplicate,
    Failed(Option<SessionFailure>),
}

impl SessionStatus {
    pub fn failed(failure: SessionFailure) -> Self {
        Self::Failed(Some(failure))
    }

    pub fn failed_unknown() -> Self {
        Self::Failed(None)
    }

    pub fn failure(self) -> Option<SessionFailure> {
        match self {
            Self::Failed(failure) => failure,
            _ => None,
        }
    }

    pub fn is_failed(self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub(crate) fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Orphaned => "orphaned",
            Self::Duplicate => "duplicate",
            Self::Failed(_) => "failed",
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFailure {
    MissingRequiredLabels,
    DriftedGitRootHash,
    MissingCacheMount,
    NotRunning,
    UnsupportedRuntimeLabel,
    MalformedLaunchDirectory,
    MalformedEndpointLabels,
    MissingPublishedAttachPort,
}

impl SessionFailure {
    pub fn requires_action_error(self, git_root: &Utf8Path, container_name: &str) -> Error {
        let action = self.action();

        Error::managed_session_requires_action(
            git_root,
            container_name,
            action.detail.as_ref(),
            action.next_step,
        )
    }

    fn action(self) -> FailureAction {
        match self {
            Self::MissingRequiredLabels => FailureAction::new(
                "is missing required session labels",
                "clean up or recreate it before retrying",
            ),
            Self::DriftedGitRootHash => FailureAction::new(
                "has a drifted `io.agentbox.git_root_hash`",
                "clean up or recreate it before retrying",
            ),
            Self::MissingCacheMount => FailureAction::new(
                format!(
                    "is missing required cache mount `{}`",
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION
                ),
                "recreate the container before retrying",
            ),
            Self::NotRunning => {
                FailureAction::new("is not running", "stop it or recreate it before retrying")
            }
            Self::UnsupportedRuntimeLabel => FailureAction::new(
                "has an unsupported or malformed `io.agentbox.runtime` label",
                "clean up or recreate it before retrying",
            ),
            Self::MalformedLaunchDirectory => FailureAction::new(
                "has a missing or malformed `io.agentbox.launch_directory` label",
                "clean up or recreate it before retrying",
            ),
            Self::MalformedEndpointLabels => FailureAction::new(
                "has missing or inconsistent attach endpoint labels",
                "clean up or recreate it before retrying",
            ),
            Self::MissingPublishedAttachPort => FailureAction::new(
                "has no published attach endpoint port",
                "clean up or recreate it before retrying",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FailureAction {
    detail: Cow<'static, str>,
    next_step: &'static str,
}

impl FailureAction {
    fn new(detail: impl Into<Cow<'static, str>>, next_step: &'static str) -> Self {
        Self {
            detail: detail.into(),
            next_step,
        }
    }
}

pub fn failed_session_requires_action_error(
    git_root: &Utf8Path,
    session: &SessionRecord,
) -> Option<Error> {
    session.status().failure().map(|failure| {
        resource_failure_requires_action_error(
            session.container_kind(),
            git_root,
            session.container_name(),
            failure,
        )
    })
}

pub fn session_failure_requires_action_error(
    git_root: &Utf8Path,
    container_name: &str,
    failure: SessionFailure,
) -> Error {
    resource_failure_requires_action_error(
        AgentboxContainerKind::Managed,
        git_root,
        container_name,
        failure,
    )
}

pub fn resource_failure_requires_action_error(
    container_kind: AgentboxContainerKind,
    git_root: &Utf8Path,
    container_name: &str,
    failure: SessionFailure,
) -> Error {
    let action = failure.action();
    Error::agentbox_container_requires_action(
        container_kind,
        git_root,
        container_name,
        action.detail.as_ref(),
        action.next_step,
    )
}

pub(super) fn derive_status(input: SessionStatusInput<'_>) -> SessionStatus {
    let SessionStatusInput {
        label_report,
        attach_endpoint,
        running,
        mounts,
        git,
    } = input;

    let required = match label_report.required_labels() {
        Ok(required) => required,
        Err(failure) => return SessionStatus::failed(failure),
    };

    if let Some(failure) = attach_endpoint.failure() {
        return SessionStatus::failed(failure);
    }

    if !has_volume_mount_destination(mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
        return SessionStatus::failed(SessionFailure::MissingCacheMount);
    }

    if !running {
        return SessionStatus::failed(SessionFailure::NotRunning);
    }

    let canonical_git_root = required.canonical_git_root();
    if git_root_is_orphaned(canonical_git_root, git) {
        return SessionStatus::Orphaned;
    }

    SessionStatus::Running
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SessionStatusInput<'a> {
    pub(super) label_report: &'a SessionLabelReport,
    pub(super) attach_endpoint: &'a AttachEndpointReport,
    pub(super) running: bool,
    pub(super) mounts: &'a [PodmanContainerMount],
    pub(super) git: &'a Git,
}

pub(super) fn mark_duplicate_sessions(mut sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
    let mut group_sizes = BTreeMap::<Utf8PathBuf, usize>::new();

    for session in &sessions {
        if session.status().is_failed() {
            continue;
        }

        if let Some(root) = session.canonical_git_root() {
            *group_sizes.entry(root.to_path_buf()).or_default() += 1;
        }
    }

    for session in &mut sessions {
        if session.status().is_failed() {
            continue;
        }

        if session
            .canonical_git_root()
            .and_then(|root| group_sizes.get(root))
            .is_some_and(|count| *count > 1)
        {
            session.mark_duplicate();
        }
    }

    sessions
}

fn git_root_is_orphaned(git_root: &Utf8Path, git: &Git) -> bool {
    let canonical_git_root = match canonicalize_utf8_path(git_root) {
        Ok(canonical_git_root) if canonical_git_root == git_root => canonical_git_root,
        _ => return true,
    };

    if !canonical_git_root.as_std_path().is_dir() {
        return true;
    }

    let git_marker = canonical_git_root.join(".git");
    if git_marker.is_dir() || git_marker.is_file() {
        return false;
    }

    match git.rev_parse_show_toplevel(&canonical_git_root) {
        Ok(resolved_git_root) => match canonicalize_utf8_path(&resolved_git_root) {
            Ok(resolved_git_root) => resolved_git_root != canonical_git_root,
            Err(_) => true,
        },
        Err(_) => true,
    }
}

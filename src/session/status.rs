// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Error;
use crate::git::Git;
use crate::podman::PodmanContainerMount;
use crate::runtime::AttachEndpoint;

use super::labels::SessionLabelReport;
use super::mounts::has_mount_destination;
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
        match self {
            Self::MissingRequiredLabels => Error::managed_session_requires_action(
                git_root,
                container_name,
                "is missing required session labels",
                "clean up or recreate it before retrying",
            ),
            Self::DriftedGitRootHash => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has a drifted `io.agentbox.git_root_hash`",
                "clean up or recreate it before retrying",
            ),
            Self::MissingCacheMount => Error::managed_session_requires_action(
                git_root,
                container_name,
                &format!(
                    "is missing required cache mount `{}`",
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION
                ),
                "recreate the container before retrying",
            ),
            Self::NotRunning => Error::managed_session_requires_action(
                git_root,
                container_name,
                "is not running",
                "stop it or recreate it before retrying",
            ),
            Self::UnsupportedRuntimeLabel => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has an unsupported or malformed `io.agentbox.runtime` label",
                "clean up or recreate it before retrying",
            ),
            Self::MalformedLaunchDirectory => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has a missing or malformed `io.agentbox.launch_directory` label",
                "clean up or recreate it before retrying",
            ),
            Self::MalformedEndpointLabels => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has missing or inconsistent attach endpoint labels",
                "clean up or recreate it before retrying",
            ),
            Self::MissingPublishedAttachPort => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has no published attach endpoint port",
                "clean up or recreate it before retrying",
            ),
        }
    }
}

pub fn failed_session_requires_action_error(
    git_root: &Utf8Path,
    session: &SessionRecord,
) -> Option<Error> {
    session.status.failure().map(|failure| {
        session_failure_requires_action_error(git_root, &session.container_name, failure)
    })
}

pub fn session_failure_requires_action_error(
    git_root: &Utf8Path,
    container_name: &str,
    failure: SessionFailure,
) -> Error {
    failure.requires_action_error(git_root, container_name)
}

pub(super) fn derive_status(input: SessionStatusInput<'_>) -> SessionStatus {
    let SessionStatusInput {
        label_report,
        attach_endpoint,
        running,
        mounts,
        git,
    } = input;

    let Some(canonical_git_root) = label_report.canonical_git_root() else {
        let failure = label_report
            .required_failure()
            .unwrap_or(SessionFailure::MissingRequiredLabels);
        return SessionStatus::failed(failure);
    };

    if let Some(failure) = label_report.attach_failure() {
        return SessionStatus::failed(failure);
    }

    if attach_endpoint.is_none() {
        return SessionStatus::failed(SessionFailure::MissingPublishedAttachPort);
    }

    if !has_mount_destination(mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
        return SessionStatus::failed(SessionFailure::MissingCacheMount);
    }

    if !running {
        return SessionStatus::failed(SessionFailure::NotRunning);
    }

    if git_root_is_orphaned(canonical_git_root, git) {
        return SessionStatus::Orphaned;
    }

    SessionStatus::Running
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SessionStatusInput<'a> {
    pub(super) label_report: &'a SessionLabelReport,
    pub(super) attach_endpoint: Option<&'a AttachEndpoint>,
    pub(super) running: bool,
    pub(super) mounts: &'a [PodmanContainerMount],
    pub(super) git: &'a Git,
}

pub(super) fn mark_duplicate_sessions(mut sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
    let mut group_sizes = BTreeMap::<Utf8PathBuf, usize>::new();

    for session in &sessions {
        if session.status.is_failed() {
            continue;
        }

        if let Some(root) = session.canonical_git_root() {
            *group_sizes.entry(root.to_path_buf()).or_default() += 1;
        }
    }

    for session in &mut sessions {
        if session.status.is_failed() {
            continue;
        }

        if session
            .canonical_git_root()
            .and_then(|root| group_sizes.get(root))
            .is_some_and(|count| *count > 1)
        {
            session.status = SessionStatus::Duplicate;
        }
    }

    sessions
}

fn git_root_is_orphaned(git_root: &Utf8Path, git: &Git) -> bool {
    let canonical_git_root = match canonicalize_utf8(git_root) {
        Some(canonical_git_root) if canonical_git_root == git_root => canonical_git_root,
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
        Ok(resolved_git_root) => canonicalize_utf8(&resolved_git_root)
            .is_none_or(|resolved_git_root| resolved_git_root != canonical_git_root),
        Err(_) => true,
    }
}

fn canonicalize_utf8(path: &Utf8Path) -> Option<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(std::fs::canonicalize(path.as_std_path()).ok()?).ok()
}

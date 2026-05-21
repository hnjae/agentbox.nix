// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

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
        git_root_probe,
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
    if git_root_is_orphaned(canonical_git_root, git_root_probe) {
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
    pub(super) git_root_probe: &'a dyn GitRootProbe,
}

pub(super) trait GitRootProbe: std::fmt::Debug {
    fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf>;
    fn is_directory(&self, git_root: &Utf8Path) -> bool;
    fn has_git_marker(&self, git_root: &Utf8Path) -> bool;
    fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf>;
}

#[derive(Debug)]
pub(super) struct HostGitRootProbe {
    git: Git,
}

impl HostGitRootProbe {
    pub(super) fn new() -> Self {
        Self { git: Git::new() }
    }
}

impl GitRootProbe for HostGitRootProbe {
    fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
        canonicalize_utf8_path(git_root).ok()
    }

    fn is_directory(&self, git_root: &Utf8Path) -> bool {
        git_root.as_std_path().is_dir()
    }

    fn has_git_marker(&self, git_root: &Utf8Path) -> bool {
        let git_marker = git_root.join(".git");
        git_marker.is_dir() || git_marker.is_file()
    }

    fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
        self.git.rev_parse_show_toplevel(git_root).ok()
    }
}

fn git_root_is_orphaned(git_root: &Utf8Path, probe: &dyn GitRootProbe) -> bool {
    let canonical_git_root = match probe.canonicalize(git_root) {
        Some(canonical_git_root) if canonical_git_root == git_root => canonical_git_root,
        _ => return true,
    };

    if !probe.is_directory(&canonical_git_root) {
        return true;
    }

    if probe.has_git_marker(&canonical_git_root) {
        return false;
    }

    match probe.rev_parse_show_toplevel(&canonical_git_root) {
        Some(resolved_git_root) => match probe.canonicalize(&resolved_git_root) {
            Some(resolved_git_root) => resolved_git_root != canonical_git_root,
            None => true,
        },
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn git_root_with_git_marker_is_not_orphaned_without_rev_parse() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_directory("/repo")
            .with_git_marker("/repo");

        assert!(!git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
        assert_eq!(probe.rev_parse_calls.get(), 0);
    }

    #[test]
    fn git_root_without_marker_is_not_orphaned_when_rev_parse_matches() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_directory("/repo")
            .with_rev_parse("/repo", "/repo");

        assert!(!git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
        assert_eq!(probe.rev_parse_calls.get(), 1);
    }

    #[test]
    fn git_root_is_orphaned_when_canonical_root_changes() {
        let probe = FakeGitRootProbe::new().with_canonical("/workspace/link", "/workspace/real");

        assert!(git_root_is_orphaned(
            Utf8Path::new("/workspace/link"),
            &probe
        ));
    }

    #[test]
    fn git_root_is_orphaned_when_rev_parse_resolves_elsewhere() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_canonical("/other", "/other")
            .with_directory("/repo")
            .with_rev_parse("/repo", "/other");

        assert!(git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
    }

    #[derive(Debug)]
    struct FakeGitRootProbe {
        canonical_paths: BTreeMap<Utf8PathBuf, Utf8PathBuf>,
        directories: BTreeSet<Utf8PathBuf>,
        git_markers: BTreeSet<Utf8PathBuf>,
        rev_parse_roots: BTreeMap<Utf8PathBuf, Utf8PathBuf>,
        rev_parse_calls: Cell<usize>,
    }

    impl FakeGitRootProbe {
        fn new() -> Self {
            Self {
                canonical_paths: BTreeMap::new(),
                directories: BTreeSet::new(),
                git_markers: BTreeSet::new(),
                rev_parse_roots: BTreeMap::new(),
                rev_parse_calls: Cell::new(0),
            }
        }

        fn with_canonical(mut self, input: &str, output: &str) -> Self {
            self.canonical_paths
                .insert(Utf8PathBuf::from(input), Utf8PathBuf::from(output));
            self
        }

        fn with_directory(mut self, path: &str) -> Self {
            self.directories.insert(Utf8PathBuf::from(path));
            self
        }

        fn with_git_marker(mut self, path: &str) -> Self {
            self.git_markers.insert(Utf8PathBuf::from(path));
            self
        }

        fn with_rev_parse(mut self, path: &str, root: &str) -> Self {
            self.rev_parse_roots
                .insert(Utf8PathBuf::from(path), Utf8PathBuf::from(root));
            self
        }
    }

    impl GitRootProbe for FakeGitRootProbe {
        fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
            self.canonical_paths.get(git_root).cloned()
        }

        fn is_directory(&self, git_root: &Utf8Path) -> bool {
            self.directories.contains(git_root)
        }

        fn has_git_marker(&self, git_root: &Utf8Path) -> bool {
            self.git_markers.contains(git_root)
        }

        fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
            self.rev_parse_calls
                .set(self.rev_parse_calls.get().saturating_add(1));
            self.rev_parse_roots.get(git_root).cloned()
        }
    }
}

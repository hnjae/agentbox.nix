// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod failure;
mod git_root;

use crate::podman::PodmanContainerMount;

use super::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use super::endpoint::AttachEndpointReport;
use super::labels::SessionLabelReport;
use super::mounts::has_volume_mount_destination;

pub use failure::{
    SessionFailure, failed_session_requires_action_error, resource_failure_requires_action_error,
    session_failure_requires_action_error,
};
pub(super) use git_root::{GitRootProbe, HostGitRootProbe};

use git_root::git_root_is_orphaned;

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

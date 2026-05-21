// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

use camino::Utf8Path;

use crate::Error;
use crate::metadata::AgentboxContainerKind;

use super::super::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use super::super::record::SessionRecord;

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

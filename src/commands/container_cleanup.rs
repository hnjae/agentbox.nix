// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::diagnostic;
use crate::podman::Podman;
use crate::session::SessionRecord;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::codex_attach_auth::remove_codex_attach_token_for_session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ManagedContainerCleanup {
    stop_error: Option<String>,
    verification: ContainerCleanupVerification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContainerCleanupVerification {
    Removed,
    StillExists,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ContainerCleanupIssue {
    StopFailed(String),
    StillExists,
    VerificationFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ContainerCleanupFailure {
    container_name: String,
    issues: Vec<ContainerCleanupIssue>,
}

impl ManagedContainerCleanup {
    pub(super) fn stop_and_verify(podman: &Podman, container_name: &str) -> Self {
        let stop_error = podman
            .stop_ignore(container_name)
            .err()
            .map(|error| error.to_string());
        let verification = match podman.container_exists(container_name) {
            Ok(false) => ContainerCleanupVerification::Removed,
            Ok(true) => ContainerCleanupVerification::StillExists,
            Err(error) => ContainerCleanupVerification::Failed(error.to_string()),
        };

        Self {
            stop_error,
            verification,
        }
    }

    pub(super) fn container_removed(&self) -> bool {
        matches!(self.verification, ContainerCleanupVerification::Removed)
    }

    pub(super) fn interrupted_messages(&self) -> Vec<String> {
        self.interrupted_issues()
            .iter()
            .map(ContainerCleanupIssue::interrupted_message)
            .collect()
    }

    fn interrupted_issues(&self) -> Vec<ContainerCleanupIssue> {
        let mut issues = self.stop_issue().into_iter().collect::<Vec<_>>();
        issues.extend(self.remaining_container_issue());
        issues
    }

    pub(super) fn interrupted_cache_volume_skip_message(&self) -> Option<&'static str> {
        match &self.verification {
            ContainerCleanupVerification::Removed => None,
            ContainerCleanupVerification::StillExists => {
                Some("cache volume removal skipped because the container still exists")
            }
            ContainerCleanupVerification::Failed(_) => {
                Some("cache volume removal skipped because container cleanup could not be verified")
            }
        }
    }

    pub(super) fn remaining_failure(
        &self,
        container_name: &str,
    ) -> Option<ContainerCleanupFailure> {
        let remaining_issue = self.remaining_container_issue()?;
        let mut issues = self.stop_issue().into_iter().collect::<Vec<_>>();
        issues.push(remaining_issue);

        Some(ContainerCleanupFailure::new(container_name, issues))
    }

    fn stop_issue(&self) -> Option<ContainerCleanupIssue> {
        self.stop_error
            .as_ref()
            .map(|error| ContainerCleanupIssue::StopFailed(error.clone()))
    }

    fn remaining_container_issue(&self) -> Option<ContainerCleanupIssue> {
        match &self.verification {
            ContainerCleanupVerification::Removed => None,
            ContainerCleanupVerification::StillExists => Some(ContainerCleanupIssue::StillExists),
            ContainerCleanupVerification::Failed(error) => {
                Some(ContainerCleanupIssue::VerificationFailed(error.clone()))
            }
        }
    }
}

pub(super) fn cleanup_managed_containers<'a>(
    podman: &Podman,
    sessions: impl IntoIterator<Item = &'a SessionRecord>,
) -> Vec<ContainerCleanupFailure> {
    sessions
        .into_iter()
        .filter_map(|session| cleanup_managed_container(podman, session))
        .collect()
}

pub(super) fn cleanup_transient_container(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
) -> Result<()> {
    diagnostic::info(format!(
        "stopping transient container `{}`",
        workspace.container_name
    ));
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, &workspace.container_name);

    if let Some(failure) = cleanup.remaining_failure(&workspace.container_name) {
        Err(Error::msg(format!(
            "failed to clean up transient run container `{}`: {}",
            workspace.container_name,
            failure.render_stop_message(),
        )))
    } else {
        Ok(())
    }
}

pub(super) fn combine_error_with_cleanup_result(error: Error, cleanup: Result<()>) -> Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => Error::msg(combined_error_message(&error, &cleanup_error)),
    }
}

pub(super) fn combined_error_message(error: &Error, cleanup_error: &Error) -> String {
    format!("{error}; additionally, {cleanup_error}")
}

fn cleanup_managed_container(
    podman: &Podman,
    session: &SessionRecord,
) -> Option<ContainerCleanupFailure> {
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, session.container_name());
    let failure = cleanup.remaining_failure(session.container_name());
    if failure.is_none() {
        let _ = remove_codex_attach_token_for_session(session);
    }
    failure
}

impl ContainerCleanupFailure {
    fn new(container_name: &str, issues: Vec<ContainerCleanupIssue>) -> Self {
        Self {
            container_name: container_name.to_string(),
            issues,
        }
    }

    pub(super) fn render_stop_message(&self) -> String {
        let details = self
            .issues
            .iter()
            .map(ContainerCleanupIssue::stop_message)
            .collect::<Vec<_>>()
            .join(", ");

        format!("container `{}` ({details})", self.container_name)
    }
}

impl ContainerCleanupIssue {
    fn interrupted_message(&self) -> String {
        match self {
            Self::StopFailed(error) => format!("container stop failed: {error}"),
            Self::StillExists => "container still exists after cleanup".to_string(),
            Self::VerificationFailed(error) => {
                format!("container cleanup verification failed: {error}")
            }
        }
    }

    fn stop_message(&self) -> String {
        match self {
            Self::StopFailed(error) => format!("stop failed: {error}"),
            Self::StillExists => "container still exists after stop".to_string(),
            Self::VerificationFailed(error) => {
                format!("follow-up inspect failed: {error}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_container_has_no_remaining_failure() {
        let cleanup = ManagedContainerCleanup {
            stop_error: Some("stop failed after removal".to_string()),
            verification: ContainerCleanupVerification::Removed,
        };

        assert!(cleanup.container_removed());
        assert_eq!(cleanup.remaining_container_issue(), None);
        assert_eq!(cleanup.remaining_failure("agentbox-demo"), None);
        assert_eq!(
            cleanup.stop_issue(),
            Some(ContainerCleanupIssue::StopFailed(
                "stop failed after removal".to_string()
            ))
        );
        assert_eq!(
            cleanup.interrupted_issues(),
            vec![ContainerCleanupIssue::StopFailed(
                "stop failed after removal".to_string()
            )]
        );
    }

    #[test]
    fn remaining_container_exposes_stable_failure_reason() {
        let cleanup = ManagedContainerCleanup {
            stop_error: None,
            verification: ContainerCleanupVerification::StillExists,
        };

        assert!(!cleanup.container_removed());
        assert_eq!(
            cleanup.remaining_container_issue(),
            Some(ContainerCleanupIssue::StillExists)
        );
        assert_eq!(
            ContainerCleanupIssue::StillExists.interrupted_message(),
            "container still exists after cleanup"
        );
        assert_eq!(
            ContainerCleanupIssue::StillExists.stop_message(),
            "container still exists after stop"
        );
        assert_eq!(
            cleanup.interrupted_cache_volume_skip_message(),
            Some("cache volume removal skipped because the container still exists")
        );
    }

    #[test]
    fn verification_error_exposes_error_detail() {
        let cleanup = ManagedContainerCleanup {
            stop_error: None,
            verification: ContainerCleanupVerification::Failed("inspect failed".to_string()),
        };

        assert!(!cleanup.container_removed());
        assert_eq!(
            cleanup.remaining_container_issue(),
            Some(ContainerCleanupIssue::VerificationFailed(
                "inspect failed".to_string()
            ))
        );
    }

    #[test]
    fn remaining_failure_includes_stop_error_and_remaining_container_issue() {
        let cleanup = ManagedContainerCleanup {
            stop_error: Some("podman stop failed".to_string()),
            verification: ContainerCleanupVerification::StillExists,
        };

        let failure = cleanup.remaining_failure("agentbox-demo").unwrap();

        assert_eq!(
            failure.render_stop_message(),
            "container `agentbox-demo` (stop failed: podman stop failed, container still exists after stop)"
        );
    }

    #[test]
    fn combines_primary_error_with_cleanup_error_stably() {
        let error = Error::msg("host client failed");
        let cleanup_error = Error::msg("cleanup failed");

        assert_eq!(
            combined_error_message(&error, &cleanup_error),
            "host client failed; additionally, cleanup failed"
        );
    }

    #[test]
    fn preserves_primary_error_when_cleanup_succeeds() {
        let error = Error::msg("readiness failed");

        assert_eq!(
            combine_error_with_cleanup_result(error, Ok(())).to_string(),
            "readiness failed"
        );
    }

    #[test]
    fn appends_cleanup_failure_when_cleanup_fails() {
        let error = Error::msg("readiness failed");
        let cleanup = Err(Error::msg("stop failed"));

        assert_eq!(
            combine_error_with_cleanup_result(error, cleanup).to_string(),
            "readiness failed; additionally, stop failed"
        );
    }
}

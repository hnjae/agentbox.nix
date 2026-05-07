// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::podman::Podman;

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

    pub(super) fn stop_issue(&self) -> Option<ContainerCleanupIssue> {
        self.stop_error
            .as_ref()
            .map(|error| ContainerCleanupIssue::StopFailed(error.clone()))
    }

    pub(super) fn remaining_container_issue(&self) -> Option<ContainerCleanupIssue> {
        match &self.verification {
            ContainerCleanupVerification::Removed => None,
            ContainerCleanupVerification::StillExists => Some(ContainerCleanupIssue::StillExists),
            ContainerCleanupVerification::Failed(error) => {
                Some(ContainerCleanupIssue::VerificationFailed(error.clone()))
            }
        }
    }
}

impl ContainerCleanupIssue {
    pub(super) fn interrupted_message(&self) -> String {
        match self {
            Self::StopFailed(error) => format!("container stop failed: {error}"),
            Self::StillExists => "container still exists after cleanup".to_string(),
            Self::VerificationFailed(error) => {
                format!("container cleanup verification failed: {error}")
            }
        }
    }

    pub(super) fn interrupted_cache_volume_skip_message(&self) -> Option<&'static str> {
        match self {
            Self::StopFailed(_) => None,
            Self::StillExists => {
                Some("cache volume removal skipped because the container still exists")
            }
            Self::VerificationFailed(_) => {
                Some("cache volume removal skipped because container cleanup could not be verified")
            }
        }
    }

    pub(super) fn stop_message(&self) -> String {
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
        assert_eq!(
            cleanup.stop_issue(),
            Some(ContainerCleanupIssue::StopFailed(
                "stop failed after removal".to_string()
            ))
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
}

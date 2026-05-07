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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContainerCleanupFailure<'a> {
    StillExists,
    VerificationFailed(&'a str),
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

    pub(super) fn stop_error(&self) -> Option<&str> {
        self.stop_error.as_deref()
    }

    pub(super) fn container_removed(&self) -> bool {
        matches!(self.verification, ContainerCleanupVerification::Removed)
    }

    pub(super) fn remaining_container_failure(&self) -> Option<ContainerCleanupFailure<'_>> {
        match &self.verification {
            ContainerCleanupVerification::Removed => None,
            ContainerCleanupVerification::StillExists => Some(ContainerCleanupFailure::StillExists),
            ContainerCleanupVerification::Failed(error) => {
                Some(ContainerCleanupFailure::VerificationFailed(error))
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
        assert_eq!(cleanup.remaining_container_failure(), None);
    }

    #[test]
    fn remaining_container_exposes_stable_failure_reason() {
        let cleanup = ManagedContainerCleanup {
            stop_error: None,
            verification: ContainerCleanupVerification::StillExists,
        };

        assert!(!cleanup.container_removed());
        assert_eq!(
            cleanup.remaining_container_failure(),
            Some(ContainerCleanupFailure::StillExists)
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
            cleanup.remaining_container_failure(),
            Some(ContainerCleanupFailure::VerificationFailed(
                "inspect failed"
            ))
        );
    }
}

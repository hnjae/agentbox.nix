// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::ExitStatus;

use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_cleanup::ManagedContainerCleanup;
use super::launch_policy::{CommandInterrupt, exit_code};
use super::runtime_command::host_client_status_error;

#[derive(Debug, Clone, Copy)]
pub(super) struct TransientRun<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
}

impl<'a> TransientRun<'a> {
    pub(super) fn new(podman: &'a Podman, workspace: &'a WorkspaceIdentity) -> Self {
        Self { podman, workspace }
    }

    pub(super) fn check_interrupted(self, interrupt: &CommandInterrupt) -> Result<()> {
        if interrupt.interrupted() {
            Err(self.interrupted_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn interrupted_error(self) -> Error {
        let error = Error::msg(format!(
            "run interrupted before transient container `{}` for `{}` finished",
            self.workspace.container_name, self.workspace.canonical_git_root,
        ));

        self.with_cleanup_result(error)
    }

    pub(super) fn finish_host_client_run(
        self,
        runtime: RuntimeKind,
        endpoint: &AttachEndpoint,
        status: Result<ExitStatus>,
    ) -> Result<()> {
        let cleanup = self.cleanup();
        match status {
            Ok(status) if status.success() => cleanup,
            Ok(status) => {
                let code = status.code().and_then(exit_code);
                let error = host_client_status_error(
                    runtime,
                    endpoint,
                    self.workspace.canonical_target.as_ref(),
                    status,
                );
                match code {
                    Some(code) => match cleanup {
                        Ok(()) => Err(Error::ExitCode(code)),
                        Err(cleanup_error) => Err(Error::ExitCodeWithMessage {
                            code,
                            message: combined_error_message(&error, &cleanup_error),
                        }),
                    },
                    None => Err(combine_error_with_cleanup_result(error, cleanup)),
                }
            }
            Err(error) => Err(combine_error_with_cleanup_result(error, cleanup)),
        }
    }

    pub(super) fn with_cleanup_result(self, error: Error) -> Error {
        combine_error_with_cleanup_result(error, self.cleanup())
    }

    fn cleanup(self) -> Result<()> {
        diagnostic::info(format!(
            "stopping transient container `{}`",
            self.workspace.container_name
        ));
        let cleanup =
            ManagedContainerCleanup::stop_and_verify(self.podman, &self.workspace.container_name);
        if let Some(failure) = cleanup.remaining_failure(&self.workspace.container_name) {
            Err(Error::msg(format!(
                "failed to clean up transient run container `{}`: {}",
                self.workspace.container_name,
                failure.render_stop_message(),
            )))
        } else {
            Ok(())
        }
    }
}

fn combine_error_with_cleanup_result(error: Error, cleanup: Result<()>) -> Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => Error::msg(combined_error_message(&error, &cleanup_error)),
    }
}

fn combined_error_message(error: &Error, cleanup_error: &Error) -> String {
    format!("{error}; additionally, {cleanup_error}")
}

#[cfg(test)]
mod tests {
    use super::*;

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

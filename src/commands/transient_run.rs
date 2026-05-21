// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::ExitStatus;

use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunSpec};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_cleanup::{
    cleanup_transient_container, combine_error_with_cleanup_result, combined_error_message,
};
use super::detached_server::{DetachedServerLifecycle, launch_detached_server};
use super::launch_policy::{
    CommandInterrupt, ContainerLogContext, error_with_container_logs, exit_code,
};
use super::runtime_command::{host_client_status_error, run_host_runtime_client_status};
use super::server_readiness::ServerEndpointContext;

#[derive(Debug, Clone, Copy)]
pub(super) struct TransientRunLaunch<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &'a RuntimeRunSpec,
}

impl<'a> TransientRunLaunch<'a> {
    pub(super) fn new(
        podman: &'a Podman,
        workspace: &'a WorkspaceIdentity,
        runtime: RuntimeKind,
        run_spec: &'a RuntimeRunSpec,
    ) -> Self {
        Self {
            podman,
            workspace,
            runtime,
            run_spec,
        }
    }

    pub(super) fn execute(self) -> Result<()> {
        let transient = TransientRun::new(self.podman, self.workspace);
        let ready_server = launch_detached_server(
            self.podman,
            self.workspace,
            self.runtime,
            self.run_spec,
            TransientServerLifecycle { transient },
        )?;
        let endpoint = ready_server.endpoint();

        diagnostic::info(format!(
            "transient container `{}` for `{}` is ready at `{endpoint}`; connecting",
            self.workspace.container_name, self.workspace.canonical_git_root,
        ));
        let status = run_host_runtime_client_status(
            self.runtime,
            endpoint,
            self.workspace.canonical_target.as_ref(),
        );
        transient.finish_host_client_run(self.runtime, endpoint, status)
    }
}

#[derive(Debug, Clone, Copy)]
struct TransientRun<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
}

impl<'a> TransientRun<'a> {
    fn new(podman: &'a Podman, workspace: &'a WorkspaceIdentity) -> Self {
        Self { podman, workspace }
    }

    fn podman(self) -> &'a Podman {
        self.podman
    }

    fn workspace(self) -> &'a WorkspaceIdentity {
        self.workspace
    }

    fn check_interrupted(self, interrupt: &CommandInterrupt) -> Result<()> {
        if interrupt.interrupted() {
            Err(self.interrupted_error())
        } else {
            Ok(())
        }
    }

    fn interrupted_error(self) -> Error {
        let error = Error::msg(format!(
            "run interrupted before transient container `{}` for `{}` finished",
            self.workspace.container_name, self.workspace.canonical_git_root,
        ));

        self.with_cleanup_result(error)
    }

    fn finish_host_client_run(
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

    fn with_cleanup_result(self, error: Error) -> Error {
        combine_error_with_cleanup_result(error, self.cleanup())
    }

    fn cleanup(self) -> Result<()> {
        cleanup_transient_container(self.podman, self.workspace)
    }
}

#[derive(Debug, Clone, Copy)]
struct TransientServerLifecycle<'a> {
    transient: TransientRun<'a>,
}

impl DetachedServerLifecycle for TransientServerLifecycle<'_> {
    fn command_name(&self) -> &'static str {
        "run"
    }

    fn launch_description(&self) -> &'static str {
        "transient container"
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::TransientRunContainer
    }

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()> {
        self.transient.check_interrupted(interrupt)
    }

    fn run_detached_error(&self, error: Error) -> Error {
        Error::msg(format!(
            "failed to start transient run container `{}` for `{}`: {error}",
            self.transient.workspace().container_name,
            self.transient.workspace().canonical_git_root,
        ))
    }

    fn readiness_error(&self, error: Error) -> Error {
        let workspace = self.transient.workspace();
        let error = error_with_container_logs(
            self.transient.podman(),
            workspace,
            ContainerLogContext::TransientRun,
            error,
        );
        self.transient.with_cleanup_result(error)
    }
}

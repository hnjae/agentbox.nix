// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::ExitStatus;

use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunSpec};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::codex_attach_auth::CodexAttachToken;
use super::container_cleanup::{cleanup_transient_container, combine_error_with_cleanup_result};
use super::detached_server::{
    DetachedServerContext, DetachedServerLifecycle, launch_detached_server,
};
use super::exit_status::CommandExitFailure;
use super::launch_policy::{CommandInterrupt, ContainerLogContext, error_with_container_logs};
use super::runtime_command::{host_client_status_error, run_host_runtime_client_status};
use super::server_readiness::ServerEndpointContext;

#[derive(Debug, Clone, Copy)]
pub(super) struct TransientRunLaunch<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &'a RuntimeRunSpec,
    codex_attach_token: Option<&'a CodexAttachToken>,
    client_args: &'a [String],
}

impl<'a> TransientRunLaunch<'a> {
    pub(super) fn new(
        podman: &'a Podman,
        workspace: &'a WorkspaceIdentity,
        runtime: RuntimeKind,
        run_spec: &'a RuntimeRunSpec,
        codex_attach_token: Option<&'a CodexAttachToken>,
        client_args: &'a [String],
    ) -> Self {
        Self {
            podman,
            workspace,
            runtime,
            run_spec,
            codex_attach_token,
            client_args,
        }
    }

    pub(super) fn execute(self) -> Result<()> {
        let transient = TransientRun::new(self.podman, self.workspace);
        let ready_server = launch_detached_server(
            DetachedServerContext::new(self.podman, self.workspace, self.runtime, self.run_spec),
            TransientServerLifecycle,
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
            self.codex_attach_token,
            self.client_args,
        );
        transient.finish_host_client_run(self.runtime, endpoint, status, self.client_args)
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
        client_args: &[String],
    ) -> Result<()> {
        let cleanup = self.cleanup();
        match status {
            Ok(status) => {
                let Some(failure) = CommandExitFailure::from_status(status, |status| {
                    host_client_status_error(
                        runtime,
                        endpoint,
                        self.workspace.canonical_target.as_ref(),
                        status,
                        client_args,
                    )
                }) else {
                    return cleanup;
                };

                Err(failure.into_error_with_cleanup_result(cleanup))
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
struct TransientServerLifecycle;

impl DetachedServerLifecycle for TransientServerLifecycle {
    fn command_name(&self) -> &'static str {
        "run"
    }

    fn launch_description(&self) -> &'static str {
        "transient container"
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::TransientRunContainer
    }

    fn check_interrupted(
        &self,
        context: DetachedServerContext<'_>,
        interrupt: &CommandInterrupt,
    ) -> Result<()> {
        TransientRun::new(context.podman(), context.workspace()).check_interrupted(interrupt)
    }

    fn run_detached_error(&self, context: DetachedServerContext<'_>, error: Error) -> Error {
        Error::msg(format!(
            "failed to start transient run container `{}` for `{}`: {error}",
            context.workspace().container_name,
            context.workspace().canonical_git_root,
        ))
    }

    fn readiness_error(&self, context: DetachedServerContext<'_>, error: Error) -> Error {
        let transient = TransientRun::new(context.podman(), context.workspace());
        let error = error_with_container_logs(
            context.podman(),
            context.workspace(),
            ContainerLogContext::TransientRun,
            error,
        );
        transient.with_cleanup_result(error)
    }
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::cli::StartArgs;
use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::RuntimeRunSpec;
use crate::session::classify_create_error_or_else;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_launch::{managed_server_launch_request, prepare_runtime_launch};
use super::detached_server::{DetachedServerLifecycle, launch_detached_server};
use super::launch_policy::{
    CommandInterrupt, ContainerLogContext, error_with_container_logs, select_runtime,
};
use super::runtime_command::run_host_runtime_client;
use super::server_readiness::ServerEndpointContext;
use super::workspace_flow::with_locked_workspace;

mod interrupt;

use interrupt::InterruptedRunCleanupScope;

pub fn run(args: StartArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox start requires --runtime when stdin or stderr is not a TTY",
    )?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_runtime_launch(managed_server_launch_request(
            &locked,
            runtime,
            args.dev_env,
        ))?;
        let run_spec = preparation.run_spec;

        let cache_volume_existed_before = podman.volume_exists(&workspace.container_name)?;
        let cleanup =
            InterruptedRunCleanupScope::new(podman, workspace, cache_volume_existed_before);
        let endpoint = launch_detached_server(
            podman,
            workspace,
            runtime,
            &run_spec,
            ManagedServerLifecycle {
                podman,
                workspace,
                run_spec: &run_spec,
                cleanup,
            },
        )?
        .into_endpoint();
        if args.connect {
            diagnostic::info(format!(
                "managed session `{}` for `{}` is ready at `{endpoint}`; connecting",
                workspace.container_name, workspace.canonical_git_root,
            ));
            run_host_runtime_client(runtime, &endpoint, workspace.canonical_target.as_ref())
                .map_err(|error| {
                    Error::msg(format!(
                        "failed to connect to newly created managed session `{}` for `{}`: {error}. The session remains running; retry with `agentbox connect {}` or stop it with `agentbox stop {}`.",
                        workspace.container_name,
                        workspace.canonical_git_root,
                        workspace.requested_target,
                        workspace.requested_target,
                    ))
                })?;
        } else {
            diagnostic::info(format!(
                "managed session `{}` for `{}` is ready at `{endpoint}`; use `agentbox connect {}` to connect",
                workspace.container_name, workspace.canonical_git_root, workspace.requested_target,
            ));
        }

        Ok(())
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ManagedServerLifecycle<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    run_spec: &'a RuntimeRunSpec,
    cleanup: InterruptedRunCleanupScope<'a>,
}

impl DetachedServerLifecycle for ManagedServerLifecycle<'_> {
    fn command_name(&self) -> &'static str {
        "start"
    }

    fn launch_description(&self) -> &'static str {
        "container"
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::ManagedSession
    }

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()> {
        self.cleanup.check_interrupted(interrupt)
    }

    fn run_detached_error(&self, error: Error) -> Error {
        classify_run_create_error(self.podman, self.workspace, self.run_spec, error)
    }

    fn readiness_error(&self, error: Error) -> Error {
        error_with_container_logs(
            self.podman,
            self.workspace,
            ContainerLogContext::ManagedSession,
            error,
        )
    }
}

fn classify_run_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    run_spec: &RuntimeRunSpec,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        "start the runtime server command",
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, run_spec.create(), wrapped, |error| {
        error_with_container_logs(
            podman,
            workspace,
            ContainerLogContext::ManagedSession,
            error,
        )
    })
}

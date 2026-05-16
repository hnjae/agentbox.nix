// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::ExitStatus;

use crate::cli::RunArgs;
use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunMode};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_cleanup::ManagedContainerCleanup;
use super::container_launch::{HostClientRequirement, prepare_container_launch};
use super::launch_policy::{
    CommandInterrupt, ContainerLogContext, error_with_container_logs, exit_code, select_runtime,
};
use super::runtime_command::{
    host_client_status_error, run_host_runtime_client_status, server_runtime_command,
};
use super::server_readiness::{ServerEndpointWait, wait_for_transient_server_endpoint};
use super::workspace_flow::with_locked_workspace;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox run requires --runtime when stdin or stderr is not a TTY",
    )?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_container_launch(
            &locked,
            runtime,
            args.dev_env,
            HostClientRequirement::Required,
        )?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            &preparation.dev_env,
        );
        let run_spec = runtime.run_spec(
            RuntimeRunMode::TransientServer,
            workspace,
            &preparation.preflight.host_nix_mounts,
            &preparation.preflight.runtime_mounts,
            server_run,
        );

        let interrupt = CommandInterrupt::install("run")?;
        diagnostic::info(format!(
            "starting transient container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        if let Err(error) = podman.run_detached(&workspace.container_name, &run_spec) {
            if interrupt.interrupted() {
                return Err(interrupted_error(podman, workspace));
            }

            return Err(Error::msg(format!(
                "failed to start transient run container `{}` for `{}`: {error}",
                workspace.container_name, workspace.canonical_git_root,
            )));
        }
        check_interrupted(&interrupt, podman, workspace)?;

        diagnostic::info(format!("waiting for `{runtime}` runtime server"));
        let endpoint = match wait_for_transient_server_endpoint(podman, workspace, runtime, || {
            interrupt.interrupted()
        }) {
            Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
            Ok(ServerEndpointWait::Interrupted) => {
                return Err(interrupted_error(podman, workspace));
            }
            Err(error) => {
                let error = error_with_container_logs(
                    podman,
                    workspace,
                    ContainerLogContext::TransientRun,
                    error,
                );
                return Err(with_cleanup_result(
                    error,
                    cleanup_transient_container(podman, workspace),
                ));
            }
        };
        check_interrupted(&interrupt, podman, workspace)?;

        diagnostic::info(format!(
            "transient container `{}` for `{}` is ready at `{endpoint}`; connecting",
            workspace.container_name, workspace.canonical_git_root,
        ));
        let status =
            run_host_runtime_client_status(runtime, &endpoint, workspace.canonical_target.as_ref());
        finish_host_client_run(podman, workspace, runtime, &endpoint, status)
    })?;
    Ok(())
}

fn finish_host_client_run(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    status: Result<ExitStatus>,
) -> Result<()> {
    let cleanup = cleanup_transient_container(podman, workspace);
    match status {
        Ok(status) if status.success() => cleanup,
        Ok(status) => {
            let code = status.code().and_then(exit_code);
            let error = host_client_status_error(
                runtime,
                endpoint,
                workspace.canonical_target.as_ref(),
                status,
            );
            match code {
                Some(code) => match cleanup {
                    Ok(()) => Err(Error::ExitCode(code)),
                    Err(cleanup_error) => Err(Error::ExitCodeWithMessage {
                        code,
                        message: format!("{error}; additionally, {cleanup_error}"),
                    }),
                },
                None => Err(with_cleanup_result(error, cleanup)),
            }
        }
        Err(error) => Err(with_cleanup_result(error, cleanup)),
    }
}

fn check_interrupted(
    interrupt: &CommandInterrupt,
    podman: &Podman,
    workspace: &WorkspaceIdentity,
) -> Result<()> {
    if interrupt.interrupted() {
        Err(interrupted_error(podman, workspace))
    } else {
        Ok(())
    }
}

fn interrupted_error(podman: &Podman, workspace: &WorkspaceIdentity) -> Error {
    let error = Error::msg(format!(
        "run interrupted before transient container `{}` for `{}` finished",
        workspace.container_name, workspace.canonical_git_root,
    ));
    with_cleanup_result(error, cleanup_transient_container(podman, workspace))
}

fn cleanup_transient_container(podman: &Podman, workspace: &WorkspaceIdentity) -> Result<()> {
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

fn with_cleanup_result(error: Error, cleanup: Result<()>) -> Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => Error::msg(format!("{error}; additionally, {cleanup_error}")),
    }
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::RunArgs;
use crate::diagnostic;
use crate::metadata::runtime_package_version_label;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites_for_runtime;
use crate::prompt;
use crate::runtime::RuntimeKind;
use crate::runtime::RuntimeRunSpec;
use crate::session::{
    classify_create_error_or_else, existing_session_error, select_single_session,
};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime::ensure_default_runtime_image;
use super::runtime_command::{run_host_runtime_client, server_runtime_command};
use super::server_readiness::{ServerEndpointWait, wait_for_server_endpoint};
use super::workspace_flow::with_locked_workspace;

mod interrupt;

use interrupt::{InterruptedRunCleanupScope, RunInterrupt};

const RUN_FAILURE_LOG_TAIL_LINES: usize = 80;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = selected_runtime(args.runtime)?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        diagnostic::info("checking workspace prerequisites");
        let preflight = check_host_prerequisites_for_runtime(
            runtime,
            Some(workspace.canonical_target.as_ref()),
            Some(workspace.canonical_git_root.as_ref()),
        )?;

        diagnostic::info("checking existing managed sessions");
        let podman = locked.podman();
        let sessions = locked.discover_sessions()?;
        if let Some(session) = select_single_session(&sessions, workspace)? {
            return Err(existing_session_error(podman, workspace, session));
        }

        let runtime_version = ensure_default_runtime_image(
            podman,
            runtime,
            workspace.canonical_git_root.as_ref(),
            diagnostic::info,
        )?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        );
        let mut run_spec = runtime.run_spec(
            workspace,
            &preflight.host_nix_mounts,
            &preflight.runtime_mounts,
            server_run,
        );
        if let Some(version) = runtime_version {
            run_spec
                .create_mut()
                .labels
                .insert(runtime_package_version_label(runtime), version);
        }

        let cache_volume_existed_before = podman.volume_exists(&workspace.container_name)?;
        let interrupt = RunInterrupt::install()?;
        let cleanup =
            InterruptedRunCleanupScope::new(podman, workspace, cache_volume_existed_before);

        diagnostic::info(format!(
            "starting container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        if let Err(error) = podman.run_detached(&workspace.container_name, &run_spec) {
            cleanup.check_interrupted(&interrupt)?;
            return Err(classify_run_create_error(
                podman, workspace, &run_spec, error,
            ));
        }
        cleanup.check_interrupted(&interrupt)?;

        diagnostic::info(format!("waiting for `{runtime}` runtime server"));
        let endpoint = match wait_for_server_endpoint(podman, workspace, runtime, || {
            interrupt.interrupted()
        }) {
            Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
            Ok(ServerEndpointWait::Interrupted) => {
                return Err(cleanup.interrupted_error());
            }
            Err(error) => return Err(error_with_container_logs(podman, workspace, error)),
        };
        cleanup.check_interrupted(&interrupt)?;

        drop(interrupt);
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

fn selected_runtime(runtime: Option<RuntimeKind>) -> Result<RuntimeKind> {
    match runtime {
        Some(runtime) => Ok(runtime),
        None => prompt::select_one(
            "Select runtime",
            RuntimeKind::variants().to_vec(),
            "agentbox run requires --runtime when stdin or stderr is not a TTY",
        ),
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
        "run the runtime server command",
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, run_spec.create(), wrapped, |error| {
        error_with_container_logs(podman, workspace, error)
    })
}

fn error_with_container_logs(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    original_error: Error,
) -> Error {
    let container_name = &workspace.container_name;
    let command = format!("podman logs --tail {RUN_FAILURE_LOG_TAIL_LINES} {container_name}");
    match podman.logs_tail(container_name, RUN_FAILURE_LOG_TAIL_LINES) {
        Ok(logs) => {
            let logs = logs.trim_end();
            if logs.is_empty() {
                Error::msg(format!(
                    "{original_error}\n\ncontainer `{container_name}` produced no logs; inspect it with `{command}`"
                ))
            } else {
                Error::msg(format!(
                    "{original_error}\n\ncontainer logs (`{command}`):\n{logs}"
                ))
            }
        }
        Err(log_error) => Error::msg(format!(
            "{original_error}\n\nfailed to read container logs with `{command}`: {log_error}"
        )),
    }
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::RunArgs;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites;
use crate::runtime::{RuntimeCreateSpec, RuntimeKind};
use crate::session::{classify_create_error_or_else, existing_session_error};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime_command::server_runtime_command;
use super::server_readiness::wait_for_server_endpoint;
use super::session_selection::select_single_session;
use super::workspace_flow::with_locked_workspace;

const RUN_FAILURE_LOG_TAIL_LINES: usize = 80;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = args.runtime;
    let diagnostics = RunDiagnostics::new(verbose);
    let (workspace, endpoint) = with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        diagnostics.phase("checking workspace prerequisites");
        let preflight = check_host_prerequisites(
            Some(workspace.canonical_target.as_ref()),
            Some(workspace.canonical_git_root.as_ref()),
        )?;

        diagnostics.phase("checking existing managed sessions");
        let podman = locked.podman();
        let sessions = locked.discover_sessions()?;
        if let Some(session) = select_single_session(&sessions, workspace)? {
            return Err(existing_session_error(podman, workspace, session));
        }

        ensure_default_runtime_image(podman, runtime, workspace, &diagnostics)?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        );
        let run_spec = runtime
            .create_spec(workspace, &preflight.host_nix_mounts)
            .with_command(server_run.argv);

        diagnostics.phase(format!(
            "starting container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        podman
            .run_detached(
                &workspace.container_name,
                &run_spec,
                Some(server_run.workdir.as_str()),
            )
            .map_err(|error| classify_run_create_error(podman, workspace, &run_spec, error))?;

        diagnostics.phase(format!("waiting for `{runtime}` runtime server"));
        let endpoint = wait_for_server_endpoint(podman, workspace, runtime)
            .map_err(|error| error_with_container_logs(podman, workspace, error))?;

        Ok(endpoint)
    })?;

    println!(
        "managed session `{}` is running for `{}` at `{endpoint}`; use `agentbox attach {}` to connect",
        workspace.container_name, workspace.canonical_git_root, workspace.requested_target,
    );
    Ok(())
}

fn ensure_default_runtime_image(
    podman: &Podman,
    runtime: RuntimeKind,
    workspace: &WorkspaceIdentity,
    diagnostics: &RunDiagnostics,
) -> Result<()> {
    let default_image = runtime.default_image();
    if podman.image_exists(default_image)? {
        diagnostics.phase(format!("using runtime image `{default_image}`"));
        return Ok(());
    }

    diagnostics.phase(format!("building runtime image `{default_image}`"));
    let context = runtime.materialize_default_image_context()?;
    let containerfile = context.containerfile();
    podman
        .build_image(default_image, containerfile.as_ref(), context.root())
        .map_err(|error| {
            Error::msg(format!(
                "failed to build default runtime image `{default_image}` for `{}` from `{}`: {error}",
                workspace.canonical_git_root,
                context.root(),
            ))
        })
}

#[derive(Debug, Clone, Copy)]
struct RunDiagnostics;

impl RunDiagnostics {
    fn new(_verbose: bool) -> Self {
        Self
    }

    fn phase(&self, message: impl AsRef<str>) {
        eprintln!("agentbox: {}", message.as_ref());
    }
}

fn classify_run_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        "run the runtime server command",
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, create_spec, wrapped, |error| {
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

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
use crate::runtime::{RuntimeAdapter, RuntimeCreateSpec};
use crate::session::{classify_create_error, existing_session_error};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime_command::server_runtime_command;
use super::server_readiness::wait_for_server_endpoint;
use super::session_selection::select_single_session;
use super::workspace_flow::with_locked_workspace;

pub fn run(args: RunArgs) -> Result<()> {
    let runtime = args.runtime.adapter();
    let (workspace, endpoint) = with_locked_workspace(&args.directory, |locked| {
        let workspace = locked.workspace();
        let preflight = check_host_prerequisites(
            Some(workspace.canonical_target.as_ref()),
            Some(workspace.canonical_git_root.as_ref()),
        )?;

        let podman = locked.podman();
        let sessions = locked.discover_sessions()?;
        if let Some(session) = select_single_session(&sessions, workspace)? {
            return Err(existing_session_error(podman, workspace, session));
        }

        ensure_default_runtime_image(podman, runtime, workspace)?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        );
        let run_spec = runtime
            .create_spec(workspace, &preflight.host_nix_mounts)
            .with_command(server_run.argv);

        let endpoint = podman
            .run_detached(
                &workspace.container_name,
                &run_spec,
                Some(server_run.workdir.as_str()),
            )
            .and_then(|_| wait_for_server_endpoint(podman, workspace, runtime))
            .map_err(|error| classify_run_error(podman, workspace, &run_spec, error))?;

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
    runtime: RuntimeAdapter,
    workspace: &WorkspaceIdentity,
) -> Result<()> {
    let default_image = runtime.default_image();
    if podman.image_exists(default_image)? {
        return Ok(());
    }

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

fn classify_run_error(
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
    classify_create_error(podman, workspace, create_spec, wrapped)
}

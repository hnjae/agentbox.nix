// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::cli::RunArgs;
use crate::lock::lock_workspace;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites;
use crate::runtime::{AttachEndpoint, RuntimeAdapter, RuntimeCreateSpec};
use crate::session::{
    classify_create_error, discover_attach_endpoint_from_inspect, discover_sessions_for_git_root,
    existing_session_error,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

use super::runtime_command::server_runtime_command;
use super::session_selection::select_single_session;

pub fn run(args: RunArgs) -> Result<()> {
    let workspace = resolve_workspace_identity(&args.directory)?;
    let runtime = args.runtime.adapter();
    let mut workspace_lock = lock_workspace(&workspace)?;
    let workspace_guard = workspace_lock.guard()?;

    let preflight = check_host_prerequisites(
        Some(workspace.canonical_target.as_ref()),
        Some(workspace.canonical_git_root.as_ref()),
    )?;

    let podman = Podman::new();
    let sessions = discover_sessions_for_git_root(&podman, workspace.canonical_git_root.as_ref())?;
    if let Some(session) = select_single_session(&sessions, &workspace)? {
        return Err(existing_session_error(&podman, &workspace, session));
    }

    ensure_default_runtime_image(&podman, runtime, &workspace)?;
    let mut run_spec = runtime.create_spec(&workspace, &preflight.host_nix_mounts);
    let server_run = server_runtime_command(
        runtime,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    );
    run_spec.command = server_run.argv;

    std::hint::black_box(&workspace_guard);

    let endpoint = podman
        .run_detached(
            &workspace.container_name,
            &run_spec,
            Some(server_run.workdir.as_str()),
        )
        .and_then(|_| wait_for_server_endpoint(&podman, &workspace, runtime))
        .map_err(|error| classify_run_error(&podman, &workspace, &run_spec, error))?;

    drop(workspace_guard);
    drop(workspace_lock);

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

fn wait_for_server_endpoint(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeAdapter,
) -> Result<AttachEndpoint> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_error = None::<String>;

    loop {
        if Instant::now() >= deadline {
            let last_error = last_error
                .as_deref()
                .unwrap_or("no inspect data was available");
            return Err(Error::msg(format!(
                "runtime server for managed session `{}` in `{}` did not become reachable: {last_error}",
                workspace.container_name, workspace.canonical_git_root,
            )));
        }

        match podman.inspect_one(&workspace.container_name) {
            Ok(inspect) if !inspect.state.running => {
                return Err(Error::msg(format!(
                    "container `{}` for `{}` exited before the `{}` runtime server became reachable; status: {}, exit code: {}",
                    workspace.container_name,
                    workspace.canonical_git_root,
                    runtime.name(),
                    inspect.state.status,
                    inspect.state.exit_code,
                )));
            }
            Ok(inspect) => match discover_attach_endpoint_from_inspect(&inspect) {
                Ok(endpoint) if readiness_check_succeeded(&endpoint) => return Ok(endpoint),
                Ok(endpoint) => {
                    last_error = Some(format!("endpoint `{endpoint}` is not reachable yet"));
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                }
            },
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

fn readiness_check_succeeded(endpoint: &AttachEndpoint) -> bool {
    if std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some() {
        return true;
    }

    let Ok(addresses) = (endpoint.host_ip.as_str(), endpoint.host_port).to_socket_addrs() else {
        return false;
    };

    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok())
}

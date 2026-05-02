// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use camino::Utf8Path;

use crate::cli::RunArgs;
use crate::direnv::wrap_exec_if_envrc_applies;
use crate::lock::lock_workspace;
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount};
use crate::preflight::check_host_prerequisites;
use crate::runtime::opencode::materialize_default_image_context;
use crate::runtime::{AttachEndpoint, RuntimeAdapter, RuntimeCreateSpec};
use crate::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionRecord, SessionStatus,
    discover_attach_endpoint_from_inspect, discover_sessions_for_git_root,
    failed_session_requires_action_error, missing_required_label, required_label_value,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

use super::session_selection::{SingleSession, duplicate_sessions_error, select_single_session};

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
    match select_single_session(&sessions) {
        SingleSession::Missing => {}
        SingleSession::Found(session) => {
            return Err(existing_session_error(&podman, &workspace, session));
        }
        SingleSession::Duplicate => {
            return Err(duplicate_sessions_error(&workspace));
        }
    }

    ensure_default_runtime_image(&podman, runtime, &workspace)?;
    let mut run_spec = runtime.create_spec(&workspace, &preflight.host_nix_mounts);
    let server_run = server_run_spec(
        &runtime,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    );
    run_spec.command = server_run.argv;

    std::hint::black_box(&workspace_guard);

    let endpoint = podman
        .run_detached(
            &workspace.container_name,
            &run_spec,
            server_run.workdir.as_deref(),
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

    let context = materialize_default_image_context()?;
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

fn existing_session_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Error {
    if session.status == SessionStatus::Duplicate {
        return duplicate_sessions_error(workspace);
    }

    match session.status {
        SessionStatus::Running => running_existing_session_error(workspace, session),
        SessionStatus::Orphaned => Error::orphaned_managed_session(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
        ),
        SessionStatus::Failed => {
            failed_session_requires_action_error(workspace.canonical_git_root.as_ref(), session)
                .unwrap_or_else(|| {
                    podman
                        .inspect_one(&session.container_name)
                        .ok()
                        .and_then(|inspect| {
                            classify_named_container_conflict(
                                workspace,
                                &session.container_name,
                                &inspect,
                            )
                        })
                        .unwrap_or_else(|| {
                            generic_failed_session_error(workspace, &session.container_name)
                        })
                })
        }
        SessionStatus::Duplicate => duplicate_sessions_error(workspace),
    }
}

fn classify_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    podman
        .inspect_one(&workspace.container_name)
        .ok()
        .and_then(|inspect| {
            classify_named_container_conflict(
                workspace,
                &create_spec.labels[LABEL_LOGICAL_NAME],
                &inspect,
            )
        })
        .unwrap_or(original_error)
}

fn classify_named_container_conflict(
    workspace: &WorkspaceIdentity,
    expected_name: &str,
    inspect: &PodmanContainerInspect,
) -> Option<Error> {
    let labels = &inspect.config.labels;
    let container_name = inspect_container_name(inspect, expected_name);
    let managed = required_label_value(labels, LABEL_MANAGED);
    let canonical_git_root = required_label_value(labels, LABEL_GIT_ROOT);
    let git_root_hash = required_label_value(labels, LABEL_GIT_ROOT_HASH);

    if managed == Some(LABEL_MANAGED_VALUE) {
        if missing_required_label(labels) {
            return Some(Error::msg(format!(
                "managed session `{}` for `{}` is missing required session labels; repair or recreate it before retrying",
                container_name, workspace.canonical_git_root,
            )));
        }

        if git_root_hash == Some(workspace.hash12.as_str())
            && canonical_git_root.is_some_and(|root| root != workspace.canonical_git_root.as_str())
        {
            return Some(Error::msg(format!(
                "managed container `{}` collides on git-root hash `{}`: stored root `{}` does not match `{}`; remove or recreate the conflicting container before retrying",
                container_name,
                workspace.hash12,
                canonical_git_root.unwrap_or("<missing>"),
                workspace.canonical_git_root,
            )));
        }

        if canonical_git_root == Some(workspace.canonical_git_root.as_str()) {
            if git_root_hash != Some(workspace.hash12.as_str()) {
                return Some(Error::msg(format!(
                    "managed session `{}` for `{}` has a drifted `io.agentbox.git_root_hash`; repair or recreate it before retrying",
                    container_name, workspace.canonical_git_root,
                )));
            }

            if !has_required_mount(&inspect.mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
                return Some(Error::msg(format!(
                    "managed session `{}` for `{}` is missing required cache mount `{}`; recreate the container before retrying",
                    container_name,
                    workspace.canonical_git_root,
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
                )));
            }

            return Some(generic_failed_session_error(workspace, &container_name));
        }

        if let Some(root) = canonical_git_root {
            return Some(Error::msg(format!(
                "container name `{}` is already used by managed session `{}` for `{}`; remove or rename the conflicting container before retrying `{}`",
                workspace.container_name, container_name, root, workspace.canonical_git_root,
            )));
        }
    }

    Some(Error::msg(format!(
        "container name `{}` is already in use by a different container; remove or rename that container before retrying `{}`",
        workspace.container_name, workspace.canonical_git_root,
    )))
}

fn running_existing_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` is already running for `{}`; use `agentbox attach {}` to join it or `agentbox stop {}` to stop it first",
        session.container_name,
        workspace.canonical_git_root,
        workspace.requested_target,
        workspace.requested_target,
    ))
}

fn generic_failed_session_error(workspace: &WorkspaceIdentity, container_name: &str) -> Error {
    Error::failed_managed_session(workspace.canonical_git_root.as_ref(), container_name)
}

fn inspect_container_name(inspect: &PodmanContainerInspect, fallback: &str) -> String {
    required_label_value(&inspect.config.labels, LABEL_LOGICAL_NAME)
        .unwrap_or(fallback)
        .to_string()
}

fn has_required_mount(mounts: &[PodmanContainerMount], destination: &str) -> bool {
    mounts.iter().any(|mount| mount.destination == destination)
}

pub(crate) struct RuntimeCommandSpec {
    pub(crate) argv: Vec<String>,
    pub(crate) workdir: Option<String>,
}

pub(crate) fn server_run_spec(
    runtime: &RuntimeAdapter,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> RuntimeCommandSpec {
    let base = runtime.server_command();
    let workdir = Some(target.to_string());
    let argv = wrap_exec_if_envrc_applies(base.argv, target, git_root);

    RuntimeCommandSpec { argv, workdir }
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

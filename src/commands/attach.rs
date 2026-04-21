// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::DirectoryArgs;
use crate::commands::run::{
    podman_exec, podman_exec_interactive, podman_start, server_start_spec, wait_for_readiness,
};
use crate::lock::lock_workspace;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites;
use crate::process::ProcessRunner;
use crate::runtime::opencode::OpencodeRuntime;
use crate::session::{
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionFailure, SessionRecord, SessionStatus,
    discover_sessions_for_git_root,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

pub fn run(args: DirectoryArgs) -> Result<()> {
    let workspace = resolve_workspace_identity(&args.directory)?;
    let mut workspace_lock = lock_workspace(&workspace)?;
    let workspace_guard = workspace_lock.guard()?;

    let podman = Podman::new();
    let sessions = discover_sessions_for_git_root(&podman, workspace.canonical_git_root.as_ref())?;
    let session = match sessions.as_slice() {
        [] => return Err(no_session_error(&workspace)),
        [session] => validate_attachable_session(&workspace, session)?,
        _ => return Err(duplicate_sessions_error(&workspace)),
    };

    let runtime = OpencodeRuntime::new();
    let process_runner = ProcessRunner::new();

    if session.status == SessionStatus::Stopped {
        check_host_prerequisites(
            Some(workspace.canonical_target.as_ref()),
            Some(workspace.canonical_git_root.as_ref()),
        )?;
        podman_start(&process_runner, &session.container_name).map_err(|error| {
            Error::session_start_failed(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                &error.to_string(),
            )
        })?;

        let server_start = server_start_spec(
            &runtime,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        );
        podman_exec(
            &process_runner,
            &session.container_name,
            &server_start.argv,
            server_start.workdir.as_deref(),
            true,
        )
        .map_err(|error| {
            Error::runtime_command_failed(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                "start the runtime server",
                &error.to_string(),
            )
        })?;
    }

    wait_for_readiness(&process_runner, &session.container_name, &runtime).map_err(|error| {
        Error::runtime_readiness_timeout(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
            &error.to_string(),
        )
    })?;

    std::hint::black_box(&workspace_guard);
    drop(workspace_guard);
    drop(workspace_lock);

    podman_exec_interactive(
        &process_runner,
        &session.container_name,
        &runtime
            .attach_command(workspace.canonical_target.as_ref())
            .argv,
        None,
    )
    .map_err(|error| {
        Error::runtime_command_failed(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
            "attach via `/entrypoint`",
            &error.to_string(),
        )
    })
}

fn validate_attachable_session<'a>(
    workspace: &WorkspaceIdentity,
    session: &'a SessionRecord,
) -> Result<&'a SessionRecord> {
    if session.status == SessionStatus::Duplicate {
        return Err(duplicate_sessions_error(workspace));
    }

    match session.status {
        SessionStatus::Running | SessionStatus::Stopped => {
            if session.runtime.as_deref() != Some(crate::runtime::opencode::RUNTIME_NAME) {
                Err(runtime_mismatch_error(
                    workspace,
                    &session.container_name,
                    session.runtime.as_deref().unwrap_or("unknown"),
                ))
            } else {
                Ok(session)
            }
        }
        SessionStatus::Orphaned => Err(Error::msg(format!(
            "managed session `{}` for `{}` is orphaned after the repository moved; remove or recreate it before retrying",
            session.container_name, workspace.canonical_git_root,
        ))),
        SessionStatus::Failed => Err(match session.failure {
            Some(SessionFailure::MissingRequiredLabels) => Error::managed_session_requires_action(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                "is missing required session labels",
                "repair or recreate it before retrying",
            ),
            Some(SessionFailure::DriftedGitRootHash) => Error::managed_session_requires_action(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                "has a drifted `io.agentbox.git_root_hash`",
                "repair or recreate it before retrying",
            ),
            Some(SessionFailure::MissingCacheMount) => Error::managed_session_requires_action(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                &format!(
                    "is missing required cache mount `{}`",
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION
                ),
                "recreate the container before retrying",
            ),
            None => Error::msg(format!(
                "managed session `{}` for `{}` is in a failed state; repair or recreate it before retrying",
                session.container_name, workspace.canonical_git_root,
            )),
        }),
        SessionStatus::Duplicate => Err(duplicate_sessions_error(workspace)),
    }
}

fn no_session_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "no managed session exists for `{}`; use `agentbox run {}` to create one",
        workspace.canonical_git_root, workspace.requested_target,
    ))
}

fn duplicate_sessions_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "duplicate managed sessions exist for `{}`; remove extras before retrying",
        workspace.canonical_git_root
    ))
}

fn runtime_mismatch_error(
    workspace: &WorkspaceIdentity,
    container_name: &str,
    actual_runtime: &str,
) -> Error {
    Error::msg(format!(
        "managed session `{}` for `{}` uses runtime `{}` instead of `{}`; recreate it before retrying",
        container_name,
        workspace.canonical_git_root,
        actual_runtime,
        crate::runtime::opencode::RUNTIME_NAME,
    ))
}

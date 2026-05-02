// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::Stdio;

use crate::cli::DirectoryArgs;
use crate::lock::lock_workspace;
use crate::podman::Podman;
use crate::process::{ProcessRunner, format_status, run_command_status};
use crate::runtime::RuntimeKind;
use crate::session::{
    SessionFailure, SessionRecord, SessionStatus, discover_sessions_for_git_root,
    duplicate_sessions_error, session_failure_requires_action_error,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

use super::runtime_command::{RuntimeInvocation, host_client_runtime_command};
use super::session_selection::{run_command_hint, select_single_session};

pub fn run(args: DirectoryArgs) -> Result<()> {
    let workspace = resolve_workspace_identity(&args.directory)?;
    let mut workspace_lock = lock_workspace(&workspace)?;
    let workspace_guard = workspace_lock.guard()?;

    let podman = Podman::new();
    let sessions = discover_sessions_for_git_root(&podman, workspace.canonical_git_root.as_ref())?;
    let Some(session) = select_single_session(&sessions, &workspace)? else {
        return Err(no_session_error(&workspace));
    };
    let session = validate_attachable_session(&workspace, session)?;

    let process_runner = ProcessRunner::new();
    let runtime = session
        .runtime
        .as_deref()
        .ok_or_else(|| unsupported_runtime_label_error(&workspace, session))?
        .parse::<RuntimeKind>()
        .map_err(|_| unsupported_runtime_label_error(&workspace, session))?
        .adapter();
    let endpoint = session
        .attach_endpoint
        .as_ref()
        .ok_or_else(|| missing_endpoint_error(&workspace, session))?;
    let client = host_client_runtime_command(runtime, endpoint, &workspace);
    let retry_run_command = run_command_hint(Some(runtime.name()), &workspace);

    std::hint::black_box(&workspace_guard);

    run_host_client(&process_runner, &client).map_err(|error| {
        Error::msg(format!(
            "failed to attach to managed session `{}` for `{}`: {error}. If the session already exited, rerun `{}` or remove the leftover container with `agentbox stop {}`.",
            session.container_name,
            workspace.canonical_git_root,
            retry_run_command,
            workspace.requested_target,
        ))
    })?;

    drop(workspace_guard);
    drop(workspace_lock);
    Ok(())
}

fn validate_attachable_session<'a>(
    workspace: &WorkspaceIdentity,
    session: &'a SessionRecord,
) -> Result<&'a SessionRecord> {
    if session.status == SessionStatus::Duplicate {
        return Err(duplicate_sessions_error(workspace));
    }

    match session.status {
        SessionStatus::Running => Ok(session),
        SessionStatus::Orphaned => Err(Error::orphaned_managed_session(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
        )),
        SessionStatus::Failed => Err(match session.failure {
            Some(SessionFailure::NotRunning) => not_running_session_error(workspace, session),
            Some(failure) => session_failure_requires_action_error(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
                failure,
            ),
            None => Error::failed_managed_session(
                workspace.canonical_git_root.as_ref(),
                &session.container_name,
            ),
        }),
        SessionStatus::Duplicate => Err(duplicate_sessions_error(workspace)),
    }
}

fn no_session_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "no managed session exists for `{}`; use `{}` to create one",
        workspace.canonical_git_root,
        run_command_hint(None, workspace),
    ))
}

fn not_running_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` for `{}` is not running; rerun `{}` to start a new session or `agentbox stop {}` to remove the leftover container",
        session.container_name,
        workspace.canonical_git_root,
        run_command_hint(session.runtime.as_deref(), workspace),
        workspace.requested_target,
    ))
}

fn unsupported_runtime_label_error(
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Error {
    Error::managed_session_requires_action(
        workspace.canonical_git_root.as_ref(),
        &session.container_name,
        "has an unsupported or malformed `io.agentbox.runtime` label",
        "repair or recreate it before retrying",
    )
}

fn missing_endpoint_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::managed_session_requires_action(
        workspace.canonical_git_root.as_ref(),
        &session.container_name,
        "has missing or inconsistent attach endpoint labels",
        "repair or recreate it before retrying",
    )
}

fn run_host_client(process_runner: &ProcessRunner, client: &RuntimeInvocation) -> Result<()> {
    let argv = &client.argv;
    let Some((program, args)) = argv.split_first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    let mut command = process_runner.command(program)?;
    command.args(args);
    command.current_dir(client.workdir.as_std_path());
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let status = run_command_status(&mut command)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "`{}` exited with {}",
            argv.join(" "),
            format_status(status)
        )))
    }
}

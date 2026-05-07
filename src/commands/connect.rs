// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::{Path, PathBuf};

use camino::Utf8Path;

use crate::cli::ConnectArgs;
use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{SessionRecord, discover_managed_sessions};
use crate::session::{prepare_connect_session, run_command_hint, select_single_session};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime_command::run_host_runtime_client;
use super::session_targets::SessionTargetKind;
use super::workspace_flow::with_locked_workspace;

pub fn run(args: ConnectArgs) -> Result<()> {
    let directory = selected_connect_directory(args.directory)?;
    connect_directory(&directory)
}

fn selected_connect_directory(directory: Option<PathBuf>) -> Result<PathBuf> {
    match directory {
        Some(directory) => Ok(directory),
        None => select_connect_directory(),
    }
}

fn select_connect_directory() -> Result<PathBuf> {
    prompt::require_interactive_terminal(
        "agentbox connect requires a target when stdin or stderr is not a TTY",
    )?;
    let podman = Podman::new();
    let candidates = connect_prompt_candidates(&discover_managed_sessions(&podman)?);

    if candidates.is_empty() {
        return Err(Error::msg("no connectable managed sessions exist"));
    }

    let selected = prompt::select_one(
        "Select session",
        candidates,
        "agentbox connect requires a target when stdin or stderr is not a TTY",
    )?;
    Ok(selected.into_value())
}

fn connect_directory(directory: &Path) -> Result<()> {
    with_locked_workspace(directory, false, |locked| {
        let workspace = locked.workspace();
        let sessions = locked.discover_sessions()?;
        let Some(session) = select_single_session(&sessions, workspace)? else {
            return Err(no_session_error(workspace));
        };
        let connect_session = prepare_connect_session(workspace, session)?;
        let retry_run_command =
            run_command_hint(Some(connect_session.runtime().as_str()), workspace);
        report_launch_directory_notice(workspace, connect_session.launch_directory());

        run_host_runtime_client(
            connect_session.runtime(),
            connect_session.endpoint(),
            connect_session.launch_directory(),
        )
        .map_err(|error| {
            Error::msg(format!(
                "failed to connect to managed session `{}` for `{}`: {error}. If the session already exited, rerun `{}` or remove the leftover container with `agentbox stop {}`.",
                connect_session.session().container_name,
                workspace.canonical_git_root,
                retry_run_command,
                workspace.requested_target,
            ))
        })
    })?;

    Ok(())
}

pub type ConnectPromptCandidate = prompt::Choice<PathBuf>;

pub fn connect_prompt_candidates(sessions: &[SessionRecord]) -> Vec<ConnectPromptCandidate> {
    SessionTargetKind::ConnectRoot.prompt_choices(
        sessions,
        |candidate| PathBuf::from(candidate.value()),
        |candidate| candidate.connect_prompt_label(),
    )
}

fn no_session_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "no managed session exists for `{}`; use `{}` to create one",
        workspace.canonical_git_root,
        run_command_hint(None, workspace),
    ))
}

fn report_launch_directory_notice(workspace: &WorkspaceIdentity, launch_directory: &Utf8Path) {
    if workspace.canonical_target.as_str() == launch_directory.as_str() {
        return;
    }

    diagnostic::info(format!(
        "agentbox connect: `{}` identified the workspace; using stored launch directory `{}`",
        workspace.canonical_target, launch_directory,
    ));
}

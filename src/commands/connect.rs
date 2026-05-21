// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use camino::Utf8Path;
use clap::Args;

use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{SessionDiscoveryQuery, SessionRecord, SessionTargetKind};
use crate::session::{prepare_connect_session, run_command_hint, select_single_session};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime_command::run_host_runtime_client;
use super::session_targets::{connect_prompt_label, prompt_choices};
use super::workspace_flow::with_locked_workspace;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ConnectArgs {
    /// Workspace directory inside a git repository.
    pub directory: Option<PathBuf>,
}

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
    let candidates =
        connect_prompt_candidates(&SessionDiscoveryQuery::managed_sessions().discover(&podman)?);

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
        let sessions = locked.discover_managed_sessions()?;
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
                connect_session.session().container_name(),
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
    prompt_choices(
        SessionTargetKind::ConnectRoot,
        sessions,
        |candidate| PathBuf::from(candidate.value()),
        connect_prompt_label,
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

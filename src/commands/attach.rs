// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use camino::Utf8Path;

use crate::cli::AttachArgs;
use crate::podman::Podman;
use crate::process::{ProcessRunner, format_status, run_command_status};
use crate::prompt;
use crate::session::{SessionRecord, SessionStatus, discover_managed_sessions};
use crate::session::{prepare_attach_session, run_command_hint, select_single_session};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::runtime_command::{RuntimeInvocation, host_client_runtime_command};
use super::workspace_flow::with_locked_workspace;

pub fn run(args: AttachArgs) -> Result<()> {
    let directory = selected_attach_directory(args.directory)?;
    attach_directory(&directory)
}

fn selected_attach_directory(directory: Option<PathBuf>) -> Result<PathBuf> {
    match directory {
        Some(directory) => Ok(directory),
        None => select_attach_directory(),
    }
}

fn select_attach_directory() -> Result<PathBuf> {
    prompt::require_interactive_terminal(
        "agentbox attach requires a target when stdin or stderr is not a TTY",
    )?;
    let podman = Podman::new();
    let candidates = attach_prompt_candidates(&discover_managed_sessions(&podman)?);

    if candidates.is_empty() {
        return Err(Error::msg("no attachable managed sessions exist"));
    }

    let selected = prompt::select_one(
        "Select session",
        candidates,
        "agentbox attach requires a target when stdin or stderr is not a TTY",
    )?;
    Ok(selected.directory)
}

fn attach_directory(directory: &Path) -> Result<()> {
    with_locked_workspace(directory, false, |locked| {
        let workspace = locked.workspace();
        let sessions = locked.discover_sessions()?;
        let Some(session) = select_single_session(&sessions, workspace)? else {
            return Err(no_session_error(workspace));
        };
        let attach_session = prepare_attach_session(workspace, session)?;

        let process_runner = ProcessRunner::new();
        let client = host_client_runtime_command(
            attach_session.runtime(),
            attach_session.endpoint(),
            attach_session.launch_directory(),
        );
        let retry_run_command =
            run_command_hint(Some(attach_session.runtime().as_str()), workspace);
        report_launch_directory_notice(workspace, attach_session.launch_directory());

        run_host_client(&process_runner, &client).map_err(|error| {
            Error::msg(format!(
                "failed to attach to managed session `{}` for `{}`: {error}. If the session already exited, rerun `{}` or remove the leftover container with `agentbox stop {}`.",
                attach_session.session().container_name,
                workspace.canonical_git_root,
                retry_run_command,
                workspace.requested_target,
            ))
        })
    })?;

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachPromptCandidate {
    label: String,
    directory: PathBuf,
}

impl AttachPromptCandidate {
    fn new(label: String, directory: PathBuf) -> Self {
        Self { label, directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

impl fmt::Display for AttachPromptCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub fn attach_prompt_candidates(sessions: &[SessionRecord]) -> Vec<AttachPromptCandidate> {
    let mut candidates = sessions
        .iter()
        .filter(|session| session.status == SessionStatus::Running)
        .filter(|session| session.attach_endpoint.is_some())
        .filter_map(attach_prompt_candidate)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates
}

fn attach_prompt_candidate(session: &SessionRecord) -> Option<AttachPromptCandidate> {
    let root = session.canonical_git_root()?;
    let runtime = session.runtime().unwrap_or("unknown");
    let label = format!("{root} ({runtime})");

    Some(AttachPromptCandidate::new(
        label,
        root.as_std_path().to_path_buf(),
    ))
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

    eprintln!(
        "agentbox attach: `{}` identified the workspace; using stored launch directory `{}`",
        workspace.canonical_target, launch_directory,
    );
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

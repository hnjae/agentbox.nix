// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::StopArgs;
use crate::lock::lock_git_root;
use crate::podman::Podman;
use crate::process::{ProcessRunner, run_command};
use crate::session::{SessionRecord, discover_sessions_for_git_root};
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

pub fn run(args: StopArgs) -> Result<()> {
    let git_root = resolve_stop_git_root(&args.directory)?;
    let mut workspace_lock = lock_git_root(git_root.as_ref())?;
    let workspace_guard = workspace_lock.guard()?;
    let podman = Podman::new();
    let sessions = exact_full_root_matches(
        discover_sessions_for_git_root(&podman, git_root.as_ref())?,
        git_root.as_ref(),
    );

    if sessions.is_empty() {
        return Err(Error::msg(format!(
            "no managed session exists for `{git_root}`"
        )));
    }

    if sessions.len() > 1 && !args.force {
        return Err(Error::msg(format!(
            "duplicate managed sessions exist for `{git_root}`; rerun `agentbox stop --force {}` to remove all exact matches",
            args.directory.display()
        )));
    }

    let process_runner = ProcessRunner::new();
    let failures = sessions
        .iter()
        .filter_map(|session| cleanup_session(&podman, &process_runner, session))
        .collect::<Vec<_>>();

    drop(workspace_guard);
    drop(workspace_lock);

    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_cleanup_failures(
            git_root.as_ref(),
            &failures,
        )))
    }
}

fn resolve_stop_git_root(directory: &Path) -> Result<Utf8PathBuf> {
    if directory.exists() {
        return resolve_workspace_identity(directory).map(|workspace| workspace.canonical_git_root);
    }

    if !directory.is_absolute() {
        return Err(Error::msg(format!(
            "failed to resolve missing path `{}`; pass the exact absolute orphaned git-root path instead",
            directory.display()
        )));
    }

    Utf8PathBuf::from_path_buf(directory.to_path_buf())
        .map_err(|path| Error::msg(format!("non-utf8 path: {path:?}")))
}

fn exact_full_root_matches(
    sessions: Vec<SessionRecord>,
    git_root: &Utf8Path,
) -> Vec<SessionRecord> {
    sessions
        .into_iter()
        .filter(|session| session.canonical_git_root.as_deref() == Some(git_root))
        .collect()
}

fn cleanup_session(
    podman: &Podman,
    process_runner: &ProcessRunner,
    session: &SessionRecord,
) -> Option<CleanupFailure> {
    let stop_error = podman_stop(process_runner, &session.container_name).err();
    let remove_error = podman_remove_container(process_runner, &session.container_name).err();

    if stop_error.is_none() && remove_error.is_none() {
        return None;
    }

    match container_exists(podman, &session.container_name) {
        Ok(false) => None,
        Ok(true) => Some(CleanupFailure {
            container_name: session.container_name.clone(),
            stop_error,
            remove_error,
            verification_error: None,
        }),
        Err(error) => Some(CleanupFailure {
            container_name: session.container_name.clone(),
            stop_error,
            remove_error,
            verification_error: Some(error.to_string()),
        }),
    }
}

fn podman_stop(process_runner: &ProcessRunner, container_name: &str) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.args(["stop", container_name]);
    run_command(&mut command).map(|_| ())
}

fn podman_remove_container(process_runner: &ProcessRunner, container_name: &str) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.args(["rm", container_name]);
    run_command(&mut command).map(|_| ())
}

fn container_exists(podman: &Podman, container_name: &str) -> Result<bool> {
    match podman.inspect_one(container_name) {
        Ok(_) => Ok(true),
        Err(error) if is_missing_container_error(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn is_missing_container_error(error: &Error) -> bool {
    let message = error.to_string();
    message.contains("no such object") || message.contains("returned no containers")
}

fn render_cleanup_failures(git_root: &Utf8Path, failures: &[CleanupFailure]) -> String {
    let details = failures
        .iter()
        .map(CleanupFailure::render)
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "partial cleanup failed for `{git_root}`; remaining artifacts: {details}. cache volumes are left untouched and may be reclaimed separately"
    )
}

struct CleanupFailure {
    container_name: String,
    stop_error: Option<Error>,
    remove_error: Option<Error>,
    verification_error: Option<String>,
}

impl CleanupFailure {
    fn render(&self) -> String {
        let mut details = Vec::new();

        if let Some(error) = &self.stop_error {
            details.push(format!("stop failed: {error}"));
        }
        if let Some(error) = &self.remove_error {
            details.push(format!("remove failed: {error}"));
        }
        if let Some(error) = &self.verification_error {
            details.push(format!("follow-up inspect failed: {error}"));
        }

        format!(
            "container `{}` ({})",
            self.container_name,
            details.join(", ")
        )
    }
}

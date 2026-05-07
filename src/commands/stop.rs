// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::StopArgs;
use crate::diagnostic;
use crate::paths::path_buf_to_utf8;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{
    SessionGroup, SessionRecord, discover_managed_sessions, exact_git_root_matches,
    partition_sessions_by_git_root, select_stable_id_prefix,
};
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

use super::container_cleanup::{ContainerCleanupVerification, ManagedContainerCleanup};
use super::workspace_flow::with_locked_git_root;

pub fn run(args: StopArgs) -> Result<()> {
    if args.all {
        if !args.targets.is_empty() {
            return Err(Error::msg("stop --all does not accept a target"));
        }

        return stop_all_running();
    }

    let targets = if args.targets.is_empty() {
        select_stop_targets()?
    } else {
        args.targets
    };

    stop_targets(&targets, args.force)
}

fn select_stop_targets() -> Result<Vec<PathBuf>> {
    let non_tty_error =
        "agentbox stop requires a target or --all when stdin or stderr is not a TTY";
    prompt::require_interactive_terminal(non_tty_error)?;
    let podman = Podman::new();
    let candidates = stop_prompt_candidates(&discover_managed_sessions(&podman)?);

    if candidates.is_empty() {
        diagnostic::info("agentbox stop: no managed sessions available to stop");
        return Ok(Vec::new());
    }

    let selected = prompt::select_many("Select sessions to stop", candidates, non_tty_error)?;
    if selected.is_empty() {
        diagnostic::warning("agentbox stop: no sessions selected");
        return Ok(Vec::new());
    }

    let mut targets = selected
        .into_iter()
        .map(prompt::Choice::into_value)
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();

    Ok(targets)
}

fn stop_targets(targets: &[PathBuf], force: bool) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }

    if targets.len() == 1 {
        return stop_target(&targets[0], force);
    }

    let mut failures = Vec::new();
    for target in targets {
        if let Err(error) = stop_target(target, force) {
            failures.push(TargetStopFailure::new(target, error));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_target_stop_failures(&failures)))
    }
}

fn stop_target(target: &Path, force: bool) -> Result<()> {
    match resolve_stop_target(target)? {
        StopTarget::ResolvedGitRoot(git_root) => stop_git_root(&git_root, force, target),
        StopTarget::ExactStoredGitRootPath(git_root) => {
            stop_exact_stored_git_root_path(&git_root, force, target)
        }
        StopTarget::StableId(prefix) => stop_stable_id(&prefix, force, target),
    }
}

struct TargetStopFailure {
    target: String,
    error: Error,
}

impl TargetStopFailure {
    fn new(target: &Path, error: Error) -> Self {
        Self {
            target: target.display().to_string(),
            error,
        }
    }
}

fn render_target_stop_failures(failures: &[TargetStopFailure]) -> String {
    let noun = if failures.len() == 1 {
        "target"
    } else {
        "targets"
    };
    let details = failures
        .iter()
        .map(|failure| format!("`{}`: {}", failure.target, failure.error))
        .collect::<Vec<_>>()
        .join("; ");

    format!("failed to stop {} {noun}: {details}", failures.len())
}

pub type StopPromptCandidate = prompt::Choice<PathBuf>;

pub fn stop_prompt_candidates(sessions: &[SessionRecord]) -> Vec<StopPromptCandidate> {
    let mut candidates = sessions
        .iter()
        .filter(|session| session.has_stable_id())
        .filter_map(stop_prompt_candidate)
        .collect::<Vec<_>>();
    prompt::sort_choices_by_label(&mut candidates);
    candidates
}

fn stop_prompt_candidate(session: &SessionRecord) -> Option<StopPromptCandidate> {
    let display = session.display();
    let id = display.id()?;
    let root = display.canonical_git_root_or_unknown();
    let runtime = display.runtime_or_unknown();
    let label = format!("{id} {root} {runtime} {}", session.status.as_str());

    Some(prompt::Choice::new(label, PathBuf::from(id)))
}

enum StopTarget {
    ResolvedGitRoot(Utf8PathBuf),
    ExactStoredGitRootPath(Utf8PathBuf),
    StableId(String),
}

fn stop_git_root(git_root: &Utf8Path, force: bool, target: &Path) -> Result<()> {
    stop_exact_git_root_matches(git_root, force, target, ExactGitRootStopMode::Scoped)
}

fn stop_exact_stored_git_root_path(git_root: &Utf8Path, force: bool, target: &Path) -> Result<()> {
    stop_exact_git_root_matches(
        git_root,
        force,
        target,
        ExactGitRootStopMode::ExactStoredPath,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExactGitRootStopMode {
    Scoped,
    ExactStoredPath,
}

impl ExactGitRootStopMode {
    fn target_ref(self, git_root: &Utf8Path) -> String {
        match self {
            Self::Scoped => format!("`{git_root}`"),
            Self::ExactStoredPath => format!("exact stored git-root path `{git_root}`"),
        }
    }

    fn discover_sessions(
        self,
        locked: &super::workspace_flow::LockedGitRoot<'_>,
    ) -> Result<Vec<SessionRecord>> {
        match self {
            Self::Scoped => locked.discover_sessions(),
            Self::ExactStoredPath => discover_managed_sessions(locked.podman()),
        }
    }
}

fn stop_exact_git_root_matches(
    git_root: &Utf8Path,
    force: bool,
    target: &Path,
    mode: ExactGitRootStopMode,
) -> Result<()> {
    let failures = with_locked_git_root(git_root, |locked| {
        let sessions = exact_git_root_matches(mode.discover_sessions(&locked)?, git_root);
        let target_ref = mode.target_ref(git_root);

        if sessions.is_empty() {
            return Err(Error::msg(format!(
                "no managed session exists for {target_ref}"
            )));
        }

        if sessions.len() > 1 && !force {
            return Err(Error::msg(format!(
                "duplicate managed sessions exist for {target_ref}; rerun `agentbox stop --force {}` to remove all exact matches",
                target.display()
            )));
        }

        Ok(cleanup_sessions(locked.podman(), sessions.iter()))
    })?;

    finish_cleanup(git_root.as_str(), &failures)
}

fn stop_stable_id(prefix: &str, force: bool, target: &Path) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    let selection = select_stable_id_prefix(&sessions, prefix)?;
    let id = selection.id().to_string();

    if selection.has_duplicate_sessions() && !force {
        return Err(Error::msg(format!(
            "duplicate managed sessions exist for stable id `{id}`; rerun `agentbox stop --force {}` to remove all exact matches",
            target.display()
        )));
    }

    let sessions = lockable_stable_id_matches(selection.into_sessions(), &id)?;
    let failures = cleanup_stable_id_matches(sessions)?;

    finish_cleanup(&format!("id {id}"), &failures)
}

fn stop_all_running() -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?
        .into_iter()
        .filter(|session| session.container_running())
        .collect::<Vec<_>>();
    let failures = cleanup_all_running_matches(sessions)?;

    finish_cleanup("all running managed sessions", &failures)
}

fn resolve_stop_target(target: &Path) -> Result<StopTarget> {
    if target.exists() {
        return resolve_workspace_identity(target)
            .map(|workspace| StopTarget::ResolvedGitRoot(workspace.canonical_git_root));
    }

    if target.is_absolute() {
        let git_root = path_buf_to_utf8(target.to_path_buf())?;
        return Ok(StopTarget::ExactStoredGitRootPath(git_root));
    }

    let prefix = target.to_str().ok_or_else(|| {
        Error::msg(format!(
            "non-utf8 target `{}` cannot be used as a stable id prefix",
            target.display()
        ))
    })?;

    Ok(StopTarget::StableId(prefix.to_string()))
}

fn lockable_stable_id_matches(
    sessions: Vec<&SessionRecord>,
    id: &str,
) -> Result<Vec<SessionRecord>> {
    let selected_count = sessions.len();
    let lockable = sessions
        .into_iter()
        .filter(|session| session.canonical_git_root().is_some())
        .cloned()
        .collect::<Vec<_>>();

    if lockable.is_empty() {
        return Err(Error::msg(format!(
            "managed session id `{id}` cannot be stopped safely because no matched session has a recoverable git-root label"
        )));
    }

    if lockable.len() != selected_count {
        return Err(Error::msg(format!(
            "managed session id `{id}` includes matched containers without a recoverable git-root label; cannot stop them safely"
        )));
    }

    Ok(lockable)
}

fn cleanup_stable_id_matches(sessions: Vec<SessionRecord>) -> Result<Vec<CleanupFailure>> {
    let partition = partition_sessions_by_git_root(sessions);

    cleanup_rooted_session_groups(partition.rooted, |locked, sessions| {
        Ok(cleanup_sessions(locked.podman(), sessions.iter()))
    })
}

fn cleanup_all_running_matches(sessions: Vec<SessionRecord>) -> Result<Vec<CleanupFailure>> {
    let partition = partition_sessions_by_git_root(sessions);

    let mut failures = cleanup_rooted_session_groups(partition.rooted, |locked, _sessions| {
        let sessions = exact_git_root_matches(locked.discover_sessions()?, locked.git_root());
        Ok(cleanup_sessions(
            locked.podman(),
            sessions
                .iter()
                .filter(|session| session.container_running()),
        ))
    })?;

    if !partition.unrooted.is_empty() {
        let podman = Podman::new();
        failures.extend(cleanup_sessions(&podman, partition.unrooted.iter()));
    }

    Ok(failures)
}

fn cleanup_rooted_session_groups(
    groups: Vec<SessionGroup>,
    mut cleanup_group: impl FnMut(
        &super::workspace_flow::LockedGitRoot<'_>,
        &[SessionRecord],
    ) -> Result<Vec<CleanupFailure>>,
) -> Result<Vec<CleanupFailure>> {
    let mut failures = Vec::new();

    for group in groups {
        let git_root = group.canonical_git_root;
        let sessions = group.sessions;
        let mut group_failures =
            with_locked_git_root(&git_root, |locked| cleanup_group(&locked, &sessions))?;
        failures.append(&mut group_failures);
    }

    Ok(failures)
}

fn cleanup_sessions<'a>(
    podman: &Podman,
    sessions: impl IntoIterator<Item = &'a SessionRecord>,
) -> Vec<CleanupFailure> {
    sessions
        .into_iter()
        .filter_map(|session| cleanup_managed_container(podman, session))
        .collect()
}

fn cleanup_managed_container(podman: &Podman, session: &SessionRecord) -> Option<CleanupFailure> {
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, &session.container_name);

    match cleanup.verification() {
        ContainerCleanupVerification::Removed => None,
        ContainerCleanupVerification::StillExists => {
            let mut reasons = cleanup_failure_reasons(&cleanup);
            reasons.push(CleanupFailureReason::StillExists);
            Some(CleanupFailure::new(&session.container_name, reasons))
        }
        ContainerCleanupVerification::Failed(error) => {
            let mut reasons = cleanup_failure_reasons(&cleanup);
            reasons.push(CleanupFailureReason::VerificationFailed(error.to_string()));
            Some(CleanupFailure::new(&session.container_name, reasons))
        }
    }
}

fn cleanup_failure_reasons(cleanup: &ManagedContainerCleanup) -> Vec<CleanupFailureReason> {
    cleanup
        .stop_error()
        .map(|error| CleanupFailureReason::StopFailed(error.to_string()))
        .into_iter()
        .collect()
}

fn finish_cleanup(identity: &str, failures: &[CleanupFailure]) -> Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_cleanup_failures(identity, failures)))
    }
}

fn render_cleanup_failures(identity: &str, failures: &[CleanupFailure]) -> String {
    let details = failures
        .iter()
        .map(CleanupFailure::render)
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "partial stop failed for `{identity}`; remaining managed containers: {details}. podman-owned image cleanup and cache volumes are left untouched"
    )
}

struct CleanupFailure {
    container_name: String,
    reasons: Vec<CleanupFailureReason>,
}

enum CleanupFailureReason {
    StopFailed(String),
    StillExists,
    VerificationFailed(String),
}

impl CleanupFailure {
    fn new(container_name: &str, reasons: Vec<CleanupFailureReason>) -> Self {
        Self {
            container_name: container_name.to_string(),
            reasons,
        }
    }

    fn render(&self) -> String {
        let details = self
            .reasons
            .iter()
            .map(CleanupFailureReason::render)
            .collect::<Vec<_>>()
            .join(", ");

        format!("container `{}` ({details})", self.container_name)
    }
}

impl CleanupFailureReason {
    fn render(&self) -> String {
        match self {
            Self::StopFailed(error) => format!("stop failed: {error}"),
            Self::StillExists => "container still exists after stop".to_string(),
            Self::VerificationFailed(error) => {
                format!("follow-up inspect failed: {error}")
            }
        }
    }
}

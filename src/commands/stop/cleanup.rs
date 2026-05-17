// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::podman::Podman;
use crate::session::{
    SessionGroup, SessionRecord, discover_agentbox_containers, exact_git_root_matches,
    partition_sessions_by_git_root, select_agentbox_stable_id_prefix,
};
use crate::{Error, Result};

use super::super::container_cleanup::{ContainerCleanupFailure, cleanup_managed_containers};
use super::super::workspace_flow::{LockedGitRoot, with_locked_git_root};
use super::target::{StopTarget, StopTargetInput, resolve_stop_target};

pub(super) fn stop_target(target: &StopTargetInput, force: bool) -> Result<()> {
    match resolve_stop_target(target)? {
        StopTarget::ResolvedGitRoot(git_root) => stop_git_root(&git_root, force, target),
        StopTarget::ExactStoredGitRootPath(git_root) => {
            stop_exact_stored_git_root_path(&git_root, force, target)
        }
        StopTarget::StableId(prefix) => stop_stable_id(&prefix, force, target),
    }
}

pub(super) fn stop_all_running() -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_agentbox_containers(&podman)?
        .into_iter()
        .filter(|session| session.container_running())
        .collect::<Vec<_>>();
    let failures = cleanup_all_running_matches(sessions)?;

    finish_cleanup("all running agentbox containers", &failures)
}

fn stop_git_root(git_root: &Utf8Path, force: bool, target: &StopTargetInput) -> Result<()> {
    stop_exact_git_root_matches(git_root, force, target, ExactGitRootStopMode::Scoped)
}

fn stop_exact_stored_git_root_path(
    git_root: &Utf8Path,
    force: bool,
    target: &StopTargetInput,
) -> Result<()> {
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

    fn discover_sessions(self, locked: &LockedGitRoot<'_>) -> Result<Vec<SessionRecord>> {
        match self {
            Self::Scoped => locked.discover_sessions(),
            Self::ExactStoredPath => locked.discover_agentbox_containers(),
        }
    }
}

fn stop_exact_git_root_matches(
    git_root: &Utf8Path,
    force: bool,
    target: &StopTargetInput,
    mode: ExactGitRootStopMode,
) -> Result<()> {
    let failures = with_locked_git_root(git_root, |locked| {
        let sessions = exact_git_root_matches(mode.discover_sessions(&locked)?, git_root);
        let target_ref = mode.target_ref(git_root);

        if sessions.is_empty() {
            return Err(Error::msg(format!(
                "no agentbox container exists for {target_ref}"
            )));
        }

        require_force_for_duplicate_matches(sessions.len() > 1, force, &target_ref, target)?;

        cleanup_selected_sessions(&locked, &sessions)
    })?;

    finish_cleanup(git_root.as_str(), &failures)
}

fn stop_stable_id(prefix: &str, force: bool, target: &StopTargetInput) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_agentbox_containers(&podman)?;
    let selection = select_agentbox_stable_id_prefix(&sessions, prefix)?;
    let id = selection.id().to_string();

    require_force_for_duplicate_matches(
        selection.has_duplicate_sessions(),
        force,
        &format!("stable id `{id}`"),
        target,
    )?;

    let sessions = lockable_stable_id_matches(selection.into_sessions(), &id)?;
    let failures = cleanup_stable_id_matches(sessions)?;

    finish_cleanup(&format!("id {id}"), &failures)
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
            "agentbox container id `{id}` cannot be stopped safely because no matched container has a recoverable git-root label"
        )));
    }

    if lockable.len() != selected_count {
        return Err(Error::msg(format!(
            "agentbox container id `{id}` includes matched containers without a recoverable git-root label; cannot stop them safely"
        )));
    }

    Ok(lockable)
}

fn require_force_for_duplicate_matches(
    has_duplicates: bool,
    force: bool,
    target_ref: &str,
    target: &StopTargetInput,
) -> Result<()> {
    if has_duplicates && !force {
        Err(Error::msg(format!(
            "duplicate agentbox containers exist for {target_ref}; rerun `agentbox stop --force {}` to remove all exact matches",
            target.display()
        )))
    } else {
        Ok(())
    }
}

fn cleanup_stable_id_matches(sessions: Vec<SessionRecord>) -> Result<Vec<ContainerCleanupFailure>> {
    let partition = partition_sessions_by_git_root(sessions);

    cleanup_rooted_session_groups(partition.rooted, RootedSessionCleanupScope::Selected)
}

fn cleanup_all_running_matches(
    sessions: Vec<SessionRecord>,
) -> Result<Vec<ContainerCleanupFailure>> {
    let partition = partition_sessions_by_git_root(sessions);

    let mut failures = cleanup_rooted_session_groups(
        partition.rooted,
        RootedSessionCleanupScope::RunningExactMatches,
    )?;

    if !partition.unrooted.is_empty() {
        let podman = Podman::new();
        failures.extend(cleanup_managed_containers(
            &podman,
            partition.unrooted.iter(),
        ));
    }

    Ok(failures)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootedSessionCleanupScope {
    Selected,
    RunningExactMatches,
}

impl RootedSessionCleanupScope {
    fn cleanup(
        self,
        locked: &LockedGitRoot<'_>,
        selected_sessions: &[SessionRecord],
    ) -> Result<Vec<ContainerCleanupFailure>> {
        match self {
            Self::Selected => cleanup_selected_sessions(locked, selected_sessions),
            Self::RunningExactMatches => cleanup_running_exact_matches_for_locked_root(locked),
        }
    }
}

fn cleanup_rooted_session_groups(
    groups: Vec<SessionGroup>,
    scope: RootedSessionCleanupScope,
) -> Result<Vec<ContainerCleanupFailure>> {
    let mut failures = Vec::new();

    for group in groups {
        let git_root = group.canonical_git_root;
        let sessions = group.sessions;
        let mut group_failures =
            with_locked_git_root(&git_root, |locked| scope.cleanup(&locked, &sessions))?;
        failures.append(&mut group_failures);
    }

    Ok(failures)
}

fn cleanup_selected_sessions(
    locked: &LockedGitRoot<'_>,
    sessions: &[SessionRecord],
) -> Result<Vec<ContainerCleanupFailure>> {
    Ok(cleanup_managed_containers(locked.podman(), sessions.iter()))
}

fn cleanup_running_exact_matches_for_locked_root(
    locked: &LockedGitRoot<'_>,
) -> Result<Vec<ContainerCleanupFailure>> {
    // Re-discover under the git-root lock so `stop --all` only removes
    // containers that are still exact matches when cleanup starts.
    let sessions = exact_git_root_matches(locked.discover_sessions()?, locked.git_root());
    Ok(cleanup_managed_containers(
        locked.podman(),
        sessions
            .iter()
            .filter(|session| session.container_running()),
    ))
}

fn finish_cleanup(identity: &str, failures: &[ContainerCleanupFailure]) -> Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_cleanup_failures(identity, failures)))
    }
}

fn render_cleanup_failures(identity: &str, failures: &[ContainerCleanupFailure]) -> String {
    let details = failures
        .iter()
        .map(ContainerCleanupFailure::render_stop_message)
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "partial stop failed for `{identity}`; remaining agentbox containers: {details}. podman-owned image cleanup and cache volumes are left untouched"
    )
}

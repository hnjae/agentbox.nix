// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::podman::Podman;
use crate::session::{
    SessionDiscoveryQuery, SessionGroup, SessionRecord, SessionTargetInput, StopExactGitRootTarget,
    StopSessionTargetPlan, StopStableIdTarget, exact_git_root_matches,
    partition_sessions_by_git_root,
};
use crate::{Error, Result};

use super::super::container_cleanup::{ContainerCleanupFailure, cleanup_managed_containers};
use super::super::workspace_flow::{LockedGitRoot, with_locked_git_root};

pub(super) fn stop_target(target: &SessionTargetInput, force: bool) -> Result<()> {
    let plan = StopSessionTargetPlan::resolve(target, || {
        let podman = Podman::new();
        SessionDiscoveryQuery::agentbox_containers().discover(&podman)
    })?;

    match plan {
        StopSessionTargetPlan::ExactGitRoot(plan) => {
            stop_exact_git_root_matches(plan, force, target)
        }
        StopSessionTargetPlan::StableId(plan) => stop_stable_id(plan, force, target),
    }
}

pub(super) fn stop_all_running() -> Result<()> {
    let podman = Podman::new();
    let sessions = SessionDiscoveryQuery::agentbox_containers()
        .discover(&podman)?
        .into_iter()
        .filter(|session| session.container_running())
        .collect::<Vec<_>>();
    let failures = cleanup_all_running_matches(sessions)?;

    finish_cleanup("all running agentbox containers", &failures)
}

fn stop_exact_git_root_matches(
    plan: StopExactGitRootTarget,
    force: bool,
    target: &SessionTargetInput,
) -> Result<()> {
    let failures = with_locked_git_root(plan.git_root(), |locked| {
        let discovered_sessions = if plan.uses_locked_root_scope() {
            locked.discover_sessions()
        } else {
            locked.discover_agentbox_containers()
        }?;
        let sessions = plan.exact_matches_from(discovered_sessions);

        plan.require_non_empty_matches(&sessions)?;
        plan.require_force_for_matches(sessions.len(), force, target)?;

        cleanup_selected_sessions(&locked, &sessions)
    })?;

    finish_cleanup(plan.git_root().as_str(), &failures)
}

fn stop_stable_id(
    plan: StopStableIdTarget,
    force: bool,
    target: &SessionTargetInput,
) -> Result<()> {
    plan.require_force(force, target)?;
    let id = plan.id().to_string();
    let failures = cleanup_stable_id_matches(plan.into_sessions())?;

    finish_cleanup(&format!("id {id}"), &failures)
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

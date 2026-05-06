// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::StopArgs;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{
    SessionRecord, SessionStatus, discover_managed_sessions, select_stable_id_prefix,
};
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

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
        eprintln!("agentbox stop: no managed sessions available to stop");
        return Ok(Vec::new());
    }

    let selected = prompt::select_many("Select sessions to stop", candidates, non_tty_error)?;
    if selected.is_empty() {
        eprintln!("agentbox stop: no sessions selected");
        return Ok(Vec::new());
    }

    let mut targets = selected
        .into_iter()
        .map(|candidate| candidate.target)
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
        StopTarget::GitRoot(git_root) => stop_git_root(&git_root, force, target),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopPromptCandidate {
    label: String,
    target: PathBuf,
}

impl StopPromptCandidate {
    fn new(label: String, target: PathBuf) -> Self {
        Self { label, target }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }
}

impl fmt::Display for StopPromptCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub fn stop_prompt_candidates(sessions: &[SessionRecord]) -> Vec<StopPromptCandidate> {
    let mut candidates = sessions
        .iter()
        .filter(|session| stop_prompt_candidate_matches(session))
        .filter_map(stop_prompt_candidate)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates
}

fn stop_prompt_candidate_matches(session: &SessionRecord) -> bool {
    session.stable_id().is_some()
        && matches!(
            session.status,
            SessionStatus::Running
                | SessionStatus::Orphaned
                | SessionStatus::Duplicate
                | SessionStatus::Failed(_)
        )
}

fn stop_prompt_candidate(session: &SessionRecord) -> Option<StopPromptCandidate> {
    let id = session.stable_id()?;
    let root = session
        .canonical_git_root()
        .map_or("unknown", |root| root.as_str());
    let runtime = session.runtime().unwrap_or("unknown");
    let label = format!("{id} {root} {runtime} {}", session.status.as_str());

    Some(StopPromptCandidate::new(label, PathBuf::from(id)))
}

enum StopTarget {
    GitRoot(Utf8PathBuf),
    StableId(String),
}

fn stop_git_root(git_root: &Utf8Path, force: bool, target: &Path) -> Result<()> {
    let failures = with_locked_git_root(git_root, |locked| {
        let sessions = exact_full_root_matches(locked.discover_sessions()?, locked.git_root());

        if sessions.is_empty() {
            return Err(Error::msg(format!(
                "no managed session exists for `{git_root}`"
            )));
        }

        if sessions.len() > 1 && !force {
            return Err(Error::msg(format!(
                "duplicate managed sessions exist for `{git_root}`; rerun `agentbox stop --force {}` to remove all exact matches",
                target.display()
            )));
        }

        let failures = sessions
            .iter()
            .filter_map(|session| cleanup_managed_container(locked.podman(), session))
            .collect::<Vec<_>>();

        Ok(failures)
    })?;

    finish_cleanup(git_root.as_str(), &failures)
}

fn stop_stable_id(prefix: &str, force: bool, target: &Path) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    let selection = select_stable_id_prefix(&sessions, prefix)?;
    let id = selection.id().to_string();

    if selection.sessions().len() > 1 && !force {
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
            .map(|workspace| StopTarget::GitRoot(workspace.canonical_git_root));
    }

    if target.is_absolute() {
        let git_root = Utf8PathBuf::from_path_buf(target.to_path_buf())
            .map_err(|path| Error::msg(format!("non-utf8 path: {path:?}")))?;
        return Ok(StopTarget::GitRoot(git_root));
    }

    let prefix = target.to_str().ok_or_else(|| {
        Error::msg(format!(
            "non-utf8 target `{}` cannot be used as a stable id prefix",
            target.display()
        ))
    })?;

    Ok(StopTarget::StableId(prefix.to_string()))
}

fn exact_full_root_matches(
    sessions: Vec<SessionRecord>,
    git_root: &Utf8Path,
) -> Vec<SessionRecord> {
    sessions
        .into_iter()
        .filter(|session| session.canonical_git_root() == Some(git_root))
        .collect()
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
    let mut groups = BTreeMap::<Utf8PathBuf, Vec<SessionRecord>>::new();

    for session in sessions {
        if let Some(root) = session.canonical_git_root() {
            groups.entry(root.to_path_buf()).or_default().push(session);
        }
    }

    let mut failures = Vec::new();
    for (git_root, sessions) in groups {
        let mut group_failures = with_locked_git_root(&git_root, |locked| {
            Ok(sessions
                .iter()
                .filter_map(|session| cleanup_managed_container(locked.podman(), session))
                .collect::<Vec<_>>())
        })?;
        failures.append(&mut group_failures);
    }

    Ok(failures)
}

fn cleanup_all_running_matches(sessions: Vec<SessionRecord>) -> Result<Vec<CleanupFailure>> {
    let mut groups = BTreeMap::<Utf8PathBuf, Vec<SessionRecord>>::new();
    let mut unrooted = Vec::new();

    for session in sessions {
        if let Some(root) = session.canonical_git_root() {
            groups.entry(root.to_path_buf()).or_default().push(session);
        } else {
            unrooted.push(session);
        }
    }

    let mut failures = Vec::new();
    for (git_root, _) in groups {
        let mut group_failures = with_locked_git_root(&git_root, |locked| {
            Ok(
                exact_full_root_matches(locked.discover_sessions()?, locked.git_root())
                    .iter()
                    .filter(|session| session.container_running())
                    .filter_map(|session| cleanup_managed_container(locked.podman(), session))
                    .collect::<Vec<_>>(),
            )
        })?;
        failures.append(&mut group_failures);
    }

    if !unrooted.is_empty() {
        let podman = Podman::new();
        failures.extend(
            unrooted
                .iter()
                .filter_map(|session| cleanup_managed_container(&podman, session)),
        );
    }

    Ok(failures)
}

fn cleanup_managed_container(podman: &Podman, session: &SessionRecord) -> Option<CleanupFailure> {
    let mut reasons = Vec::new();
    if let Err(error) = podman.stop_ignore(&session.container_name) {
        reasons.push(CleanupFailureReason::StopFailed(error.to_string()));
    }

    match podman.container_exists(&session.container_name) {
        Ok(false) => None,
        Ok(true) => {
            reasons.push(CleanupFailureReason::StillExists);
            Some(CleanupFailure::new(&session.container_name, reasons))
        }
        Err(error) => {
            reasons.push(CleanupFailureReason::VerificationFailed(error.to_string()));
            Some(CleanupFailure::new(&session.container_name, reasons))
        }
    }
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

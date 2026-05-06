// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::StopArgs;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions, select_stable_id_prefix};
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

use super::workspace_flow::with_locked_git_root;

pub fn run(args: StopArgs) -> Result<()> {
    match resolve_stop_target(&args.target)? {
        StopTarget::GitRoot(git_root) => stop_git_root(&git_root, args.force, &args.target),
        StopTarget::StableId(prefix) => stop_stable_id(&prefix, args.force, &args.target),
    }
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

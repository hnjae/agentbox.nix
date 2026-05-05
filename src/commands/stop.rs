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
use crate::podman::Podman;
use crate::session::SessionRecord;
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

use super::workspace_flow::with_locked_git_root;

pub fn run(args: StopArgs) -> Result<()> {
    let git_root = resolve_stop_git_root(&args.directory)?;
    let failures = with_locked_git_root(git_root.as_ref(), |locked| {
        let sessions = exact_full_root_matches(locked.discover_sessions()?, locked.git_root());

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

        let failures = sessions
            .iter()
            .filter_map(|session| cleanup_managed_container(locked.podman(), session))
            .collect::<Vec<_>>();

        Ok(failures)
    })?;

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
        .filter(|session| session.canonical_git_root() == Some(git_root))
        .collect()
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

fn render_cleanup_failures(git_root: &Utf8Path, failures: &[CleanupFailure]) -> String {
    let details = failures
        .iter()
        .map(CleanupFailure::render)
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "partial stop failed for `{git_root}`; remaining managed containers: {details}. podman-owned image cleanup and cache volumes are left untouched"
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

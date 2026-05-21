// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::session::{SessionRecord, exact_git_root_matches};
use crate::{Error, Result};

use super::{
    ResolvedSessionTarget, SessionTargetInput, StableIdTargetAction,
    select_agentbox_stable_id_target,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StopSessionTargetPlan {
    ExactGitRoot(StopExactGitRootTarget),
    StableId(StopStableIdTarget),
}

impl StopSessionTargetPlan {
    pub(crate) fn resolve(
        target: &SessionTargetInput,
        discover_agentbox_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
    ) -> Result<Self> {
        match target.resolve()? {
            ResolvedSessionTarget::ResolvedGitRoot(git_root) => {
                Ok(Self::ExactGitRoot(StopExactGitRootTarget {
                    git_root,
                    mode: StopExactGitRootTargetMode::Scoped,
                }))
            }
            ResolvedSessionTarget::ExactStoredGitRootPath(git_root) => {
                Ok(Self::ExactGitRoot(StopExactGitRootTarget {
                    git_root,
                    mode: StopExactGitRootTargetMode::ExactStoredPath,
                }))
            }
            ResolvedSessionTarget::StableId(prefix) => {
                let sessions = discover_agentbox_sessions()?;
                let selection = select_agentbox_stable_id_target(&sessions, &prefix)?;
                let id = selection.id().to_string();
                let has_duplicate_sessions = selection.has_duplicate_sessions();
                let sessions = selection.into_all_rooted_sessions(StableIdTargetAction::Stop)?;

                Ok(Self::StableId(StopStableIdTarget {
                    id,
                    sessions,
                    has_duplicate_sessions,
                }))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StopExactGitRootTarget {
    git_root: Utf8PathBuf,
    mode: StopExactGitRootTargetMode,
}

impl StopExactGitRootTarget {
    pub(crate) fn git_root(&self) -> &Utf8Path {
        &self.git_root
    }

    pub(crate) fn uses_locked_root_scope(&self) -> bool {
        self.mode == StopExactGitRootTargetMode::Scoped
    }

    pub(crate) fn exact_matches_from(&self, sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
        exact_git_root_matches(sessions, &self.git_root)
    }

    pub(crate) fn require_non_empty_matches(&self, sessions: &[SessionRecord]) -> Result<()> {
        if sessions.is_empty() {
            Err(Error::msg(format!(
                "no agentbox container exists for {}",
                self.target_ref()
            )))
        } else {
            Ok(())
        }
    }

    pub(crate) fn require_force_for_matches(
        &self,
        match_count: usize,
        force: bool,
        target: &SessionTargetInput,
    ) -> Result<()> {
        require_force_for_duplicate_matches(match_count > 1, force, &self.target_ref(), target)
    }

    fn target_ref(&self) -> String {
        match self.mode {
            StopExactGitRootTargetMode::Scoped => format!("`{}`", self.git_root),
            StopExactGitRootTargetMode::ExactStoredPath => {
                format!("exact stored git-root path `{}`", self.git_root)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopExactGitRootTargetMode {
    Scoped,
    ExactStoredPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StopStableIdTarget {
    id: String,
    sessions: Vec<SessionRecord>,
    has_duplicate_sessions: bool,
}

impl StopStableIdTarget {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn into_sessions(self) -> Vec<SessionRecord> {
        self.sessions
    }

    pub(crate) fn require_force(&self, force: bool, target: &SessionTargetInput) -> Result<()> {
        require_force_for_duplicate_matches(
            self.has_duplicate_sessions,
            force,
            &format!("stable id `{}`", self.id),
            target,
        )
    }
}

fn require_force_for_duplicate_matches(
    has_duplicates: bool,
    force: bool,
    target_ref: &str,
    target: &SessionTargetInput,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::process::Command;

    use crate::metadata::{AgentboxContainerKind, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH};
    use crate::session::{SessionMetadata, SessionStatus};

    use super::*;

    #[test]
    fn existing_cli_target_becomes_scoped_exact_git_root_plan() {
        let sandbox = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(sandbox.path())
            .status()
            .unwrap();
        assert!(status.success());
        let target = SessionTargetInput::Cli(sandbox.path().to_path_buf());

        let StopSessionTargetPlan::ExactGitRoot(plan) =
            StopSessionTargetPlan::resolve(&target, || unreachable!()).unwrap()
        else {
            panic!("expected exact git-root stop plan");
        };

        assert!(plan.uses_locked_root_scope());
        let git_root = Utf8PathBuf::from_path_buf(sandbox.path().canonicalize().unwrap()).unwrap();
        assert_eq!(plan.git_root(), git_root);
    }

    #[test]
    fn missing_absolute_cli_target_becomes_exact_stored_path_plan() {
        let target = SessionTargetInput::Cli(PathBuf::from("/missing/workspace"));

        let StopSessionTargetPlan::ExactGitRoot(plan) =
            StopSessionTargetPlan::resolve(&target, || unreachable!()).unwrap()
        else {
            panic!("expected exact git-root stop plan");
        };

        assert!(!plan.uses_locked_root_scope());
        assert_eq!(plan.git_root(), Utf8Path::new("/missing/workspace"));
    }

    #[test]
    fn stable_id_plan_keeps_all_rooted_matching_sessions() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![
            session(Some("/workspace/project"), "abcdef123456", "first"),
            session(Some("/workspace/project"), "abcdef123456", "second"),
        ];

        let StopSessionTargetPlan::StableId(plan) =
            StopSessionTargetPlan::resolve(&target, || Ok(sessions)).unwrap()
        else {
            panic!("expected stable-id stop plan");
        };

        assert_eq!(plan.id(), "abcdef123456");
        assert_eq!(plan.clone().into_sessions().len(), 2);
        assert!(plan.require_force(false, &target).is_err());
        assert!(plan.require_force(true, &target).is_ok());
    }

    #[test]
    fn stable_id_plan_rejects_unrooted_matches() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![session(None, "abcdef123456", "unrooted")];

        let error = StopSessionTargetPlan::resolve(&target, || Ok(sessions)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("no matched container has a recoverable git-root label")
        );
    }

    fn session(canonical_git_root: Option<&str>, stable_id: &str, name: &str) -> SessionRecord {
        let mut labels = BTreeMap::from([(LABEL_GIT_ROOT_HASH.to_string(), stable_id.to_string())]);
        if let Some(canonical_git_root) = canonical_git_root {
            labels.insert(LABEL_GIT_ROOT.to_string(), canonical_git_root.to_string());
        }

        SessionRecord::new(
            format!("{name}-id"),
            name,
            AgentboxContainerKind::Managed,
            SessionMetadata::from_labels(&labels),
            None,
            true,
            SessionStatus::Running,
        )
    }
}

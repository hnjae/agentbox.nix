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
pub(crate) struct RestartSessionTargetPlan {
    input: SessionTargetInput,
    lock_git_root: Utf8PathBuf,
}

impl RestartSessionTargetPlan {
    pub(crate) fn resolve(
        target: &SessionTargetInput,
        discover_agentbox_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
    ) -> Result<Self> {
        let lock_git_root = match target.resolve()? {
            ResolvedSessionTarget::ResolvedGitRoot(git_root)
            | ResolvedSessionTarget::ExactStoredGitRootPath(git_root) => git_root,
            ResolvedSessionTarget::StableId(prefix) => {
                restart_lock_git_root_for_stable_id_from_sessions(
                    &discover_agentbox_sessions()?,
                    &prefix,
                )?
            }
        };

        Ok(Self {
            input: target.clone(),
            lock_git_root,
        })
    }

    pub(crate) fn lock_git_root(&self) -> &Utf8Path {
        &self.lock_git_root
    }

    pub(crate) fn select_session_candidate(
        &self,
        locked_git_root: &Utf8Path,
        discover_locked_root_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
        discover_agentbox_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
    ) -> Result<SessionRecord> {
        let sessions = self.matching_sessions(
            locked_git_root,
            discover_locked_root_sessions,
            discover_agentbox_sessions,
        )?;
        require_single_restart_target_match(sessions, &self.input)
    }

    fn matching_sessions(
        &self,
        locked_git_root: &Utf8Path,
        discover_locked_root_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
        discover_agentbox_sessions: impl FnOnce() -> Result<Vec<SessionRecord>>,
    ) -> Result<Vec<SessionRecord>> {
        match self.input.resolve()? {
            ResolvedSessionTarget::ResolvedGitRoot(git_root) => {
                require_locked_restart_target_unchanged(locked_git_root, &git_root)?;
                discover_locked_root_sessions()
            }
            ResolvedSessionTarget::ExactStoredGitRootPath(git_root) => {
                require_locked_restart_target_unchanged(locked_git_root, &git_root)?;
                Ok(exact_git_root_matches(
                    discover_agentbox_sessions()?,
                    &git_root,
                ))
            }
            ResolvedSessionTarget::StableId(prefix) => {
                let sessions =
                    select_agentbox_stable_id_target(&discover_agentbox_sessions()?, &prefix)?
                        .into_sessions();
                require_stable_id_still_matches_locked_root(locked_git_root, &sessions)?;
                Ok(sessions)
            }
        }
    }
}

fn restart_lock_git_root_for_stable_id_from_sessions(
    sessions: &[SessionRecord],
    prefix: &str,
) -> Result<Utf8PathBuf> {
    select_agentbox_stable_id_target(sessions, prefix)?
        .single_recoverable_git_root(StableIdTargetAction::Restart)
}

fn require_locked_restart_target_unchanged(locked: &Utf8Path, current: &Utf8Path) -> Result<()> {
    if locked == current {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "restart target changed from `{locked}` to `{current}` while waiting for the workspace lock; retry the command"
        )))
    }
}

fn require_stable_id_still_matches_locked_root(
    locked: &Utf8Path,
    sessions: &[SessionRecord],
) -> Result<()> {
    let matches_locked_root = sessions
        .iter()
        .any(|session| session.canonical_git_root() == Some(locked));
    if matches_locked_root {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "restart target changed away from `{locked}` while waiting for the workspace lock; retry the command"
        )))
    }
}

fn require_single_restart_target_match(
    sessions: Vec<SessionRecord>,
    target: &SessionTargetInput,
) -> Result<SessionRecord> {
    match sessions.as_slice() {
        [] => Err(Error::msg(format!(
            "no running managed session matches restart target `{}`",
            target.display()
        ))),
        [_] => Ok(sessions.into_iter().next().unwrap()),
        _ => Err(Error::msg(format!(
            "restart target `{}` matches {} agentbox containers; restart requires exactly one running managed session. Clean up duplicates with `agentbox stop --force {}` before retrying.",
            target.display(),
            sessions.len(),
            target.display(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::metadata::{AgentboxContainerKind, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH};
    use crate::session::{SessionMetadata, SessionStatus};

    use super::*;

    #[test]
    fn restart_target_plan_uses_the_single_recoverable_stable_id_git_root() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![session(Some("/workspace/project"), "abcdef123456")];

        let plan = RestartSessionTargetPlan::resolve(&target, || Ok(sessions)).unwrap();

        assert_eq!(plan.lock_git_root(), Utf8Path::new("/workspace/project"));
    }

    #[test]
    fn restart_target_plan_rejects_unrooted_stable_id_matches() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![session(None, "abcdef123456")];

        let error = RestartSessionTargetPlan::resolve(&target, || Ok(sessions)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("no matched container has a recoverable git-root label")
        );
    }

    #[test]
    fn restart_target_plan_rejects_stable_id_matches_across_multiple_git_roots() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![
            session(Some("/workspace/first"), "abcdef123456"),
            session(Some("/workspace/second"), "abcdef123456"),
        ];

        let error = RestartSessionTargetPlan::resolve(&target, || Ok(sessions)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("matches containers with multiple git roots")
        );
    }

    #[test]
    fn restart_target_plan_revalidates_stable_id_under_locked_root() {
        let target = SessionTargetInput::StableId("abcdef".to_string());
        let sessions = vec![session(Some("/workspace/project"), "abcdef123456")];
        let plan = RestartSessionTargetPlan::resolve(&target, || Ok(sessions.clone())).unwrap();

        let selected = plan
            .select_session_candidate(
                Utf8Path::new("/workspace/project"),
                || unreachable!("stable-id matching should discover all agentbox sessions"),
                || Ok(sessions.clone()),
            )
            .unwrap();
        assert_eq!(selected.container_name, "agentbox-abcdef123456");

        let error = plan
            .select_session_candidate(
                Utf8Path::new("/workspace/other"),
                || unreachable!("stable-id matching should discover all agentbox sessions"),
                || Ok(sessions),
            )
            .unwrap_err();

        assert!(error.to_string().contains("changed away"));
    }

    fn session(canonical_git_root: Option<&str>, stable_id: &str) -> SessionRecord {
        let mut labels = BTreeMap::from([(LABEL_GIT_ROOT_HASH.to_string(), stable_id.to_string())]);
        if let Some(canonical_git_root) = canonical_git_root {
            labels.insert(LABEL_GIT_ROOT.to_string(), canonical_git_root.to_string());
        }

        SessionRecord {
            container_id: format!("{stable_id}-id"),
            container_name: format!("agentbox-{stable_id}"),
            container_kind: AgentboxContainerKind::Managed,
            metadata: SessionMetadata::from_labels(&labels),
            attach_endpoint: None,
            container_running: true,
            status: SessionStatus::Running,
        }
    }
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::session::SessionRecord;
use crate::session::selection::select_agentbox_stable_id_prefix;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StableIdTargetSelection {
    id: String,
    sessions: Vec<SessionRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StableIdTargetAction {
    Stop,
    Restart,
}

impl StableIdTargetAction {
    fn verb(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    fn safety_verb(self) -> &'static str {
        match self {
            Self::Stop => "stopped",
            Self::Restart => "restarted",
        }
    }
}

impl StableIdTargetSelection {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn has_duplicate_sessions(&self) -> bool {
        self.sessions.len() > 1
    }

    pub(crate) fn single_recoverable_git_root(
        &self,
        action: StableIdTargetAction,
    ) -> Result<Utf8PathBuf> {
        let roots = recoverable_git_roots(&self.sessions);

        match roots.as_slice() {
            [] => Err(no_recoverable_git_root_error(&self.id, action)),
            [root] => Ok(root.clone()),
            _ => Err(Error::msg(format!(
                "agentbox container id `{}` matches containers with multiple git roots; cannot {action} safely",
                self.id,
                action = action.verb(),
            ))),
        }
    }

    pub(crate) fn into_all_rooted_sessions(
        self,
        action: StableIdTargetAction,
    ) -> Result<Vec<SessionRecord>> {
        let selected_count = self.sessions.len();
        let rooted = self
            .sessions
            .into_iter()
            .filter(|session| session.canonical_git_root().is_some())
            .collect::<Vec<_>>();

        if rooted.is_empty() {
            return Err(no_recoverable_git_root_error(&self.id, action));
        }

        if rooted.len() != selected_count {
            return Err(Error::msg(format!(
                "agentbox container id `{}` includes matched containers without a recoverable git-root label; cannot {verb} them safely",
                self.id,
                verb = action.verb(),
            )));
        }

        Ok(rooted)
    }

    pub(crate) fn into_sessions(self) -> Vec<SessionRecord> {
        self.sessions
    }
}

pub(crate) fn select_agentbox_stable_id_target(
    sessions: &[SessionRecord],
    prefix: &str,
) -> Result<StableIdTargetSelection> {
    let selection = select_agentbox_stable_id_prefix(sessions, prefix)?;
    let id = selection.id().to_string();
    let sessions = selection.into_sessions().into_iter().cloned().collect();

    Ok(StableIdTargetSelection { id, sessions })
}

fn recoverable_git_roots(sessions: &[SessionRecord]) -> Vec<Utf8PathBuf> {
    let mut roots = sessions
        .iter()
        .filter_map(|session| session.canonical_git_root().map(Utf8Path::to_path_buf))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

fn no_recoverable_git_root_error(id: &str, action: StableIdTargetAction) -> Error {
    Error::msg(format!(
        "agentbox container id `{id}` cannot be {action} safely because no matched container has a recoverable git-root label",
        action = action.safety_verb(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::metadata::{AgentboxContainerKind, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH};
    use crate::session::{SessionMetadata, SessionStatus};

    use super::*;

    #[test]
    fn stable_id_target_returns_the_single_recoverable_git_root() {
        let sessions = vec![session(Some("/workspace/project"), "abcdef123456")];
        let selection = select_agentbox_stable_id_target(&sessions, "abcdef").unwrap();

        assert_eq!(
            selection
                .single_recoverable_git_root(StableIdTargetAction::Restart)
                .unwrap(),
            Utf8PathBuf::from("/workspace/project")
        );
    }

    #[test]
    fn stable_id_target_rejects_single_root_when_only_unrooted_matches_exist() {
        let sessions = vec![session(None, "abcdef123456")];
        let selection = select_agentbox_stable_id_target(&sessions, "abcdef").unwrap();

        let error = selection
            .single_recoverable_git_root(StableIdTargetAction::Restart)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("no matched container has a recoverable git-root label")
        );
    }

    #[test]
    fn stable_id_target_rejects_single_root_when_multiple_roots_match() {
        let sessions = vec![
            session(Some("/workspace/first"), "abcdef123456"),
            session(Some("/workspace/second"), "abcdef123456"),
        ];
        let selection = select_agentbox_stable_id_target(&sessions, "abcdef").unwrap();

        let error = selection
            .single_recoverable_git_root(StableIdTargetAction::Restart)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("matches containers with multiple git roots")
        );
    }

    #[test]
    fn stable_id_target_requires_all_sessions_to_be_rooted_for_group_cleanup() {
        let sessions = vec![
            session(Some("/workspace/project"), "abcdef123456"),
            session(None, "abcdef123456"),
        ];
        let selection = select_agentbox_stable_id_target(&sessions, "abcdef").unwrap();

        let error = selection
            .into_all_rooted_sessions(StableIdTargetAction::Stop)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("includes matched containers without a recoverable git-root label")
        );
    }

    #[test]
    fn stable_id_target_keeps_all_rooted_matches_for_group_cleanup() {
        let sessions = vec![
            session(Some("/workspace/project"), "abcdef123456"),
            session(Some("/workspace/project"), "abcdef123456"),
        ];
        let selection = select_agentbox_stable_id_target(&sessions, "abcdef").unwrap();

        let rooted = selection
            .into_all_rooted_sessions(StableIdTargetAction::Stop)
            .unwrap();

        assert_eq!(rooted.len(), 2);
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

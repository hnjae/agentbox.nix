// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use camino::Utf8PathBuf;

use crate::session::SessionRecord;
use crate::session::selection::select_stable_id_prefix;
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HealthSessionTargetPlan {
    RunningSessions,
    StableIdPrefix(String),
    Workspace(Utf8PathBuf),
}

impl HealthSessionTargetPlan {
    pub(crate) fn from_target(target: Option<&str>) -> Result<Self> {
        match target {
            Some(target) if Path::new(target).exists() => {
                let workspace = resolve_workspace_identity(target)?;
                Ok(Self::Workspace(workspace.canonical_git_root))
            }
            Some(target) => Ok(Self::StableIdPrefix(target.to_string())),
            None => Ok(Self::RunningSessions),
        }
    }

    pub(crate) fn select_sessions<'a>(
        &self,
        sessions: &'a [SessionRecord],
    ) -> Result<Vec<&'a SessionRecord>> {
        match self {
            Self::RunningSessions => Ok(sessions
                .iter()
                .filter(|session| session.is_running())
                .collect()),
            Self::StableIdPrefix(prefix) => select_stable_id_health_session(sessions, prefix),
            Self::Workspace(git_root) => select_workspace_health_session(sessions, git_root),
        }
    }
}

fn select_workspace_health_session<'a>(
    sessions: &'a [SessionRecord],
    git_root: &camino::Utf8Path,
) -> Result<Vec<&'a SessionRecord>> {
    let matches = sessions
        .iter()
        .filter(|session| session.canonical_git_root() == Some(git_root))
        .collect::<Vec<_>>();

    let Some(session) = single_workspace_match(git_root, matches)? else {
        return Err(Error::msg(format!(
            "no running managed session matches target `{git_root}`"
        )));
    };
    if !session.is_running() {
        return Err(Error::msg(format!(
            "managed session `{}` is `{}`; health only probes running sessions",
            session.stable_id().unwrap_or(git_root.as_str()),
            session.status().as_str()
        )));
    }

    Ok(vec![session])
}

fn single_workspace_match<'a>(
    git_root: &camino::Utf8Path,
    matches: Vec<&'a SessionRecord>,
) -> Result<Option<&'a SessionRecord>> {
    match matches.as_slice() {
        [] => Ok(None),
        [session] => Ok(Some(*session)),
        _ => Err(Error::msg(format!(
            "workspace target `{git_root}` matches multiple managed sessions; health requires a single running session"
        ))),
    }
}

fn select_stable_id_health_session<'a>(
    sessions: &'a [SessionRecord],
    prefix: &str,
) -> Result<Vec<&'a SessionRecord>> {
    let selection = select_stable_id_prefix(sessions, prefix)?;
    let selection_id = selection.id().to_string();
    let Some(session) = selection.into_single_session() else {
        return Err(Error::msg(format!(
            "stable id `{selection_id}` matches multiple managed sessions; health requires a single running session",
        )));
    };
    if !session.is_running() {
        return Err(Error::msg(format!(
            "managed session `{}` is `{}`; health only probes running sessions",
            session.stable_id().unwrap_or(&selection_id),
            session.status().as_str()
        )));
    }

    Ok(vec![session])
}

#[cfg(test)]
mod tests {
    use crate::session::SessionStatus;
    use crate::session::test_support::SessionRecordFixture;

    use super::*;

    #[test]
    fn running_sessions_plan_filters_out_non_running_sessions() {
        let sessions = vec![
            session("running", "abcdef123456", SessionStatus::Running),
            session("stopped", "fedcba654321", SessionStatus::failed_unknown()),
            session("failed", "aaaaaa111111", SessionStatus::failed_unknown()),
        ];

        let selected = HealthSessionTargetPlan::RunningSessions
            .select_sessions(&sessions)
            .unwrap()
            .into_iter()
            .map(|session| session.container_name())
            .collect::<Vec<_>>();

        assert_eq!(selected, ["running"]);
    }

    #[test]
    fn stable_id_prefix_plan_selects_one_running_session() {
        let sessions = vec![
            session("selected", "abcdef123456", SessionStatus::Running),
            session("other", "fedcba654321", SessionStatus::Running),
        ];

        let selected = HealthSessionTargetPlan::from_target(Some("abc"))
            .unwrap()
            .select_sessions(&sessions)
            .unwrap();

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].container_name(), "selected");
    }

    #[test]
    fn stable_id_prefix_plan_rejects_duplicate_sessions_for_one_id() {
        let sessions = vec![
            session("first", "abcdef123456", SessionStatus::Running),
            session("second", "abcdef123456", SessionStatus::Running),
        ];

        let error = HealthSessionTargetPlan::from_target(Some("abc"))
            .unwrap()
            .select_sessions(&sessions)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("health requires a single running session")
        );
    }

    #[test]
    fn stable_id_prefix_plan_rejects_non_running_session() {
        let sessions = vec![session(
            "stopped",
            "abcdef123456",
            SessionStatus::failed_unknown(),
        )];

        let error = HealthSessionTargetPlan::from_target(Some("abc"))
            .unwrap()
            .select_sessions(&sessions)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("health only probes running sessions")
        );
    }

    fn session(container_name: &str, stable_id: &str, status: SessionStatus) -> SessionRecord {
        SessionRecordFixture::managed(stable_id)
            .named(container_name)
            .status(status)
            .build()
    }
}

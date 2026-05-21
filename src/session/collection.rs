// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use super::record::{SessionGroup, SessionRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRootPartition {
    pub(crate) rooted: Vec<SessionGroup>,
    pub(crate) unrooted: Vec<SessionRecord>,
}

pub(crate) fn sorted_session_refs_by_identity<'a>(
    sessions: impl IntoIterator<Item = &'a SessionRecord>,
) -> Vec<&'a SessionRecord> {
    let mut sessions = sessions.into_iter().collect::<Vec<_>>();
    sort_session_refs_by_identity(&mut sessions);
    sessions
}

pub(crate) fn sort_session_refs_by_identity(sessions: &mut [&SessionRecord]) {
    sessions.sort_by(|left, right| compare_sessions_by_identity(left, right));
}

pub(crate) fn exact_git_root_matches(
    sessions: impl IntoIterator<Item = SessionRecord>,
    git_root: &Utf8Path,
) -> Vec<SessionRecord> {
    sessions
        .into_iter()
        .filter(|session| session.canonical_git_root() == Some(git_root))
        .collect()
}

pub fn group_sessions_by_git_root(sessions: &[SessionRecord]) -> Vec<SessionGroup> {
    partition_sessions_by_git_root(sessions.iter().cloned()).rooted
}

pub(crate) fn partition_sessions_by_git_root(
    sessions: impl IntoIterator<Item = SessionRecord>,
) -> SessionRootPartition {
    let mut groups = BTreeMap::<Utf8PathBuf, Vec<SessionRecord>>::new();
    let mut unrooted = Vec::new();

    for session in sessions {
        if let Some(root) = session.canonical_git_root() {
            groups.entry(root.to_path_buf()).or_default().push(session);
        } else {
            unrooted.push(session);
        }
    }

    SessionRootPartition {
        rooted: groups
            .into_iter()
            .map(|(canonical_git_root, sessions)| SessionGroup {
                canonical_git_root,
                sessions,
            })
            .collect(),
        unrooted,
    }
}

fn compare_sessions_by_identity(left: &SessionRecord, right: &SessionRecord) -> Ordering {
    left.canonical_git_root()
        .map(|root| root.as_str())
        .cmp(&right.canonical_git_root().map(|root| root.as_str()))
        .then_with(|| left.container_name().cmp(right.container_name()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::metadata::{AgentboxContainerKind, LABEL_GIT_ROOT};
    use crate::session::{SessionMetadata, SessionStatus};

    use super::*;

    #[test]
    fn sorted_session_refs_by_identity_orders_by_root_then_container() {
        let sessions = vec![
            session(Some("/workspace/b"), "beta"),
            session(Some("/workspace/a"), "second"),
            session(None, "unknown"),
            session(Some("/workspace/a"), "first"),
        ];

        let sorted = sorted_session_refs_by_identity(&sessions)
            .into_iter()
            .map(|session| session.container_name())
            .collect::<Vec<_>>();

        assert_eq!(sorted, ["unknown", "first", "second", "beta"]);
    }

    #[test]
    fn exact_git_root_matches_requires_the_full_canonical_root() {
        let sessions = vec![
            session(Some("/workspace/a"), "match"),
            session(Some("/workspace/ab"), "prefix-only"),
            session(None, "unknown"),
        ];

        let matches = exact_git_root_matches(sessions, Utf8Path::new("/workspace/a"))
            .into_iter()
            .map(|session| session.container_name().to_string())
            .collect::<Vec<_>>();

        assert_eq!(matches, ["match"]);
    }

    #[test]
    fn partition_sessions_by_git_root_keeps_unrooted_sessions_separate() {
        let partition = partition_sessions_by_git_root(vec![
            session(Some("/workspace/b"), "beta"),
            session(None, "unknown"),
            session(Some("/workspace/a"), "alpha"),
        ]);

        let roots = partition
            .rooted
            .iter()
            .map(|group| group.canonical_git_root.as_str())
            .collect::<Vec<_>>();
        let unrooted = partition
            .unrooted
            .iter()
            .map(|session| session.container_name())
            .collect::<Vec<_>>();

        assert_eq!(roots, ["/workspace/a", "/workspace/b"]);
        assert_eq!(unrooted, ["unknown"]);
    }

    fn session(root: Option<&str>, container_name: &str) -> SessionRecord {
        let labels = root.map_or_else(BTreeMap::new, |root| {
            BTreeMap::from([(LABEL_GIT_ROOT.to_string(), root.to_string())])
        });

        SessionRecord::new(
            format!("{container_name}-id"),
            container_name,
            AgentboxContainerKind::Managed,
            SessionMetadata::from_labels(&labels),
            None,
            false,
            SessionStatus::failed_unknown(),
        )
    }
}

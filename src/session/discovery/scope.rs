// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::metadata::{
    AgentboxContainerKind, LABEL_GIT_ROOT_HASH, agentbox_container_kind_from_labels,
    required_label_value,
};
use crate::podman::PodmanPsContainer;
use crate::workspace::git_root_hash12;
use crate::{Error, Result};

use super::super::record::SessionRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContainerDiscoveryScope {
    ManagedSessions,
    AgentboxOwned,
}

impl ContainerDiscoveryScope {
    fn includes(self, kind: AgentboxContainerKind) -> bool {
        match self {
            Self::ManagedSessions => kind == AgentboxContainerKind::Managed,
            Self::AgentboxOwned => true,
        }
    }
}

pub(super) enum SessionDiscoveryScope<'a> {
    All,
    GitRoot(GitRootDiscoveryScope<'a>),
}

impl<'a> SessionDiscoveryScope<'a> {
    pub(super) fn for_git_root(git_root: &'a Utf8Path) -> Self {
        Self::GitRoot(GitRootDiscoveryScope::new(git_root))
    }

    pub(super) fn should_inspect_ps_candidate(&self, container: &PodmanPsContainer) -> bool {
        match self {
            Self::All => true,
            Self::GitRoot(git_root) => git_root.should_inspect_ps_candidate(container),
        }
    }
}

pub(super) struct GitRootDiscoveryScope<'a> {
    git_root: &'a Utf8Path,
    git_root_hash: String,
}

impl<'a> GitRootDiscoveryScope<'a> {
    fn new(git_root: &'a Utf8Path) -> Self {
        Self {
            git_root,
            git_root_hash: git_root_hash12(git_root),
        }
    }

    fn should_inspect_ps_candidate(&self, container: &PodmanPsContainer) -> bool {
        required_label_value(&container.labels, LABEL_GIT_ROOT_HASH)
            .is_none_or(|git_root_hash| git_root_hash == self.git_root_hash)
    }

    fn classify_session(&self, record: SessionRecord) -> ScopedSessionCandidate {
        let hash_matches = record.git_root_hash() == Some(self.git_root_hash.as_str());

        if record.canonical_git_root() == Some(self.git_root) {
            return ScopedSessionCandidate::Match(record);
        }

        if !hash_matches {
            return ScopedSessionCandidate::Ignore;
        }

        match record.canonical_git_root() {
            Some(root) => ScopedSessionCandidate::Collision(root.to_string()),
            None => ScopedSessionCandidate::Match(record),
        }
    }

    fn collision_error(&self, mismatched_roots: &[String]) -> Error {
        Error::msg(format!(
            "managed identity collision for `{}` matched different full git roots: {}",
            self.git_root,
            mismatched_roots.join(", ")
        ))
    }
}

enum ScopedSessionCandidate {
    Match(SessionRecord),
    Collision(String),
    Ignore,
}

pub(super) struct SessionCollector<'a> {
    scope: SessionDiscoveryScope<'a>,
    sessions: Vec<SessionRecord>,
    mismatched_roots: Vec<String>,
}

impl<'a> SessionCollector<'a> {
    pub(super) fn new(scope: SessionDiscoveryScope<'a>) -> Self {
        Self {
            scope,
            sessions: Vec::new(),
            mismatched_roots: Vec::new(),
        }
    }

    pub(super) fn should_inspect_ps_candidate(&self, container: &PodmanPsContainer) -> bool {
        self.scope.should_inspect_ps_candidate(container)
    }

    pub(super) fn collect(&mut self, record: SessionRecord) {
        match &self.scope {
            SessionDiscoveryScope::All => self.sessions.push(record),
            SessionDiscoveryScope::GitRoot(git_root) => match git_root.classify_session(record) {
                ScopedSessionCandidate::Match(record) => self.sessions.push(record),
                ScopedSessionCandidate::Collision(root) => self.mismatched_roots.push(root),
                ScopedSessionCandidate::Ignore => {}
            },
        }
    }

    pub(super) fn finish(mut self) -> Result<Vec<SessionRecord>> {
        if let SessionDiscoveryScope::GitRoot(git_root) = &self.scope {
            self.mismatched_roots.sort();
            self.mismatched_roots.dedup();

            if !self.mismatched_roots.is_empty() {
                return Err(git_root.collision_error(&self.mismatched_roots));
            }
        }

        Ok(mark_duplicate_sessions(self.sessions))
    }
}

pub(super) fn ps_candidate(
    container: PodmanPsContainer,
    container_scope: ContainerDiscoveryScope,
) -> Option<(PodmanPsContainer, AgentboxContainerKind)> {
    let kind = agentbox_container_kind_from_labels(&container.labels)?;
    if container_scope.includes(kind) {
        Some((container, kind))
    } else {
        None
    }
}

fn mark_duplicate_sessions(mut sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
    let mut group_sizes = BTreeMap::<camino::Utf8PathBuf, usize>::new();

    for session in &sessions {
        if session.status().is_failed() {
            continue;
        }

        if let Some(root) = session.canonical_git_root() {
            *group_sizes.entry(root.to_path_buf()).or_default() += 1;
        }
    }

    for session in &mut sessions {
        if session.status().is_failed() {
            continue;
        }

        if session
            .canonical_git_root()
            .and_then(|root| group_sizes.get(root))
            .is_some_and(|count| *count > 1)
        {
            session.mark_duplicate();
        }
    }

    sessions
}

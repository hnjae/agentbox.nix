// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::metadata::{
    AgentboxContainerKind, LABEL_GIT_ROOT_HASH, agentbox_container_kind_from_labels,
    required_label_value,
};
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount, PodmanPsContainer};
use crate::workspace::git_root_hash12;
use crate::{Error, Result};

use super::endpoint::AttachEndpointReport;
use super::labels::SessionLabelReport;
use super::record::{SessionMetadata, SessionRecord, SessionRecordInput};
use super::status::{
    GitRootProbe, HostGitRootProbe, SessionStatusInput, derive_status, mark_duplicate_sessions,
};

pub struct SessionDiscoveryQuery<'a> {
    scope: SessionDiscoveryScope<'a>,
    container_scope: ContainerDiscoveryScope,
}

impl<'a> SessionDiscoveryQuery<'a> {
    pub fn managed_sessions() -> Self {
        Self {
            scope: SessionDiscoveryScope::All,
            container_scope: ContainerDiscoveryScope::ManagedSessions,
        }
    }

    pub fn agentbox_containers() -> Self {
        Self {
            scope: SessionDiscoveryScope::All,
            container_scope: ContainerDiscoveryScope::AgentboxOwned,
        }
    }

    pub fn for_git_root(mut self, git_root: &'a Utf8Path) -> Self {
        self.scope = SessionDiscoveryScope::for_git_root(git_root);
        self
    }

    pub fn discover(self, podman: &Podman) -> Result<Vec<SessionRecord>> {
        discover_scoped_sessions_from_podman(podman, self.scope, self.container_scope)
    }

    pub fn discover_from_ps(
        self,
        containers: Vec<PodmanPsContainer>,
        inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    ) -> Result<Vec<SessionRecord>> {
        discover_scoped_sessions_from_ps(
            containers,
            self.scope,
            self.container_scope,
            inspect_container,
        )
    }
}

fn discover_scoped_sessions_from_podman(
    podman: &Podman,
    scope: SessionDiscoveryScope<'_>,
    container_scope: ContainerDiscoveryScope,
) -> Result<Vec<SessionRecord>> {
    let containers = match container_scope {
        ContainerDiscoveryScope::ManagedSessions => podman.ps()?,
        ContainerDiscoveryScope::AgentboxOwned => podman.ps_all()?,
    };
    discover_scoped_sessions_from_ps(containers, scope, container_scope, |container_id| {
        podman.inspect_one(container_id)
    })
}

fn discover_scoped_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    scope: SessionDiscoveryScope<'_>,
    container_scope: ContainerDiscoveryScope,
    inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git_root_probe = HostGitRootProbe::new();
    discover_sessions_from_ps_with_git_root_probe(
        containers,
        scope,
        container_scope,
        inspect_container,
        &git_root_probe,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerDiscoveryScope {
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

enum SessionDiscoveryScope<'a> {
    All,
    GitRoot(GitRootDiscoveryScope<'a>),
}

impl<'a> SessionDiscoveryScope<'a> {
    fn for_git_root(git_root: &'a Utf8Path) -> Self {
        Self::GitRoot(GitRootDiscoveryScope::new(git_root))
    }

    fn should_inspect_ps_candidate(&self, container: &PodmanPsContainer) -> bool {
        match self {
            Self::All => true,
            Self::GitRoot(git_root) => git_root.should_inspect_ps_candidate(container),
        }
    }
}

struct GitRootDiscoveryScope<'a> {
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

struct SessionCollector<'a> {
    scope: SessionDiscoveryScope<'a>,
    sessions: Vec<SessionRecord>,
    mismatched_roots: Vec<String>,
}

impl<'a> SessionCollector<'a> {
    fn new(scope: SessionDiscoveryScope<'a>) -> Self {
        Self {
            scope,
            sessions: Vec::new(),
            mismatched_roots: Vec::new(),
        }
    }

    fn collect(&mut self, record: SessionRecord) {
        match &self.scope {
            SessionDiscoveryScope::All => self.sessions.push(record),
            SessionDiscoveryScope::GitRoot(git_root) => match git_root.classify_session(record) {
                ScopedSessionCandidate::Match(record) => self.sessions.push(record),
                ScopedSessionCandidate::Collision(root) => self.mismatched_roots.push(root),
                ScopedSessionCandidate::Ignore => {}
            },
        }
    }

    fn finish(mut self) -> Result<Vec<SessionRecord>> {
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

fn discover_sessions_from_ps_with_git_root_probe(
    containers: Vec<PodmanPsContainer>,
    scope: SessionDiscoveryScope<'_>,
    container_scope: ContainerDiscoveryScope,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    git_root_probe: &dyn GitRootProbe,
) -> Result<Vec<SessionRecord>> {
    let mut collector = SessionCollector::new(scope);

    for (container, container_kind) in containers
        .into_iter()
        .filter_map(|container| ps_candidate(container, container_scope))
    {
        if !collector.scope.should_inspect_ps_candidate(&container) {
            continue;
        }

        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect, container_kind, git_root_probe);
        collector.collect(record);
    }

    collector.finish()
}

fn build_session_record(
    container: PodmanPsContainer,
    inspect: PodmanContainerInspect,
    container_kind: AgentboxContainerKind,
    git_root_probe: &dyn GitRootProbe,
) -> SessionRecord {
    InspectedAgentboxContainer::from_podman(container, inspect, container_kind)
        .into_session_record(git_root_probe)
}

struct InspectedAgentboxContainer {
    container_id: String,
    container_name: String,
    container_kind: AgentboxContainerKind,
    metadata: SessionMetadata,
    label_report: SessionLabelReport,
    attach_endpoint: AttachEndpointReport,
    running: bool,
    mounts: Vec<PodmanContainerMount>,
}

impl InspectedAgentboxContainer {
    fn from_podman(
        container: PodmanPsContainer,
        inspect: PodmanContainerInspect,
        container_kind: AgentboxContainerKind,
    ) -> Self {
        let labels = &inspect.config.labels;
        let container_name = container
            .names
            .as_ref()
            .and_then(|names| names.first())
            .cloned()
            .unwrap_or_else(|| container.id.clone());
        let metadata = SessionMetadata::from_labels(labels);
        let label_report = SessionLabelReport::from_metadata(&metadata);
        let attach_endpoint =
            AttachEndpointReport::from_label_report_and_inspect(&label_report, &inspect);
        let running = inspect.state.running;
        let mounts = inspect.mounts;

        Self {
            container_id: container.id,
            container_name,
            container_kind,
            metadata,
            label_report,
            attach_endpoint,
            running,
            mounts,
        }
    }

    fn into_session_record(self, git_root_probe: &dyn GitRootProbe) -> SessionRecord {
        let status = derive_status(SessionStatusInput {
            label_report: &self.label_report,
            attach_endpoint: &self.attach_endpoint,
            running: self.running,
            mounts: &self.mounts,
            git_root_probe,
        });
        let attach_endpoint = self.attach_endpoint.into_endpoint();

        SessionRecord::new(SessionRecordInput {
            container_id: self.container_id,
            container_name: self.container_name,
            container_kind: self.container_kind,
            metadata: self.metadata,
            attach_endpoint,
            container_running: self.running,
            status,
        })
    }
}

fn ps_candidate(
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

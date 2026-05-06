// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

use crate::git::Git;
use crate::metadata::{
    LABEL_GIT_ROOT_HASH, LABEL_MANAGED, LABEL_MANAGED_VALUE, required_label_value,
};
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount, PodmanPsContainer};
use crate::runtime::AttachEndpoint;
use crate::workspace::git_root_hash12;
use crate::{Error, Result};

use super::endpoint::derive_attach_endpoint;
use super::labels::SessionLabelReport;
use super::record::{SessionMetadata, SessionRecord};
use super::status::{SessionStatusInput, derive_status, mark_duplicate_sessions};

pub fn discover_managed_sessions(podman: &Podman) -> Result<Vec<SessionRecord>> {
    discover_scoped_sessions_from_podman(podman, SessionDiscoveryScope::All)
}

pub fn discover_managed_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    discover_scoped_sessions_from_ps(containers, SessionDiscoveryScope::All, inspect_container)
}

pub fn discover_sessions_for_git_root(
    podman: &Podman,
    git_root: &Utf8Path,
) -> Result<Vec<SessionRecord>> {
    discover_scoped_sessions_from_podman(podman, SessionDiscoveryScope::for_git_root(git_root))
}

pub fn discover_sessions_for_git_root_from_ps(
    containers: Vec<PodmanPsContainer>,
    git_root: &Utf8Path,
    inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    discover_scoped_sessions_from_ps(
        containers,
        SessionDiscoveryScope::for_git_root(git_root),
        inspect_container,
    )
}

fn discover_scoped_sessions_from_podman(
    podman: &Podman,
    scope: SessionDiscoveryScope<'_>,
) -> Result<Vec<SessionRecord>> {
    discover_scoped_sessions_from_ps(podman.ps()?, scope, |container_id| {
        podman.inspect_one(container_id)
    })
}

fn discover_scoped_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    scope: SessionDiscoveryScope<'_>,
    inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_from_ps_with_git(containers, scope, inspect_container, &git)
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

fn discover_sessions_from_ps_with_git(
    containers: Vec<PodmanPsContainer>,
    scope: SessionDiscoveryScope<'_>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    git: &Git,
) -> Result<Vec<SessionRecord>> {
    let mut collector = SessionCollector::new(scope);

    for container in containers.into_iter().filter(ps_candidate_is_managed) {
        if !collector.scope.should_inspect_ps_candidate(&container) {
            continue;
        }

        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect, git);
        collector.collect(record);
    }

    collector.finish()
}

fn build_session_record(
    container: PodmanPsContainer,
    inspect: PodmanContainerInspect,
    git: &Git,
) -> SessionRecord {
    InspectedManagedContainer::from_podman(container, inspect).into_session_record(git)
}

struct InspectedManagedContainer {
    container_id: String,
    container_name: String,
    metadata: SessionMetadata,
    label_report: SessionLabelReport,
    attach_endpoint: Option<AttachEndpoint>,
    running: bool,
    mounts: Vec<PodmanContainerMount>,
}

impl InspectedManagedContainer {
    fn from_podman(container: PodmanPsContainer, inspect: PodmanContainerInspect) -> Self {
        let labels = &inspect.config.labels;
        let container_name = container
            .names
            .as_ref()
            .and_then(|names| names.first())
            .cloned()
            .unwrap_or_else(|| container.id.clone());
        let metadata = SessionMetadata::from_labels(labels);
        let label_report = SessionLabelReport::from_metadata(&metadata);
        let attach_endpoint = label_report
            .attach_labels()
            .and_then(|attach_labels| derive_attach_endpoint(attach_labels, &inspect).ok());
        let running = inspect.state.running;
        let mounts = inspect.mounts;

        Self {
            container_id: container.id,
            container_name,
            metadata,
            label_report,
            attach_endpoint,
            running,
            mounts,
        }
    }

    fn into_session_record(self, git: &Git) -> SessionRecord {
        let status = derive_status(SessionStatusInput {
            label_report: &self.label_report,
            attach_endpoint: self.attach_endpoint.as_ref(),
            running: self.running,
            mounts: &self.mounts,
            git,
        });

        SessionRecord {
            container_id: self.container_id,
            container_name: self.container_name,
            metadata: self.metadata,
            runtime_kind: self.label_report.runtime_kind(),
            attach_endpoint: self.attach_endpoint,
            container_running: self.running,
            status,
        }
    }
}

fn ps_candidate_is_managed(container: &PodmanPsContainer) -> bool {
    required_label_value(&container.labels, LABEL_MANAGED) == Some(LABEL_MANAGED_VALUE)
}

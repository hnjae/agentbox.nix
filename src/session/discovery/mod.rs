// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::Result;
use crate::podman::{Podman, PodmanContainerInspect, PodmanPsContainer};

use super::record::SessionRecord;
use super::status::{GitRootProbe, HostGitRootProbe};

mod inspected;
mod scope;

use inspected::build_session_record;
use scope::{ContainerDiscoveryScope, SessionCollector, SessionDiscoveryScope, ps_candidate};

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
        if !collector.should_inspect_ps_candidate(&container) {
            continue;
        }

        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect, container_kind, git_root_probe);
        collector.collect(record);
    }

    collector.finish()
}

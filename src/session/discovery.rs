// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::git::Git;
use crate::metadata::{LABEL_MANAGED, LABEL_MANAGED_VALUE, required_label_value};
use crate::podman::{Podman, PodmanContainerInspect, PodmanPsContainer};
use crate::workspace::hash12;
use crate::{Error, Result};

use super::endpoint::derive_attach_endpoint;
use super::labels::SessionLabels;
use super::record::{SessionGroup, SessionRecord};
use super::status::{derive_status, mark_duplicate_sessions};

pub fn discover_managed_sessions(podman: &Podman) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_from_ps_with_git(
        podman.ps()?,
        SessionDiscoveryScope::All,
        |container_id| podman.inspect_one(container_id),
        &git,
    )
}

pub fn discover_managed_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_from_ps_with_git(
        containers,
        SessionDiscoveryScope::All,
        |container_id| inspect_container(container_id),
        &git,
    )
}

pub fn discover_sessions_for_git_root(
    podman: &Podman,
    git_root: &Utf8Path,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_from_ps_with_git(
        podman.ps()?,
        SessionDiscoveryScope::GitRoot(git_root),
        |container_id| podman.inspect_one(container_id),
        &git,
    )
}

pub fn discover_sessions_for_git_root_from_ps(
    containers: Vec<PodmanPsContainer>,
    git_root: &Utf8Path,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_from_ps_with_git(
        containers,
        SessionDiscoveryScope::GitRoot(git_root),
        |container_id| inspect_container(container_id),
        &git,
    )
}

pub fn group_sessions_by_git_root(sessions: &[SessionRecord]) -> Vec<SessionGroup> {
    let mut groups = BTreeMap::<Utf8PathBuf, Vec<SessionRecord>>::new();

    for session in sessions {
        if let Some(root) = &session.canonical_git_root {
            groups
                .entry(root.clone())
                .or_default()
                .push(session.clone());
        }
    }

    groups
        .into_iter()
        .map(|(canonical_git_root, sessions)| SessionGroup {
            canonical_git_root,
            sessions,
        })
        .collect()
}

enum SessionDiscoveryScope<'a> {
    All,
    GitRoot(&'a Utf8Path),
}

fn discover_sessions_from_ps_with_git(
    containers: Vec<PodmanPsContainer>,
    scope: SessionDiscoveryScope<'_>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    git: &Git,
) -> Result<Vec<SessionRecord>> {
    let target_hash = match scope {
        SessionDiscoveryScope::All => None,
        SessionDiscoveryScope::GitRoot(git_root) => Some(hash12(git_root.as_str().as_bytes())),
    };
    let mut sessions = Vec::new();
    let mut mismatched_roots = Vec::new();

    for container in containers.into_iter().filter(ps_candidate_is_managed) {
        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect, git);

        match scope {
            SessionDiscoveryScope::All => sessions.push(record),
            SessionDiscoveryScope::GitRoot(git_root) => collect_git_root_scoped_session(
                record,
                git_root,
                target_hash
                    .as_deref()
                    .expect("target hash exists for git-root scope"),
                &mut sessions,
                &mut mismatched_roots,
            ),
        }
    }

    match scope {
        SessionDiscoveryScope::All => {}
        SessionDiscoveryScope::GitRoot(git_root) if !mismatched_roots.is_empty() => {
            mismatched_roots.sort();
            mismatched_roots.dedup();
            return Err(Error::msg(format!(
                "managed identity collision for `{git_root}` matched different full git roots: {}",
                mismatched_roots.join(", ")
            )));
        }
        SessionDiscoveryScope::GitRoot(_) => {}
    }

    Ok(mark_duplicate_sessions(sessions))
}

fn collect_git_root_scoped_session(
    record: SessionRecord,
    git_root: &Utf8Path,
    target_hash: &str,
    matches: &mut Vec<SessionRecord>,
    mismatched_roots: &mut Vec<String>,
) {
    let hash_matches = record.git_root_hash.as_deref() == Some(target_hash);

    match (record.canonical_git_root.as_deref(), hash_matches) {
        (Some(root), _) if root == git_root => matches.push(record),
        (Some(root), true) => mismatched_roots.push(root.to_string()),
        (None, true) => matches.push(record),
        _ => {}
    }
}

fn build_session_record(
    container: PodmanPsContainer,
    inspect: PodmanContainerInspect,
    git: &Git,
) -> SessionRecord {
    let labels = &inspect.config.labels;
    let container_name = container
        .names
        .as_ref()
        .and_then(|names| names.first())
        .cloned()
        .unwrap_or_else(|| container.id.clone());

    let session_labels = SessionLabels::from_map(labels);
    let attach_endpoint = derive_attach_endpoint(
        session_labels.runtime.as_deref(),
        session_labels.attach_scheme.as_deref(),
        session_labels.container_port.as_deref(),
        &inspect,
    )
    .ok();

    let (status, failure) = derive_status(
        &session_labels,
        attach_endpoint.as_ref(),
        inspect.state.running,
        &inspect.mounts,
        git,
    );

    SessionRecord {
        container_id: container.id,
        container_name,
        managed: session_labels.managed,
        schema: session_labels.schema,
        canonical_git_root: session_labels.canonical_git_root,
        git_root_hash: session_labels.git_root_hash,
        runtime: session_labels.runtime,
        image: session_labels.image,
        logical_name: session_labels.logical_name,
        attach_scheme: session_labels.attach_scheme,
        container_port: session_labels.container_port,
        container_listen_ip: session_labels.container_listen_ip,
        attach_endpoint,
        failure,
        status,
    }
}

fn ps_candidate_is_managed(container: &PodmanPsContainer) -> bool {
    required_label_value(&container.labels, LABEL_MANAGED) == Some(LABEL_MANAGED_VALUE)
}

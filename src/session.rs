// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount, PodmanPsContainer};
use crate::workspace::hash12;
use crate::{Error, Result};

pub const LABEL_MANAGED: &str = "io.agentbox.managed";
pub const LABEL_SCHEMA: &str = "io.agentbox.schema";
pub const LABEL_GIT_ROOT: &str = "io.agentbox.git_root";
pub const LABEL_GIT_ROOT_HASH: &str = "io.agentbox.git_root_hash";
pub const LABEL_RUNTIME: &str = "io.agentbox.runtime";
pub const LABEL_IMAGE: &str = "io.agentbox.image";
pub const LABEL_LOGICAL_NAME: &str = "io.agentbox.logical_name";

pub const REQUIRED_LABEL_NAMES: [&str; 7] = [
    LABEL_MANAGED,
    LABEL_SCHEMA,
    LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH,
    LABEL_RUNTIME,
    LABEL_IMAGE,
    LABEL_LOGICAL_NAME,
];

pub const LABEL_MANAGED_VALUE: &str = "true";
pub const LABEL_SCHEMA_VALUE: &str = "1";
pub const REQUIRED_NIX_CACHE_MOUNT_DESTINATION: &str = "/home/user/.cache/nix";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Stopped,
    Orphaned,
    Duplicate,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub container_id: String,
    pub container_name: String,
    pub managed: Option<String>,
    pub schema: Option<String>,
    pub canonical_git_root: Option<Utf8PathBuf>,
    pub git_root_hash: Option<String>,
    pub runtime: Option<String>,
    pub image: Option<String>,
    pub logical_name: Option<String>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

pub fn discover_managed_sessions(podman: &Podman) -> Result<Vec<SessionRecord>> {
    discover_managed_sessions_from_ps(podman.ps()?, |container| podman.inspect_one(container))
}

pub fn discover_managed_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let mut sessions = Vec::new();

    for container in containers.into_iter().filter(ps_candidate_is_managed) {
        let inspect = inspect_container(&container.id)?;
        sessions.push(build_session_record(container, inspect));
    }

    Ok(mark_duplicate_sessions(sessions))
}

pub fn discover_sessions_for_git_root(
    podman: &Podman,
    git_root: &Utf8Path,
) -> Result<Vec<SessionRecord>> {
    discover_sessions_for_git_root_from_ps(podman.ps()?, git_root, |container| {
        podman.inspect_one(container)
    })
}

pub fn discover_sessions_for_git_root_from_ps(
    containers: Vec<PodmanPsContainer>,
    git_root: &Utf8Path,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let target_hash = hash12(git_root.as_str().as_bytes());
    let mut matches = Vec::new();
    let mut mismatched_roots = Vec::new();

    for container in containers
        .into_iter()
        .filter(ps_candidate_is_managed)
        .filter(|container| container.labels.get(LABEL_GIT_ROOT_HASH) == Some(&target_hash))
    {
        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect);

        match record.canonical_git_root.as_deref() {
            Some(root) if root == git_root => matches.push(record),
            Some(root) => mismatched_roots.push(root.to_string()),
            None => matches.push(record),
        }
    }

    if !mismatched_roots.is_empty() {
        mismatched_roots.sort();
        mismatched_roots.dedup();
        return Err(Error::msg(format!(
            "hash12 prefilter for `{git_root}` matched different full git roots: {}",
            mismatched_roots.join(", ")
        )));
    }

    Ok(mark_duplicate_sessions(matches))
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

fn build_session_record(
    container: PodmanPsContainer,
    inspect: PodmanContainerInspect,
) -> SessionRecord {
    let labels = &inspect.config.labels;
    let container_name = container
        .names
        .first()
        .cloned()
        .unwrap_or_else(|| container.id.clone());

    let managed = required_label_value(labels, LABEL_MANAGED);
    let schema = required_label_value(labels, LABEL_SCHEMA);
    let canonical_git_root = required_label_value(labels, LABEL_GIT_ROOT).map(Utf8PathBuf::from);
    let git_root_hash = required_label_value(labels, LABEL_GIT_ROOT_HASH);
    let runtime = required_label_value(labels, LABEL_RUNTIME);
    let image = required_label_value(labels, LABEL_IMAGE);
    let logical_name = required_label_value(labels, LABEL_LOGICAL_NAME);

    let status = derive_status(
        &managed,
        &schema,
        canonical_git_root.as_deref(),
        git_root_hash.as_deref(),
        runtime.as_deref(),
        image.as_deref(),
        logical_name.as_deref(),
        &inspect,
    );

    SessionRecord {
        container_id: container.id,
        container_name,
        managed,
        schema,
        canonical_git_root,
        git_root_hash,
        runtime,
        image,
        logical_name,
        status,
    }
}

fn derive_status(
    managed: &Option<String>,
    schema: &Option<String>,
    canonical_git_root: Option<&Utf8Path>,
    git_root_hash: Option<&str>,
    runtime: Option<&str>,
    image: Option<&str>,
    logical_name: Option<&str>,
    inspect: &PodmanContainerInspect,
) -> SessionStatus {
    let labels_are_valid = managed.as_deref() == Some(LABEL_MANAGED_VALUE)
        && schema.as_deref() == Some(LABEL_SCHEMA_VALUE)
        && canonical_git_root.is_some()
        && git_root_hash.is_some()
        && runtime.is_some()
        && image.is_some()
        && logical_name.is_some();

    let hash_matches_root = canonical_git_root
        .zip(git_root_hash)
        .is_some_and(|(git_root, stored_hash)| stored_hash == hash12(git_root.as_str().as_bytes()));

    if !labels_are_valid
        || !hash_matches_root
        || !has_required_mount(&inspect.mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION)
    {
        return SessionStatus::Failed;
    }

    let canonical_git_root = canonical_git_root.expect("validated above");
    if git_root_is_orphaned(canonical_git_root) {
        return SessionStatus::Orphaned;
    }

    if inspect.state.running {
        SessionStatus::Running
    } else {
        SessionStatus::Stopped
    }
}

fn mark_duplicate_sessions(mut sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
    let mut group_sizes = BTreeMap::<Utf8PathBuf, usize>::new();

    for session in &sessions {
        if session.status == SessionStatus::Failed {
            continue;
        }

        if let Some(root) = &session.canonical_git_root {
            *group_sizes.entry(root.clone()).or_default() += 1;
        }
    }

    for session in &mut sessions {
        if session.status == SessionStatus::Failed {
            continue;
        }

        if session
            .canonical_git_root
            .as_ref()
            .and_then(|root| group_sizes.get(root))
            .is_some_and(|count| *count > 1)
        {
            session.status = SessionStatus::Duplicate;
        }
    }

    sessions
}

fn ps_candidate_is_managed(container: &PodmanPsContainer) -> bool {
    container.labels.get(LABEL_MANAGED) == Some(&LABEL_MANAGED_VALUE.to_string())
}

fn required_label_value(labels: &BTreeMap<String, String>, name: &str) -> Option<String> {
    labels
        .get(name)
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

fn has_required_mount(mounts: &[PodmanContainerMount], destination: &str) -> bool {
    mounts.iter().any(|mount| mount.destination == destination)
}

fn git_root_is_orphaned(git_root: &Utf8Path) -> bool {
    let path = git_root.as_std_path();
    if !path.is_dir() {
        return true;
    }

    match std::fs::canonicalize(path) {
        Ok(canonical) => {
            Utf8PathBuf::from_path_buf(canonical).map_or(true, |canonical| canonical != git_root)
        }
        Err(_) => true,
    }
}

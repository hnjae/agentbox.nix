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
use crate::podman::{Podman, PodmanContainerInspect, PodmanPsContainer};
use crate::runtime::AttachEndpoint;
use crate::workspace::hash12;
use crate::{Error, Result};

mod conflict;
mod endpoint;
mod labels;
mod status;

use endpoint::derive_attach_endpoint;
use labels::SessionLabels;
use status::{derive_status, mark_duplicate_sessions};

pub(crate) use conflict::{
    classify_create_error, duplicate_sessions_error, existing_session_error,
};
pub use endpoint::discover_attach_endpoint_from_inspect;

pub use labels::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_LABEL_NAMES,
};

pub(crate) use labels::{missing_required_label, required_label_value};
pub use status::{
    SessionFailure, SessionStatus, failed_session_requires_action_error,
    session_failure_requires_action_error,
};

pub const REQUIRED_NIX_CACHE_MOUNT_DESTINATION: &str = "/home/user/.cache/nix";

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
    pub attach_scheme: Option<String>,
    pub container_port: Option<String>,
    pub container_listen_ip: Option<String>,
    pub attach_endpoint: Option<AttachEndpoint>,
    pub failure: Option<SessionFailure>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

pub fn discover_managed_sessions(podman: &Podman) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_managed_sessions_from_ps_with_git(
        podman.ps()?,
        |container| podman.inspect_one(container),
        &git,
    )
}

pub fn discover_managed_sessions_from_ps(
    containers: Vec<PodmanPsContainer>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_managed_sessions_from_ps_with_git(
        containers,
        |container| inspect_container(container),
        &git,
    )
}

fn discover_managed_sessions_from_ps_with_git(
    containers: Vec<PodmanPsContainer>,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    git: &Git,
) -> Result<Vec<SessionRecord>> {
    let mut sessions = Vec::new();

    for container in containers.into_iter().filter(ps_candidate_is_managed) {
        let inspect = inspect_container(&container.id)?;
        sessions.push(build_session_record(container, inspect, git));
    }

    Ok(mark_duplicate_sessions(sessions))
}

pub fn discover_sessions_for_git_root(
    podman: &Podman,
    git_root: &Utf8Path,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_for_git_root_from_ps_with_git(
        podman.ps()?,
        git_root,
        |container| podman.inspect_one(container),
        &git,
    )
}

pub fn discover_sessions_for_git_root_from_ps(
    containers: Vec<PodmanPsContainer>,
    git_root: &Utf8Path,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
) -> Result<Vec<SessionRecord>> {
    let git = Git::new();
    discover_sessions_for_git_root_from_ps_with_git(
        containers,
        git_root,
        |container| inspect_container(container),
        &git,
    )
}

fn discover_sessions_for_git_root_from_ps_with_git(
    containers: Vec<PodmanPsContainer>,
    git_root: &Utf8Path,
    mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
    git: &Git,
) -> Result<Vec<SessionRecord>> {
    let target_hash = hash12(git_root.as_str().as_bytes());
    let mut matches = Vec::new();
    let mut mismatched_roots = Vec::new();

    for container in containers
        .into_iter()
        .filter(ps_candidate_is_managed)
        .filter(|container| {
            required_label_value(&container.labels, LABEL_GIT_ROOT_HASH)
                == Some(target_hash.as_str())
        })
    {
        let inspect = inspect_container(&container.id)?;
        let record = build_session_record(container, inspect, git);

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

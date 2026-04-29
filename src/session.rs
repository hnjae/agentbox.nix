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
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount, PodmanPsContainer};
use crate::runtime::{AttachEndpoint, DEFAULT_HOST_ATTACH_IP, RuntimeKind};
use crate::workspace::hash12;
use crate::{Error, Result};

pub const LABEL_MANAGED: &str = "io.agentbox.managed";
pub const LABEL_SCHEMA: &str = "io.agentbox.schema";
pub const LABEL_GIT_ROOT: &str = "io.agentbox.git_root";
pub const LABEL_GIT_ROOT_HASH: &str = "io.agentbox.git_root_hash";
pub const LABEL_RUNTIME: &str = "io.agentbox.runtime";
pub const LABEL_IMAGE: &str = "io.agentbox.image";
pub const LABEL_LOGICAL_NAME: &str = "io.agentbox.logical_name";
pub const LABEL_ATTACH_SCHEME: &str = "io.agentbox.attach_scheme";
pub const LABEL_CONTAINER_PORT: &str = "io.agentbox.container_port";
pub const LABEL_CONTAINER_LISTEN_IP: &str = "io.agentbox.container_listen_ip";

pub const REQUIRED_LABEL_NAMES: [&str; 10] = [
    LABEL_MANAGED,
    LABEL_SCHEMA,
    LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH,
    LABEL_RUNTIME,
    LABEL_IMAGE,
    LABEL_LOGICAL_NAME,
    LABEL_ATTACH_SCHEME,
    LABEL_CONTAINER_PORT,
    LABEL_CONTAINER_LISTEN_IP,
];

pub const LABEL_MANAGED_VALUE: &str = "true";
pub const LABEL_SCHEMA_VALUE: &str = "1";
pub const REQUIRED_NIX_CACHE_MOUNT_DESTINATION: &str = "/home/user/.cache/nix";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Orphaned,
    Duplicate,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFailure {
    MissingRequiredLabels,
    DriftedGitRootHash,
    MissingCacheMount,
    NotRunning,
    UnsupportedRuntimeLabel,
    MalformedEndpointLabels,
    MissingPublishedAttachPort,
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
        .filter(|container| container.labels.get(LABEL_GIT_ROOT_HASH) == Some(&target_hash))
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

    let managed = required_label_value(labels, LABEL_MANAGED);
    let schema = required_label_value(labels, LABEL_SCHEMA);
    let canonical_git_root = required_label_value(labels, LABEL_GIT_ROOT).map(Utf8PathBuf::from);
    let git_root_hash = required_label_value(labels, LABEL_GIT_ROOT_HASH);
    let runtime = required_label_value(labels, LABEL_RUNTIME);
    let image = required_label_value(labels, LABEL_IMAGE);
    let logical_name = required_label_value(labels, LABEL_LOGICAL_NAME);
    let attach_scheme = required_label_value(labels, LABEL_ATTACH_SCHEME);
    let container_port = required_label_value(labels, LABEL_CONTAINER_PORT);
    let container_listen_ip = required_label_value(labels, LABEL_CONTAINER_LISTEN_IP);
    let attach_endpoint = derive_attach_endpoint(
        runtime.as_deref(),
        attach_scheme.as_deref(),
        container_port.as_deref(),
        &inspect,
    )
    .ok();

    let (status, failure) = derive_status(
        &managed,
        &schema,
        canonical_git_root.as_deref(),
        git_root_hash.as_deref(),
        runtime.as_deref(),
        image.as_deref(),
        logical_name.as_deref(),
        attach_scheme.as_deref(),
        container_port.as_deref(),
        container_listen_ip.as_deref(),
        attach_endpoint.as_ref(),
        inspect.state.running,
        &inspect.mounts,
        git,
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
        attach_scheme,
        container_port,
        container_listen_ip,
        attach_endpoint,
        failure,
        status,
    }
}

#[allow(clippy::too_many_arguments)]
fn derive_status(
    managed: &Option<String>,
    schema: &Option<String>,
    canonical_git_root: Option<&Utf8Path>,
    git_root_hash: Option<&str>,
    runtime: Option<&str>,
    image: Option<&str>,
    logical_name: Option<&str>,
    attach_scheme: Option<&str>,
    container_port: Option<&str>,
    container_listen_ip: Option<&str>,
    attach_endpoint: Option<&AttachEndpoint>,
    running: bool,
    mounts: &[PodmanContainerMount],
    git: &Git,
) -> (SessionStatus, Option<SessionFailure>) {
    let labels_are_valid = managed.as_deref() == Some(LABEL_MANAGED_VALUE)
        && schema.as_deref() == Some(LABEL_SCHEMA_VALUE)
        && canonical_git_root.is_some()
        && git_root_hash.is_some()
        && runtime.is_some()
        && image.is_some()
        && logical_name.is_some()
        && attach_scheme.is_some()
        && container_port.is_some()
        && container_listen_ip.is_some();

    let hash_matches_root = canonical_git_root
        .zip(git_root_hash)
        .is_some_and(|(git_root, stored_hash)| stored_hash == hash12(git_root.as_str().as_bytes()));

    if !labels_are_valid {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::MissingRequiredLabels),
        );
    }

    if !hash_matches_root {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::DriftedGitRootHash),
        );
    }

    let runtime = match runtime.and_then(|runtime| runtime.parse::<RuntimeKind>().ok()) {
        Some(runtime) => runtime,
        None => {
            return (
                SessionStatus::Failed,
                Some(SessionFailure::UnsupportedRuntimeLabel),
            );
        }
    };

    let adapter = runtime.adapter();
    let _parsed_container_port = match container_port.and_then(|port| port.parse::<u16>().ok()) {
        Some(port) if port == adapter.container_port() => port,
        _ => {
            return (
                SessionStatus::Failed,
                Some(SessionFailure::MalformedEndpointLabels),
            );
        }
    };

    if attach_scheme != Some(adapter.attach_scheme())
        || container_listen_ip != Some(adapter.container_listen_ip())
    {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::MalformedEndpointLabels),
        );
    }

    if attach_endpoint.is_none() {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::MissingPublishedAttachPort),
        );
    }

    if !has_required_mount(mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::MissingCacheMount),
        );
    }

    let canonical_git_root = canonical_git_root.expect("validated above");
    if !running {
        return (SessionStatus::Failed, Some(SessionFailure::NotRunning));
    }

    if git_root_is_orphaned(canonical_git_root, git) {
        return (SessionStatus::Orphaned, None);
    }

    (SessionStatus::Running, None)
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

pub fn discover_attach_endpoint_from_inspect(
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let labels = &inspect.config.labels;
    derive_attach_endpoint(
        required_label_value(labels, LABEL_RUNTIME).as_deref(),
        required_label_value(labels, LABEL_ATTACH_SCHEME).as_deref(),
        required_label_value(labels, LABEL_CONTAINER_PORT).as_deref(),
        inspect,
    )
}

fn derive_attach_endpoint(
    runtime: Option<&str>,
    attach_scheme: Option<&str>,
    container_port: Option<&str>,
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let runtime = runtime
        .ok_or_else(|| Error::msg("missing required label `io.agentbox.runtime`"))?
        .parse::<RuntimeKind>()?;
    let adapter = runtime.adapter();
    let attach_scheme = attach_scheme
        .ok_or_else(|| Error::msg("missing required label `io.agentbox.attach_scheme`"))?;
    if attach_scheme != adapter.attach_scheme() {
        return Err(Error::msg(format!(
            "managed session has attach scheme `{attach_scheme}` but runtime `{runtime}` requires `{}`",
            adapter.attach_scheme(),
        )));
    }

    let container_port = container_port
        .ok_or_else(|| Error::msg("missing required label `io.agentbox.container_port`"))?
        .parse::<u16>()
        .map_err(|error| {
            Error::msg(format!(
                "malformed `io.agentbox.container_port` label: {error}"
            ))
        })?;

    if container_port != adapter.container_port() {
        return Err(Error::msg(format!(
            "managed session publishes container port `{container_port}` but runtime `{runtime}` requires `{}`",
            adapter.container_port(),
        )));
    }

    let port_key = format!("{container_port}/tcp");
    let binding = inspect
        .network_settings
        .ports
        .get(&port_key)
        .and_then(|bindings| bindings.as_ref())
        .and_then(|bindings| bindings.iter().find(|binding| binding.host_port.is_some()))
        .ok_or_else(|| {
            Error::msg(format!(
                "managed session has no published attach port for `{port_key}`"
            ))
        })?;

    let host_port = binding
        .host_port
        .as_deref()
        .ok_or_else(|| Error::msg(format!("missing host port for `{port_key}`")))?
        .parse::<u16>()
        .map_err(|error| Error::msg(format!("malformed published host port: {error}")))?;
    let host_ip = binding
        .host_ip
        .as_deref()
        .filter(|host_ip| !host_ip.trim().is_empty())
        .unwrap_or(DEFAULT_HOST_ATTACH_IP)
        .to_string();

    Ok(AttachEndpoint {
        scheme: attach_scheme.to_string(),
        host_ip,
        host_port,
    })
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

fn git_root_is_orphaned(git_root: &Utf8Path, git: &Git) -> bool {
    let canonical_git_root = match canonicalize_utf8(git_root) {
        Some(canonical_git_root) if canonical_git_root == git_root => canonical_git_root,
        _ => return true,
    };

    if !canonical_git_root.as_std_path().is_dir() {
        return true;
    }

    let git_marker = canonical_git_root.join(".git");
    if git_marker.is_dir() || git_marker.is_file() {
        return false;
    }

    match git.rev_parse_show_toplevel(&canonical_git_root) {
        Ok(resolved_git_root) => canonicalize_utf8(&resolved_git_root)
            .is_none_or(|resolved_git_root| resolved_git_root != canonical_git_root),
        Err(_) => true,
    }
}

fn canonicalize_utf8(path: &Utf8Path) -> Option<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(std::fs::canonicalize(path.as_std_path()).ok()?).ok()
}

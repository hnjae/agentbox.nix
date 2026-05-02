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

mod labels;

use labels::SessionLabels;

pub use labels::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_LABEL_NAMES,
};

pub(crate) use labels::{missing_required_label, required_label_value};

pub const REQUIRED_NIX_CACHE_MOUNT_DESTINATION: &str = "/home/user/.cache/nix";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Orphaned,
    Duplicate,
    Failed,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Orphaned => "orphaned",
            Self::Duplicate => "duplicate",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
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

impl SessionFailure {
    pub fn requires_action_error(self, git_root: &Utf8Path, container_name: &str) -> Error {
        match self {
            Self::MissingRequiredLabels => Error::managed_session_requires_action(
                git_root,
                container_name,
                "is missing required session labels",
                "repair or recreate it before retrying",
            ),
            Self::DriftedGitRootHash => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has a drifted `io.agentbox.git_root_hash`",
                "repair or recreate it before retrying",
            ),
            Self::MissingCacheMount => Error::managed_session_requires_action(
                git_root,
                container_name,
                &format!(
                    "is missing required cache mount `{}`",
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION
                ),
                "recreate the container before retrying",
            ),
            Self::NotRunning => Error::managed_session_requires_action(
                git_root,
                container_name,
                "is not running",
                "stop it or recreate it before retrying",
            ),
            Self::UnsupportedRuntimeLabel => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has an unsupported or malformed `io.agentbox.runtime` label",
                "repair or recreate it before retrying",
            ),
            Self::MalformedEndpointLabels => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has missing or inconsistent attach endpoint labels",
                "repair or recreate it before retrying",
            ),
            Self::MissingPublishedAttachPort => Error::managed_session_requires_action(
                git_root,
                container_name,
                "has no published attach endpoint port",
                "repair or recreate it before retrying",
            ),
        }
    }
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

pub fn failed_session_requires_action_error(
    git_root: &Utf8Path,
    session: &SessionRecord,
) -> Option<Error> {
    session.failure.map(|failure| {
        session_failure_requires_action_error(git_root, &session.container_name, failure)
    })
}

pub fn session_failure_requires_action_error(
    git_root: &Utf8Path,
    container_name: &str,
    failure: SessionFailure,
) -> Error {
    failure.requires_action_error(git_root, container_name)
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

fn derive_status(
    session_labels: &SessionLabels,
    attach_endpoint: Option<&AttachEndpoint>,
    running: bool,
    mounts: &[PodmanContainerMount],
    git: &Git,
) -> (SessionStatus, Option<SessionFailure>) {
    if !session_labels.has_required_values() {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::MissingRequiredLabels),
        );
    }

    if !session_labels.hash_matches_root() {
        return (
            SessionStatus::Failed,
            Some(SessionFailure::DriftedGitRootHash),
        );
    }

    let runtime = match session_labels
        .runtime
        .as_deref()
        .and_then(|runtime| runtime.parse::<RuntimeKind>().ok())
    {
        Some(runtime) => runtime,
        None => {
            return (
                SessionStatus::Failed,
                Some(SessionFailure::UnsupportedRuntimeLabel),
            );
        }
    };

    let adapter = runtime.adapter();
    let _parsed_container_port = match session_labels
        .container_port
        .as_deref()
        .and_then(|port| port.parse::<u16>().ok())
    {
        Some(port) if port == adapter.container_port() => port,
        _ => {
            return (
                SessionStatus::Failed,
                Some(SessionFailure::MalformedEndpointLabels),
            );
        }
    };

    if session_labels.attach_scheme.as_deref() != Some(adapter.attach_scheme())
        || session_labels.container_listen_ip.as_deref() != Some(adapter.container_listen_ip())
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

    let canonical_git_root = session_labels
        .canonical_git_root
        .as_deref()
        .expect("validated above");
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
        required_label_value(labels, LABEL_RUNTIME),
        required_label_value(labels, LABEL_ATTACH_SCHEME),
        required_label_value(labels, LABEL_CONTAINER_PORT),
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
    required_label_value(&container.labels, LABEL_MANAGED) == Some(LABEL_MANAGED_VALUE)
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

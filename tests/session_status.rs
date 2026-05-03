// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fs;

use agentbox::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::podman::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
    PodmanHostConfig, PodmanNetworkSettings, PodmanPortBinding, PodmanPsContainer,
};
use agentbox::session::{
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionFailure, SessionStatus,
    discover_managed_sessions_from_ps, discover_sessions_for_git_root_from_ps,
    group_sessions_by_git_root,
};
use agentbox::workspace::hash12;
use camino::Utf8Path;

#[path = "support/git_repo.rs"]
mod git_repo;

#[test]
fn duplicate_root_group_marks_each_row_duplicate() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let first = managed_container("dup-a", root, true, true);
    let second = managed_container("dup-b", root, true, true);

    let sessions = discover_managed_sessions_from_ps(
        vec![first.0, second.0],
        inspect_by_id(vec![first.1, second.1]),
    )
    .unwrap();

    assert_eq!(sessions.len(), 2);
    assert!(
        sessions
            .iter()
            .all(|session| session.status == SessionStatus::Duplicate)
    );

    let groups = group_sessions_by_git_root(&sessions);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].sessions.len(), 2);
}

#[test]
fn missing_required_labels_marks_failed() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-runtime", root, true, true);
    inspect.config.labels.remove(LABEL_RUNTIME);

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(sessions[0].status, SessionStatus::Failed);
}

#[test]
fn scoped_discovery_keeps_matching_root_when_identity_hash_is_missing() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (mut ps, mut inspect) = managed_container("missing-hash", root, true, true);
    ps.labels.remove(LABEL_GIT_ROOT_HASH);
    inspect.config.labels.remove(LABEL_GIT_ROOT_HASH);

    let sessions =
        discover_sessions_for_git_root_from_ps(vec![ps], root, inspect_by_id(vec![inspect]))
            .unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, SessionStatus::Failed);
    assert_eq!(
        sessions[0].failure,
        Some(SessionFailure::MissingRequiredLabels)
    );
}

#[test]
fn missing_cache_mount_marks_failed() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-cache", root, true, true);
    inspect.mounts.clear();

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(sessions[0].status, SessionStatus::Failed);
}

#[test]
fn unsupported_runtime_label_records_specific_failure() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("unknown-runtime", root, true, true);
    inspect
        .config
        .labels
        .insert(LABEL_RUNTIME.to_string(), "future-runtime".to_string());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].failure,
        Some(SessionFailure::UnsupportedRuntimeLabel)
    );
}

#[test]
fn malformed_endpoint_labels_record_specific_failure() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("bad-endpoint-label", root, true, true);
    inspect
        .config
        .labels
        .insert(LABEL_CONTAINER_PORT.to_string(), "4097".to_string());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].failure,
        Some(SessionFailure::MalformedEndpointLabels)
    );
}

#[test]
fn missing_published_attach_port_records_specific_failure() {
    let repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-port", root, true, true);
    inspect.network_settings.ports.clear();

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].failure,
        Some(SessionFailure::MissingPublishedAttachPort)
    );
}

#[test]
fn non_running_containers_are_failed_in_the_live_session_model() {
    let running_repo = git_repo::temp_git_repo();
    let stopped_repo = git_repo::temp_git_repo();
    let running_root = Utf8Path::from_path(running_repo.path()).unwrap();
    let stopped_root = Utf8Path::from_path(stopped_repo.path()).unwrap();
    let running = managed_container("running", running_root, true, true);
    let stopped = managed_container("stopped", stopped_root, false, true);

    let sessions = discover_managed_sessions_from_ps(
        vec![running.0, stopped.0],
        inspect_by_id(vec![running.1, stopped.1]),
    )
    .unwrap();

    assert_eq!(status_for(&sessions, "running"), SessionStatus::Running);
    assert_eq!(status_for(&sessions, "stopped"), SessionStatus::Failed);
}

#[test]
fn missing_git_root_path_marks_orphaned() {
    let missing_repo = git_repo::temp_git_repo();
    let root = Utf8Path::from_path(missing_repo.path()).unwrap().to_owned();
    let (ps, inspect) = managed_container("orphaned", &root, true, true);
    drop(missing_repo);

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(sessions[0].status, SessionStatus::Orphaned);
}

#[test]
fn existing_git_root_path_that_resolves_to_different_repo_marks_orphaned() {
    let workspace = tempfile::tempdir().unwrap();
    let stored_repo = workspace.path().join("stored-repo");
    fs::create_dir(&stored_repo).unwrap();
    init_git_repo(&stored_repo);

    let stored_root = Utf8Path::from_path(stored_repo.canonicalize().unwrap().as_path())
        .unwrap()
        .to_owned();
    let (ps, inspect) = managed_container("replaced-repo", &stored_root, true, true);

    fs::remove_dir_all(stored_repo.join(".git")).unwrap();
    init_git_repo(workspace.path());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(sessions[0].status, SessionStatus::Orphaned);
}

#[test]
fn hash_collision_between_different_roots_fails_clearly() {
    let target_repo = git_repo::temp_git_repo();
    let other_repo = git_repo::temp_git_repo();
    let target_root = Utf8Path::from_path(target_repo.path()).unwrap();
    let other_root = Utf8Path::from_path(other_repo.path()).unwrap();
    let forced_hash = hash12(target_root.as_str().as_bytes());
    let target =
        managed_container_with_hash("target", target_root, forced_hash.as_str(), true, true);
    let other = managed_container_with_hash("other", other_root, forced_hash.as_str(), true, true);

    let error = discover_sessions_for_git_root_from_ps(
        vec![target.0, other.0],
        target_root,
        inspect_by_id(vec![target.1, other.1]),
    )
    .unwrap_err();

    assert!(error.to_string().contains("managed identity collision"));
    assert!(error.to_string().contains(target_root.as_str()));
    assert!(error.to_string().contains(other_root.as_str()));
}

fn managed_container(
    name: &str,
    root: &Utf8Path,
    running: bool,
    include_cache_mount: bool,
) -> (PodmanPsContainer, PodmanContainerInspect) {
    managed_container_with_hash(
        name,
        root,
        hash12(root.as_str().as_bytes()).as_str(),
        running,
        include_cache_mount,
    )
}

fn managed_container_with_hash(
    name: &str,
    root: &Utf8Path,
    git_root_hash: &str,
    running: bool,
    include_cache_mount: bool,
) -> (PodmanPsContainer, PodmanContainerInspect) {
    let mut ps_labels = BTreeMap::new();
    ps_labels.insert(LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string());
    ps_labels.insert(LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string());

    let mut inspect_labels = BTreeMap::new();
    inspect_labels.insert(LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string());
    inspect_labels.insert(LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string());
    inspect_labels.insert(LABEL_GIT_ROOT.to_string(), root.as_str().to_string());
    inspect_labels.insert(LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string());
    inspect_labels.insert(LABEL_RUNTIME.to_string(), "opencode".to_string());
    inspect_labels.insert(
        LABEL_IMAGE.to_string(),
        "localhost/agentbox-opencode:local".to_string(),
    );
    inspect_labels.insert(LABEL_LOGICAL_NAME.to_string(), name.to_string());
    inspect_labels.insert(LABEL_ATTACH_SCHEME.to_string(), "http".to_string());
    inspect_labels.insert(LABEL_CONTAINER_PORT.to_string(), "4096".to_string());
    inspect_labels.insert(LABEL_CONTAINER_LISTEN_IP.to_string(), "0.0.0.0".to_string());

    let mounts = if include_cache_mount {
        vec![PodmanContainerMount {
            kind: "volume".to_string(),
            source: "agentbox-cache".to_string(),
            destination: REQUIRED_NIX_CACHE_MOUNT_DESTINATION.to_string(),
            rw: true,
        }]
    } else {
        Vec::new()
    };

    (
        PodmanPsContainer {
            id: format!("{name}-id"),
            image: "localhost/agentbox-opencode:local".to_string(),
            command: Some(vec!["opencode".to_string()]),
            created: 0,
            created_at: "2026-04-21 00:00:00 +0000 UTC".to_string(),
            names: Some(vec![name.to_string()]),
            ports: Some(Vec::new()),
            status: if running {
                "Up 1 minute".to_string()
            } else {
                "Exited (0) 1 minute ago".to_string()
            },
            state: if running {
                "running".to_string()
            } else {
                "exited".to_string()
            },
            labels: ps_labels,
            mounts: Some(Vec::new()),
            networks: Some(vec!["podman".to_string()]),
            namespaces: None,
        },
        PodmanContainerInspect {
            id: format!("{name}-id"),
            created: "2026-04-21T00:00:00.000000000Z".to_string(),
            path: "/usr/bin/opencode".to_string(),
            args: Vec::new(),
            state: PodmanContainerState {
                status: if running {
                    "running".to_string()
                } else {
                    "exited".to_string()
                },
                running,
                exit_code: 0,
                pid: if running { 4321 } else { 0 },
                started_at: Some("2026-04-21T00:00:01.000000000Z".to_string()),
                finished_at: None,
                health: None,
            },
            image_name: "localhost/agentbox-opencode:local".to_string(),
            config: PodmanContainerConfig {
                user: Some("user".to_string()),
                env: Vec::new(),
                cmd: vec!["opencode".to_string()],
                working_dir: Some("/workspace".to_string()),
                labels: inspect_labels,
                entrypoint: Some(vec!["/entrypoint".to_string()]),
                stop_signal: Some("SIGTERM".to_string()),
            },
            host_config: PodmanHostConfig {
                auto_remove: false,
                network_mode: Some("bridge".to_string()),
                privileged: false,
            },
            mounts,
            network_settings: PodmanNetworkSettings {
                networks: BTreeMap::new(),
                ports: BTreeMap::from([(
                    "4096/tcp".to_string(),
                    Some(vec![PodmanPortBinding {
                        host_ip: Some("127.0.0.1".to_string()),
                        host_port: Some("49152".to_string()),
                    }]),
                )]),
            },
        },
    )
}

fn inspect_by_id(
    inspects: Vec<PodmanContainerInspect>,
) -> impl FnMut(&str) -> agentbox::Result<PodmanContainerInspect> {
    let mut inspects = inspects
        .into_iter()
        .map(|inspect| (inspect.id.clone(), inspect))
        .collect::<BTreeMap<_, _>>();

    move |container_id| {
        inspects.remove(container_id).ok_or_else(|| {
            agentbox::Error::msg(format!("missing inspect fixture for `{container_id}`"))
        })
    }
}

fn status_for(
    sessions: &[agentbox::session::SessionRecord],
    container_name: &str,
) -> SessionStatus {
    sessions
        .iter()
        .find(|session| session.container_name == container_name)
        .map(|session| session.status)
        .unwrap()
}

fn init_git_repo(path: &std::path::Path) {
    fs::create_dir_all(path.join(".git/refs/heads")).unwrap();
    fs::write(path.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(
        path.join(".git/config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tfilemode = true\n\tlogallrefupdates = true\n",
    )
    .unwrap();
    fs::write(path.join(".gitignore"), "\n").unwrap();
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::metadata::{LABEL_CONTAINER_PORT, LABEL_GIT_ROOT_HASH, LABEL_RUNTIME};
use agentbox::session::{
    SessionFailure, SessionStatus, discover_managed_sessions_from_ps,
    discover_sessions_for_git_root_from_ps, group_sessions_by_git_root,
};
use agentbox::workspace::git_root_hash12;
use camino::Utf8Path;

#[path = "support/mod.rs"]
mod support;

use support::{
    inspect_models_by_id as inspect_by_id, managed_container_models as managed_container,
    managed_container_models_with_hash as managed_container_with_hash,
};

#[test]
fn duplicate_root_group_marks_each_row_duplicate() {
    let repo = support::temp_git_repo();
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
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-runtime", root, true, true);
    inspect.config.labels.remove(LABEL_RUNTIME);

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert!(sessions[0].status.is_failed());
}

#[test]
fn scoped_discovery_keeps_matching_root_when_identity_hash_is_missing() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (mut ps, mut inspect) = managed_container("missing-hash", root, true, true);
    ps.labels.remove(LABEL_GIT_ROOT_HASH);
    inspect.config.labels.remove(LABEL_GIT_ROOT_HASH);

    let sessions =
        discover_sessions_for_git_root_from_ps(vec![ps], root, inspect_by_id(vec![inspect]))
            .unwrap();

    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].status.is_failed());
    assert_eq!(
        sessions[0].status.failure(),
        Some(SessionFailure::MissingRequiredLabels)
    );
}

#[test]
fn scoped_discovery_skips_nonmatching_hash_without_inspect() {
    let target_repo = support::temp_git_repo();
    let unrelated_repo = support::temp_git_repo();
    let target_root = Utf8Path::from_path(target_repo.path()).unwrap();
    let unrelated_root = Utf8Path::from_path(unrelated_repo.path()).unwrap();
    let target = managed_container("target", target_root, true, true);
    let unrelated = managed_container("unrelated", unrelated_root, true, true);

    let sessions = discover_sessions_for_git_root_from_ps(
        vec![unrelated.0, target.0],
        target_root,
        inspect_by_id(vec![target.1]),
    )
    .unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].container_name, "target");
}

#[test]
fn missing_cache_mount_marks_failed() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-cache", root, true, true);
    inspect.mounts.clear();

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert!(sessions[0].status.is_failed());
}

#[test]
fn unsupported_runtime_label_records_specific_failure() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("unknown-runtime", root, true, true);
    inspect
        .config
        .labels
        .insert(LABEL_RUNTIME.to_string(), "future-runtime".to_string());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].status.failure(),
        Some(SessionFailure::UnsupportedRuntimeLabel)
    );
}

#[test]
fn malformed_endpoint_labels_record_specific_failure() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("bad-endpoint-label", root, true, true);
    inspect
        .config
        .labels
        .insert(LABEL_CONTAINER_PORT.to_string(), "4097".to_string());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].status.failure(),
        Some(SessionFailure::MalformedEndpointLabels)
    );
}

#[test]
fn missing_published_attach_port_records_specific_failure() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("missing-port", root, true, true);
    inspect.network_settings.ports.clear();

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].status.failure(),
        Some(SessionFailure::MissingPublishedAttachPort)
    );
}

#[test]
fn malformed_published_attach_port_records_specific_failure() {
    let repo = support::temp_git_repo();
    let root = Utf8Path::from_path(repo.path()).unwrap();
    let (ps, mut inspect) = managed_container("malformed-port", root, true, true);
    let bindings = inspect
        .network_settings
        .ports
        .values_mut()
        .next()
        .and_then(Option::as_mut)
        .expect("fixture should publish an attach port");
    bindings[0].host_port = Some("not-a-port".to_string());

    let sessions =
        discover_managed_sessions_from_ps(vec![ps], inspect_by_id(vec![inspect])).unwrap();

    assert_eq!(
        sessions[0].status.failure(),
        Some(SessionFailure::MissingPublishedAttachPort)
    );
    assert_eq!(sessions[0].attach_endpoint, None);
}

#[test]
fn non_running_containers_are_failed_in_the_live_session_model() {
    let running_repo = support::temp_git_repo();
    let stopped_repo = support::temp_git_repo();
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
    assert!(status_for(&sessions, "stopped").is_failed());
}

#[test]
fn missing_git_root_path_marks_orphaned() {
    let missing_repo = support::temp_git_repo();
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
    let target_repo = support::temp_git_repo();
    let other_repo = support::temp_git_repo();
    let target_root = Utf8Path::from_path(target_repo.path()).unwrap();
    let other_root = Utf8Path::from_path(other_repo.path()).unwrap();
    let forced_hash = git_root_hash12(target_root);
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

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::lock::{WorkspaceLockStore, lock_path_in_state_dir, lock_workspace_in_state_dir};
use agentbox::workspace::WorkspaceIdentity;
use camino::Utf8PathBuf;
use std::fs;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use tempfile::TempDir;

#[test]
fn builds_lock_path_from_state_home_and_digest() {
    let path = lock_path_in_state_dir("/tmp/state", "a".repeat(64));

    assert_eq!(
        path,
        std::path::PathBuf::from(
            "/tmp/state/agentbox/locks/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.lock"
        )
    );
}

#[test]
fn lock_store_builds_paths_for_a_known_state_dir() {
    let store = WorkspaceLockStore::in_state_dir("/tmp/state");

    assert_eq!(
        store.lock_path_for_digest("f".repeat(64)),
        lock_path_in_state_dir("/tmp/state", "f".repeat(64))
    );
}

#[test]
fn creates_parent_directories_for_new_lock_files() {
    let repo = temp_git_repo();
    let identity = identity_with_digest(&repo, "b".repeat(64));

    let lock = lock_workspace_in_state_dir(repo.path().join("state"), &identity).unwrap();
    assert!(lock.path().starts_with(repo.path().join("state")));
    assert!(lock.path().exists());
    assert!(lock.path().parent().unwrap().exists());
}

#[test]
fn reuses_preexisting_unlocked_lock_file() {
    let repo = temp_git_repo();
    let identity = identity_with_digest(&repo, "c".repeat(64));
    let path = lock_path_in_state_dir(repo.path().join("state"), &identity.digest64);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"stale").unwrap();

    let lock = lock_workspace_in_state_dir(repo.path().join("state"), &identity).unwrap();

    assert_eq!(lock.path(), path);
    assert_eq!(fs::read(&path).unwrap(), b"stale");
}

#[test]
fn guard_releases_lock_when_dropped() {
    let repo = temp_git_repo();
    let identity = identity_with_digest(&repo, "d".repeat(64));
    let mut lock = lock_workspace_in_state_dir(repo.path().join("state"), &identity).unwrap();

    {
        let _guard = lock.guard().unwrap();
    }

    assert!(lock.guard().is_ok());
}

#[test]
fn same_root_acquisitions_serialize() {
    let repo = temp_git_repo();
    let identity = identity_with_digest(&repo, "e".repeat(64));
    let state_dir = repo.path().join("state");
    let barrier = Arc::new(Barrier::new(2));
    let (tx, rx) = mpsc::channel();

    let first_identity = identity.clone();
    let first_barrier = Arc::clone(&barrier);
    let first_state_dir = state_dir.clone();
    let first_handle = thread::spawn(move || {
        let mut lock = lock_workspace_in_state_dir(first_state_dir, &first_identity).unwrap();
        let _guard = lock.guard().unwrap();
        first_barrier.wait();
        tx.send(()).unwrap();
    });

    barrier.wait();

    let second_state_dir = state_dir;
    let second_identity = identity;
    let second_handle = thread::spawn(move || {
        let mut lock = lock_workspace_in_state_dir(second_state_dir, &second_identity).unwrap();
        lock.guard().is_ok()
    });

    assert!(rx.recv_timeout(std::time::Duration::from_secs(1)).is_ok());
    assert!(second_handle.join().unwrap());
    first_handle.join().unwrap();
}

fn identity_with_digest(repo: &TempDir, digest64: String) -> WorkspaceIdentity {
    let path = Utf8PathBuf::from_path_buf(repo.path().to_path_buf()).unwrap();
    WorkspaceIdentity {
        requested_target: path.clone(),
        absolute_target: path.clone(),
        canonical_target: path.clone(),
        canonical_git_root: path,
        digest64,
        hash12: "ignored".to_string(),
        container_name: "ignored".to_string(),
    }
}

fn temp_git_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join("state")).unwrap();
    repo
}

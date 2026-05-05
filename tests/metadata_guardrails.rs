// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::lock::lock_path_in_state_dir;
use agentbox::workspace::{WorkspaceIdentity, hash12, resolve_workspace_identity};

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, cached_managed_inspect_fixture as managed_inspect_fixture,
    managed_ps_entry, opencode_managed_labels as managed_labels, ps_fixture,
};

#[test]
fn no_extra_host_metadata_is_written_beyond_locks() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));
    write_workspace_inspect(&harness, &workspace);
    harness
        .agentbox_assert(&["run", "--runtime", "opencode", target.to_str().unwrap()])
        .success();

    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "completion-id",
        "agentbox-completion",
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "completion-id",
        &managed_inspect_fixture(
            "agentbox-completion",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "agentbox-completion",
            ),
        ),
    );
    let completion = harness
        .agentbox_output(&["__completion-roots", "stop"])
        .status
        .success();
    assert!(completion);

    let expected_lock = lock_path_in_state_dir(harness.state_home_path(), &workspace.digest64);
    let files = harness.state_files();
    assert_eq!(
        files,
        vec![
            expected_lock
                .strip_prefix(harness.state_home_path())
                .unwrap()
                .to_path_buf()
        ]
    );
}

#[test]
fn stale_lock_file_is_reused_in_run_and_attach_flows() {
    let run_repo = support::temp_git_repo();
    let run_target = run_repo.path().join("run-nested");
    fs::create_dir(&run_target).unwrap();
    let run_workspace = resolve_workspace_identity(&run_target).unwrap();
    let run_harness = Harness::new();
    run_harness.write_ps(&ps_fixture(Vec::new()));
    write_workspace_inspect(&run_harness, &run_workspace);
    let run_lock = lock_path_in_state_dir(run_harness.state_home_path(), &run_workspace.digest64);
    fs::create_dir_all(run_lock.parent().unwrap()).unwrap();
    fs::write(&run_lock, b"stale-lock").unwrap();

    run_harness
        .agentbox_assert(&["run", "--runtime", "opencode", run_target.to_str().unwrap()])
        .success();
    assert_eq!(fs::read(&run_lock).unwrap(), b"stale-lock");

    let attach_repo = support::temp_git_repo();
    let attach_target = attach_repo.path().join("attach-nested");
    fs::create_dir(&attach_target).unwrap();
    let attach_workspace = resolve_workspace_identity(&attach_target).unwrap();
    let attach_harness = Harness::new();
    let attach_lock =
        lock_path_in_state_dir(attach_harness.state_home_path(), &attach_workspace.digest64);
    fs::create_dir_all(attach_lock.parent().unwrap()).unwrap();
    fs::write(&attach_lock, b"stale-lock").unwrap();
    attach_harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "attach-id",
        &attach_workspace.container_name,
        &attach_workspace.hash12,
    )]));
    attach_harness.write_inspect(
        "attach-id",
        &managed_inspect_fixture(
            &attach_workspace.container_name,
            attach_workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                attach_workspace.canonical_git_root.as_str(),
                &attach_workspace.hash12,
                &attach_workspace.container_name,
            ),
        ),
    );

    attach_harness
        .agentbox_assert(&["attach", attach_target.to_str().unwrap()])
        .success();
    assert_eq!(fs::read(&attach_lock).unwrap(), b"stale-lock");
}

#[test]
fn completion_uses_live_discovery_instead_of_cached_files() {
    let repo_a = support::temp_git_repo();
    let repo_b = support::temp_git_repo();
    let root_a = repo_a.path().canonicalize().unwrap();
    let root_b = repo_b.path().canonicalize().unwrap();

    let harness = Harness::new();
    let fake_cache = harness
        .state_home_path()
        .join("agentbox")
        .join("completion-cache.txt");
    fs::create_dir_all(fake_cache.parent().unwrap()).unwrap();
    fs::write(&fake_cache, root_a.to_str().unwrap()).unwrap();

    write_live_session(&harness, "live-a", root_a.to_str().unwrap());
    let first = harness.agentbox_output(&["__completion-roots", "stop"]);
    assert!(first.status.success());
    let first = String::from_utf8(first.stdout).unwrap();
    assert!(first.contains(root_a.to_str().unwrap()));
    assert!(!first.contains(root_b.to_str().unwrap()));

    write_live_session(&harness, "live-b", root_b.to_str().unwrap());
    let second = harness.agentbox_output(&["__completion-roots", "stop"]);
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert!(second.contains(root_b.to_str().unwrap()));
    assert!(!second.contains(root_a.to_str().unwrap()));
    assert_eq!(
        fs::read_to_string(&fake_cache).unwrap(),
        root_a.to_str().unwrap()
    );
}

fn write_live_session(harness: &Harness, name: &str, git_root: &str) {
    let git_root_hash = hash12(git_root.as_bytes());
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        name,
        name,
        &git_root_hash,
    )]));
    harness.write_inspect(
        name,
        &managed_inspect_fixture(
            name,
            git_root,
            true,
            managed_labels(git_root, &git_root_hash, name),
        ),
    );
}

fn write_workspace_inspect(harness: &Harness, workspace: &WorkspaceIdentity) {
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                &workspace.container_name,
            ),
        ),
    );
}

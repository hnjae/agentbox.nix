// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::Path;

use agentbox::workspace::{hash12, resolve_workspace_identity};

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, cached_managed_inspect_fixture as managed_inspect_fixture,
    managed_ps_entry, opencode_managed_labels as managed_labels, operation_names, ps_fixture,
};

#[test]
fn stop_stops_the_container_and_leaves_the_volume_and_workspace_untouched() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "session-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "session-id",
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

    run_command(&harness, &target, &[]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
    assert!(log[2].contains("--ignore"));
    assert!(target.exists(), "stop must not delete the user workspace");
    assert!(
        !log.iter().any(|line| line.starts_with("rm ")),
        "stop must rely on podman --rm instead of directly removing containers"
    );
    assert!(
        !log.iter().any(|line| line.starts_with("volume ")),
        "stop must not delete the matching cache volume"
    );
}

#[test]
fn stop_is_idempotent_when_the_container_disappears_during_cleanup() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "session-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "session-id",
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
    harness.mark_missing_during_cleanup();

    run_command(&harness, &target, &[]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
    assert!(log[2].contains("--ignore"));
}

#[test]
fn stop_force_removes_all_exact_duplicate_root_matches() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![
        managed_ps_entry("dup-a-id", "dup-a", &workspace.hash12),
        managed_ps_entry("dup-b-id", "dup-b", &workspace.hash12),
    ]));
    harness.write_inspect(
        "dup-a-id",
        &managed_inspect_fixture(
            "dup-a",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "dup-a",
            ),
        ),
    );
    harness.write_inspect(
        "dup-b-id",
        &managed_inspect_fixture(
            "dup-b",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "dup-b",
            ),
        ),
    );

    run_command(&harness, &target, &["--force"]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "inspect",
            "inspect",
            "stop",
            "container-exists",
            "stop",
            "container-exists"
        ]
    );
}

#[test]
fn stop_duplicate_root_requires_force_before_cleanup() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![
        managed_ps_entry("dup-a-id", "dup-a", &workspace.hash12),
        managed_ps_entry("dup-b-id", "dup-b", &workspace.hash12),
    ]));
    harness.write_inspect(
        "dup-a-id",
        &managed_inspect_fixture(
            "dup-a",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "dup-a",
            ),
        ),
    );
    harness.write_inspect(
        "dup-b-id",
        &managed_inspect_fixture(
            "dup-b",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "dup-b",
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist",
        ))
        .stderr(predicates::str::contains("agentbox stop --force"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn stop_allows_exact_missing_path_match_for_orphaned_root_identity() {
    let repo = support::temp_git_repo();
    let root = repo.path().canonicalize().unwrap();
    let root_string = root.to_str().unwrap().to_string();
    let hash = hash12(root_string.as_bytes());
    let container_name = "orphaned-session";
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "orphaned-id",
        container_name,
        &hash,
    )]));
    harness.write_inspect(
        "orphaned-id",
        &managed_inspect_fixture(
            container_name,
            &root_string,
            true,
            managed_labels(&root_string, &hash, container_name),
        ),
    );
    drop(repo);

    run_command(&harness, Path::new(&root_string), &[]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
}

fn install_harness() -> Harness {
    Harness::new()
}

fn run_command(
    harness: &Harness,
    target: &Path,
    extra_args: &[&str],
) -> assert_cmd::assert::Assert {
    harness.stop_assert(target, extra_args)
}

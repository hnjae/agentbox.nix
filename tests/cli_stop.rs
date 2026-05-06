// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::Path;

use agentbox::workspace::git_root_hash12;
use camino::Utf8Path;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, cached_managed_inspect_fixture as managed_inspect_fixture,
    managed_ps_entry, opencode_managed_labels as managed_labels,
    opencode_workspace_inspect_fixture, operation_names, ps_fixture, workspace_ps_entry,
};

#[test]
fn stop_stops_the_container_and_leaves_the_volume_and_workspace_untouched() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "session-id",
        workspace,
    )]));
    harness.write_inspect(
        "session-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    run_command(&harness, target, &[]).success();

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
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "session-id",
        workspace,
    )]));
    harness.write_inspect(
        "session-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );
    harness.mark_missing_during_cleanup();

    run_command(&harness, target, &[]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
    assert!(log[2].contains("--ignore"));
}

#[test]
fn stop_reports_container_that_still_exists_after_cleanup() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "session-id",
        workspace,
    )]));
    harness.write_inspect(
        "session-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );
    harness.mark_container_exists(&workspace.container_name);

    run_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains("partial stop failed"))
        .stderr(predicates::str::contains(&workspace.container_name))
        .stderr(predicates::str::contains(
            "container still exists after stop",
        ));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
}

#[test]
fn stop_force_removes_all_exact_duplicate_root_matches() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
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

    run_command(&harness, target, &["--force"]).success();

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
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
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

    run_command(&harness, target, &[])
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
    let hash = git_root_hash12(Utf8Path::new(&root_string));
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

#[test]
fn stop_accepts_case_insensitive_stable_id_prefix() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "session-id",
        workspace,
    )]));
    harness.write_inspect(
        "session-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );
    let prefix = workspace.hash12[..6].to_ascii_uppercase();

    run_command(&harness, Path::new(&prefix), &[]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
}

#[test]
fn stop_stable_id_prefix_reports_no_match() {
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));

    run_command(&harness, Path::new("deadbeef"), &[])
        .failure()
        .stderr(predicates::str::contains(
            "no managed session id matches prefix `deadbeef`",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps"]);
}

#[test]
fn stop_stable_id_prefix_reports_ambiguous_distinct_ids() {
    let first_fixture = support::temp_workspace("first");
    let second_fixture = support::temp_workspace("second");
    let first = &first_fixture.workspace;
    let second = &second_fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![
        managed_ps_entry("first-id", "first-session", "abcdef111111"),
        managed_ps_entry("second-id", "second-session", "abcdef222222"),
    ]));
    harness.write_inspect(
        "first-id",
        &managed_inspect_fixture(
            "first-session",
            first.canonical_git_root.as_str(),
            true,
            managed_labels(
                first.canonical_git_root.as_str(),
                "abcdef111111",
                "first-session",
            ),
        ),
    );
    harness.write_inspect(
        "second-id",
        &managed_inspect_fixture(
            "second-session",
            second.canonical_git_root.as_str(),
            true,
            managed_labels(
                second.canonical_git_root.as_str(),
                "abcdef222222",
                "second-session",
            ),
        ),
    );

    run_command(&harness, Path::new("abcdef"), &[])
        .failure()
        .stderr(predicates::str::contains(
            "stable id prefix `abcdef` matches multiple ids",
        ))
        .stderr(predicates::str::contains("use a longer prefix"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn stop_stable_id_duplicate_requires_force_before_cleanup() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
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

    run_command(&harness, Path::new(&workspace.hash12[..6]), &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist for stable id",
        ))
        .stderr(predicates::str::contains("agentbox stop --force"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn stop_force_removes_all_duplicate_stable_id_matches() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
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

    run_command(&harness, Path::new(&workspace.hash12[..6]), &["--force"]).success();

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

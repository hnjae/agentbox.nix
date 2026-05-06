// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::Path;

use agentbox::commands::stop::stop_prompt_candidates;
use agentbox::metadata::{LABEL_ATTACH_SCHEME, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH};
use agentbox::session::discover_managed_sessions_from_ps;
use agentbox::workspace::git_root_hash12;
use camino::Utf8Path;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, cached_managed_inspect_fixture as managed_inspect_fixture,
    inspect_models_by_id, managed_container_models, managed_ps_entry,
    opencode_managed_labels as managed_labels, opencode_workspace_inspect_fixture, operation_names,
    ps_fixture, workspace_ps_entry,
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
fn stop_without_targets_requires_tty_for_selection() {
    let harness = install_harness();

    harness
        .agentbox_assert(&["stop"])
        .failure()
        .stderr(predicates::str::contains(
            "agentbox stop requires a target or --all when stdin or stderr is not a TTY",
        ));

    assert!(harness.read_log().is_empty());
}

#[test]
fn stop_prompt_candidates_include_stop_completion_eligible_sessions() {
    let running_fixture = support::temp_workspace("running");
    let failed_fixture = support::temp_workspace("failed");
    let unidentifiable_fixture = support::temp_workspace("unidentifiable");
    let running = &running_fixture.workspace;
    let failed = &failed_fixture.workspace;
    let unidentifiable = &unidentifiable_fixture.workspace;
    let (running_ps, running_inspect) = managed_container_models(
        &running.container_name,
        running.canonical_git_root.as_ref(),
        true,
        true,
    );
    let (failed_ps, mut failed_inspect) = managed_container_models(
        &failed.container_name,
        failed.canonical_git_root.as_ref(),
        true,
        true,
    );
    let (unidentifiable_ps, mut unidentifiable_inspect) = managed_container_models(
        &unidentifiable.container_name,
        unidentifiable.canonical_git_root.as_ref(),
        true,
        true,
    );
    failed_inspect.config.labels.remove(LABEL_ATTACH_SCHEME);
    unidentifiable_inspect
        .config
        .labels
        .remove(LABEL_GIT_ROOT_HASH);

    let sessions = discover_managed_sessions_from_ps(
        vec![running_ps, failed_ps, unidentifiable_ps],
        inspect_models_by_id(vec![
            running_inspect,
            failed_inspect,
            unidentifiable_inspect,
        ]),
    )
    .unwrap();

    let candidates = stop_prompt_candidates(&sessions);
    let mut targets = candidates
        .iter()
        .map(|candidate| candidate.value().to_str().unwrap().to_string())
        .collect::<Vec<_>>();
    targets.sort();
    let mut expected = vec![failed.hash12.clone(), running.hash12.clone()];
    expected.sort();

    assert_eq!(targets, expected);
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
fn stop_accepts_multiple_explicit_targets() {
    let first_fixture = support::temp_workspace("first");
    let second_fixture = support::temp_workspace("second");
    let first_target = first_fixture.target.as_path();
    let second_target = second_fixture.target.as_path();
    let first = &first_fixture.workspace;
    let second = &second_fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("first-id", first),
        workspace_ps_entry("second-id", second),
    ]));
    harness.write_inspect(
        "first-id",
        &opencode_workspace_inspect_fixture(first, true, true),
    );
    harness.write_inspect(
        "second-id",
        &opencode_workspace_inspect_fixture(second, true, true),
    );

    let mut command = harness.agentbox_command();
    command.arg("stop").arg(first_target).arg(second_target);

    command.assert().success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "inspect",
            "stop",
            "container-exists",
            "ps",
            "inspect",
            "stop",
            "container-exists"
        ]
    );
}

#[test]
fn stop_reports_aggregate_failures_after_processing_other_targets() {
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

    let mut command = harness.agentbox_command();
    command.arg("stop").arg(target).arg("deadbeef");

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains("failed to stop 1 target"))
        .stderr(predicates::str::contains("`deadbeef`"))
        .stderr(predicates::str::contains(
            "no managed session id matches prefix `deadbeef`",
        ));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists", "ps", "inspect"]
    );
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
fn stop_force_allows_exact_missing_path_match_for_failed_root_identity() {
    let repo = support::temp_git_repo();
    let root = repo.path().canonicalize().unwrap();
    let root_string = root.to_str().unwrap().to_string();
    let hash = git_root_hash12(Utf8Path::new(&root_string));
    let container_name = "failed-session";
    let harness = install_harness();
    let mut labels = managed_labels(&root_string, &hash, container_name);
    labels.remove(LABEL_ATTACH_SCHEME);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-id",
        container_name,
        &hash,
    )]));
    harness.write_inspect(
        "failed-id",
        &managed_inspect_fixture(container_name, &root_string, true, labels),
    );
    drop(repo);

    run_command(&harness, Path::new(&root_string), &["--force"]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
}

#[test]
fn stop_missing_absolute_path_without_exact_stored_match_fails() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let target = fixture
        .workspace
        .canonical_git_root
        .with_file_name("missing-agentbox-root");
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "session-id",
        workspace,
    )]));
    harness.write_inspect(
        "session-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    run_command(&harness, target.as_std_path(), &[])
        .failure()
        .stderr(predicates::str::contains(
            "no managed session exists for exact stored git-root path",
        ))
        .stderr(predicates::str::contains(target.as_str()));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn stop_duplicate_exact_missing_stored_path_requires_force_before_cleanup() {
    let repo = support::temp_git_repo();
    let root = repo.path().canonicalize().unwrap();
    let root_string = root.to_str().unwrap().to_string();
    let hash = git_root_hash12(Utf8Path::new(&root_string));
    let harness = install_harness();
    write_duplicate_stored_git_root_sessions(&harness, &root_string, &hash);
    drop(repo);

    run_command(&harness, Path::new(&root_string), &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist for exact stored git-root path",
        ))
        .stderr(predicates::str::contains("agentbox stop --force"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn stop_force_removes_duplicate_exact_missing_stored_path_matches() {
    let repo = support::temp_git_repo();
    let root = repo.path().canonicalize().unwrap();
    let root_string = root.to_str().unwrap().to_string();
    let hash = git_root_hash12(Utf8Path::new(&root_string));
    let harness = install_harness();
    write_duplicate_stored_git_root_sessions(&harness, &root_string, &hash);
    drop(repo);

    run_command(&harness, Path::new(&root_string), &["--force"]).success();

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

#[test]
fn stop_all_stops_running_managed_sessions_and_ignores_stopped_ones() {
    let first_fixture = support::temp_workspace("first");
    let second_fixture = support::temp_workspace("second");
    let stopped_fixture = support::temp_workspace("stopped");
    let first = &first_fixture.workspace;
    let second = &second_fixture.workspace;
    let stopped = &stopped_fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("first-id", first),
        workspace_ps_entry("second-id", second),
        workspace_ps_entry("stopped-id", stopped),
    ]));
    harness.write_inspect(
        "first-id",
        &opencode_workspace_inspect_fixture(first, true, true),
    );
    harness.write_inspect(
        "second-id",
        &opencode_workspace_inspect_fixture(second, true, true),
    );
    harness.write_inspect(
        "stopped-id",
        &opencode_workspace_inspect_fixture(stopped, false, true),
    );

    run_all_command(&harness).success();

    let log = harness.read_log();
    let stop_lines = log
        .iter()
        .filter(|line| line.starts_with("stop "))
        .collect::<Vec<_>>();
    assert_eq!(stop_lines.len(), 2);
    assert!(
        stop_lines
            .iter()
            .any(|line| line.contains(&first.container_name))
    );
    assert!(
        stop_lines
            .iter()
            .any(|line| line.contains(&second.container_name))
    );
    assert!(
        !stop_lines
            .iter()
            .any(|line| line.contains(&stopped.container_name))
    );
    assert_eq!(
        operation_names(&log)
            .into_iter()
            .filter(|operation| *operation == "stop")
            .count(),
        2
    );
}

#[test]
fn stop_all_succeeds_when_no_running_managed_sessions_exist() {
    let fixture = support::temp_workspace("stopped");
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "stopped-id",
        workspace,
    )]));
    harness.write_inspect(
        "stopped-id",
        &opencode_workspace_inspect_fixture(workspace, false, true),
    );

    run_all_command(&harness).success();

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn stop_all_stops_running_managed_container_without_recoverable_git_root() {
    let fixture = support::temp_workspace("broken");
    let workspace = &fixture.workspace;
    let container_name = "broken-session";
    let harness = install_harness();
    let mut labels = managed_labels(
        workspace.canonical_git_root.as_str(),
        &workspace.hash12,
        container_name,
    );
    labels.remove(LABEL_GIT_ROOT);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "broken-id",
        container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "broken-id",
        &managed_inspect_fixture(
            container_name,
            workspace.canonical_git_root.as_str(),
            true,
            labels,
        ),
    );

    run_all_command(&harness).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "stop", "container-exists"]
    );
    assert!(log[2].contains(container_name));
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

fn run_all_command(harness: &Harness) -> assert_cmd::assert::Assert {
    harness.agentbox_assert(&["stop", "--all"])
}

fn write_duplicate_stored_git_root_sessions(harness: &Harness, root: &str, hash: &str) {
    harness.write_ps(&ps_fixture(vec![
        managed_ps_entry("dup-a-id", "dup-a", hash),
        managed_ps_entry("dup-b-id", "dup-b", hash),
    ]));
    harness.write_inspect(
        "dup-a-id",
        &managed_inspect_fixture("dup-a", root, true, managed_labels(root, hash, "dup-a")),
    );
    harness.write_inspect(
        "dup-b-id",
        &managed_inspect_fixture("dup-b", root, true, managed_labels(root, hash, "dup-b")),
    );
}

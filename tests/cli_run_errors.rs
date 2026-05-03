// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::Path;

use agentbox::session::{LABEL_RUNTIME, REQUIRED_NIX_CACHE_MOUNT_DESTINATION};
use agentbox::workspace::{hash12, resolve_workspace_identity};

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, managed_labels, managed_ps_entry, operation_names, ps_fixture,
    running_managed_inspect_fixture as managed_inspect_fixture,
};

#[test]
fn existing_managed_session_suggests_attach_before_image_work() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "existing-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "existing-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    let assert = run_command(&harness, &target, &[]);

    assert
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox attach {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )))
        .stderr(predicates::str::contains(&workspace.container_name));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("create ")));
    assert!(!log.iter().any(|line| line.starts_with("image ")));
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn duplicate_sessions_fail_closed() {
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
                "opencode",
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
                "opencode",
                "dup-b",
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist",
        ))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn unsupported_runtime_label_requires_repair_or_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "mismatch-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "mismatch-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "other-runtime",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "unsupported or malformed `io.agentbox.runtime` label",
        ))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));
}

#[test]
fn hash_collision_fails_closed() {
    let target_repo = support::temp_git_repo();
    let other_repo = support::temp_git_repo();
    let target = target_repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let other_root = other_repo.path().canonicalize().unwrap();
    let other_root = other_root.to_str().unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "collision-id",
        "collision-name",
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "collision-id",
        &managed_inspect_fixture(
            "collision-name",
            other_root,
            true,
            managed_labels(other_root, &workspace.hash12, "opencode", "collision-name"),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("managed identity collision"))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ))
        .stderr(predicates::str::contains(other_root));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn create_name_conflict_reports_the_conflicting_root() {
    let repo = support::temp_git_repo();
    let other_repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let other_root = other_repo.path().canonicalize().unwrap();
    let other_root = other_root.to_str().unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("run", "the container name is already in use", 125);
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            other_root,
            true,
            managed_labels(
                other_root,
                &hash12(other_root.as_bytes()),
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(format!(
            "container name `{}` is already used by managed session",
            workspace.container_name
        )))
        .stderr(predicates::str::contains(other_root))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "run", "inspect"]);
}

#[test]
fn failed_session_with_missing_labels_requires_repair_or_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    let mut labels = managed_labels(
        workspace.canonical_git_root.as_str(),
        &workspace.hash12,
        "opencode",
        &workspace.container_name,
    );
    labels.remove(LABEL_RUNTIME);
    harness.write_inspect(
        "failed-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            labels,
        ),
    );
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            )
            .into_iter()
            .filter(|(key, _)| key != LABEL_RUNTIME)
            .collect(),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required session labels"))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn failed_session_with_missing_cache_mount_requires_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "missing-cache-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "missing-cache-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required cache mount"))
        .stderr(predicates::str::contains(
            REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
        ))
        .stderr(predicates::str::contains(
            "recreate the container before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

fn install_harness() -> Harness {
    Harness::new()
}

fn run_command(
    harness: &Harness,
    target: &Path,
    extra_args: &[&str],
) -> assert_cmd::assert::Assert {
    harness.run_assert_with_args(target, extra_args)
}

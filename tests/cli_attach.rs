// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::metadata::LABEL_LAUNCH_DIRECTORY;
use agentbox::workspace::resolve_workspace_identity;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, managed_inspect_fixture, managed_ps_entry,
    opencode_managed_labels as managed_labels, opencode_workspace_labels, operation_names,
    ps_fixture, workspace_ps_entry,
};

#[test]
fn attach_to_a_running_session_attaches_to_container_stdio() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            labels_with_launch_directory(
                opencode_workspace_labels(workspace),
                workspace.canonical_target.as_str(),
            ),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("attach").arg(target);

    command.assert().success().stderr("");

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "opencode"]);
    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[2].contains("attach http://127.0.0.1:49152"));
    assert!(log[2].contains(&format!("cwd={}", workspace.canonical_target)));
    assert!(!log.iter().any(|line| line.starts_with("create ")));
    assert!(!log.iter().any(|line| line.starts_with("attach ")));
}

#[test]
fn attach_does_not_wrap_host_client_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            labels_with_launch_directory(
                opencode_workspace_labels(workspace),
                workspace.canonical_target.as_str(),
            ),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("attach").arg(target);

    command.assert().success().stderr("");

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "opencode"]);
    assert!(log[2].contains("attach http://127.0.0.1:49152"));
    assert!(log[2].contains(&format!("cwd={}", workspace.canonical_target)));
    assert!(!log.iter().any(|line| line.starts_with("direnv ")));
    assert!(!log.iter().any(|line| line.contains("direnv exec .")));
}

#[test]
fn attach_uses_stored_launch_directory_when_requesting_another_subdirectory() {
    let repo = support::temp_git_repo();
    let launch_target = repo.path().join("launch");
    let request_target = repo.path().join("request");
    fs::create_dir(&launch_target).unwrap();
    fs::create_dir(&request_target).unwrap();

    let launch_workspace = resolve_workspace_identity(&launch_target).unwrap();
    let request_workspace = resolve_workspace_identity(&request_target).unwrap();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "running-id",
        &request_workspace.container_name,
        &request_workspace.hash12,
    )]));
    harness.write_inspect(
        "running-id",
        &managed_inspect_fixture(
            &request_workspace.container_name,
            request_workspace.canonical_git_root.as_str(),
            true,
            true,
            labels_with_launch_directory(
                managed_labels(
                    request_workspace.canonical_git_root.as_str(),
                    &request_workspace.hash12,
                    &request_workspace.container_name,
                ),
                launch_workspace.canonical_target.as_str(),
            ),
        ),
    );

    let mut command = harness.locked_agentbox_command(&request_workspace);
    command.arg("attach").arg(&request_target);

    command
        .assert()
        .success()
        .stderr(predicates::str::contains("using stored launch directory"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "opencode"]);
    assert!(log[2].contains(&format!("cwd={}", launch_workspace.canonical_target)));
    assert!(!log[2].contains(&format!("cwd={}", request_workspace.canonical_target)));
}

#[test]
fn attach_to_a_stopped_session_reports_the_running_only_model() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "stopped-id",
        workspace,
    )]));
    harness.write_inspect(
        "stopped-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            true,
            labels_with_launch_directory(
                opencode_workspace_labels(workspace),
                workspace.canonical_target.as_str(),
            ),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("attach").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains("is not running"))
        .stderr(predicates::str::contains(format!(
            "agentbox run --runtime opencode {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

fn labels_with_launch_directory(
    mut labels: std::collections::BTreeMap<String, String>,
    launch_directory: &str,
) -> std::collections::BTreeMap<String, String> {
    labels.insert(
        LABEL_LAUNCH_DIRECTORY.to_string(),
        launch_directory.to_string(),
    );
    labels
}

#[test]
fn attach_without_an_existing_session_suggests_run() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("attach").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains(format!(
            "use `agentbox run --runtime <opencode|codex> {}` to create one",
            target.display()
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps"]);
}

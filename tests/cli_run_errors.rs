// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::net::TcpListener;
use std::path::Path;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use agentbox::metadata::{LABEL_LAUNCH_DIRECTORY, LABEL_RUNTIME};
use agentbox::prompt;
use agentbox::runtime::RuntimeKind;
use agentbox::session::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use agentbox::workspace::git_root_hash12;
use assert_cmd::Command as AssertCommand;
use camino::Utf8Path;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, ReadyEndpoint, managed_ps_entry,
    opencode_managed_labels as managed_labels, opencode_workspace_inspect_fixture,
    opencode_workspace_inspect_fixture_with_cache_bind, opencode_workspace_labels, operation_names,
    ps_fixture, running_managed_inspect_fixture as managed_inspect_fixture,
    running_workspace_inspect_fixture_with_host_port, workspace_ps_entry,
};

#[test]
fn missing_runtime_requires_tty_for_prompting() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.args(["start", "/tmp/workspace"]);

    command.assert().failure().stderr(predicates::str::contains(
        "agentbox start requires --runtime when stdin or stderr is not a TTY",
    ));
}

#[test]
fn prompt_selection_errors_are_stable() {
    assert_eq!(
        prompt::selection_error(inquire::InquireError::OperationCanceled).to_string(),
        "selection canceled",
    );
    assert_eq!(
        prompt::selection_error(inquire::InquireError::OperationInterrupted).to_string(),
        "selection interrupted",
    );
}

#[test]
fn start_existing_managed_session_suggests_connect_before_image_work() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "existing-id",
        workspace,
    )]));
    harness.write_inspect(
        "existing-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    let assert = start_command(&harness, target, &[]);

    assert
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox connect {}",
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
fn run_fails_when_a_managed_session_already_exists() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "existing-id",
        workspace,
    )]));
    harness.write_inspect(
        "existing-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox connect {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )))
        .stderr(predicates::str::contains(&workspace.container_name));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("image ")));
    assert!(!log.iter().any(|line| line.starts_with("run ")));
}

#[test]
fn exec_fails_when_a_managed_session_already_exists() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "existing-id",
        workspace,
    )]));
    harness.write_inspect(
        "existing-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("exec").arg(target).args(["--", "fix-tests"]);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox connect {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )))
        .stderr(predicates::str::contains(&workspace.container_name));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("image ")));
    assert!(!log.iter().any(|line| line.starts_with("run ")));
}

#[test]
fn start_with_connect_still_fails_when_a_managed_session_already_exists() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "existing-id",
        workspace,
    )]));
    harness.write_inspect(
        "existing-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    start_command(&harness, target, &["--connect"])
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox connect {}",
            target.display()
        )))
        .stderr(predicates::str::contains(&workspace.container_name));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("opencode ")));
    assert!(!log.iter().any(|line| line.starts_with("image ")));
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn run_propagates_host_client_exit_code_and_cleans_up() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("opencode", "host client exited\n", 42);
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command
        .assert()
        .failure()
        .code(42)
        .stderr(predicates::str::contains("host client exited"))
        .stderr(predicates::str::contains("ERR:").not());
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "image",
            "run",
            "inspect",
            "opencode",
            "stop",
            "container-exists"
        ]
    );
}

#[test]
fn run_reports_host_client_failure_and_cleanup_failure_together() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("opencode", "host client exited\n", 42);
    harness.mark_container_exists(&workspace.container_name);
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command
        .assert()
        .failure()
        .code(42)
        .stderr(predicates::str::contains("host client exited"))
        .stderr(predicates::str::contains("exited with exit status 42"))
        .stderr(predicates::str::contains(
            "failed to clean up transient run container",
        ))
        .stderr(predicates::str::contains(
            "container still exists after stop",
        ));
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "image",
            "run",
            "inspect",
            "opencode",
            "stop",
            "container-exists"
        ]
    );
}

#[test]
fn run_stops_transient_container_when_readiness_fails() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.write_inspect(
        &workspace.container_name,
        &opencode_workspace_inspect_fixture(workspace, false, true),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains("transient run container"))
        .stderr(predicates::str::contains(
            "exited before the `opencode` runtime server became reachable",
        ));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "image",
            "run",
            "inspect",
            "logs",
            "stop",
            "container-exists"
        ]
    );
}

#[test]
fn run_fails_before_starting_container_when_host_client_is_missing() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.remove_fake_program("opencode");

    let mut command = harness.locked_agentbox_command(workspace);
    command.env("PATH", harness.fake_bin_only_path_env());
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command.assert().failure().stderr(predicates::str::contains(
        "`opencode` was not found on PATH",
    ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps"]);
}

#[test]
fn exec_propagates_foreground_podman_exit_code() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("run", "codex exec exited\n", 42);

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("exec").arg(target).args(["--", "fix-tests"]);

    command
        .assert()
        .failure()
        .code(42)
        .stderr(predicates::str::contains("codex exec exited"))
        .stderr(predicates::str::contains("ERROR:").not());

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "run"]);
}

#[test]
fn start_duplicate_sessions_fail_closed() {
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

    start_command(&harness, target, &[])
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
fn start_unsupported_runtime_label_requires_cleanup_or_recreation() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "mismatch-id",
        workspace,
    )]));
    harness.write_inspect(
        "mismatch-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            labels_with_runtime(
                managed_labels(
                    workspace.canonical_git_root.as_str(),
                    &workspace.hash12,
                    &workspace.container_name,
                ),
                "other-runtime",
            ),
        ),
    );

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "unsupported or malformed `io.agentbox.runtime` label",
        ))
        .stderr(predicates::str::contains(
            "clean up or recreate it before retrying",
        ));
}

#[test]
fn start_hash_collision_fails_closed() {
    let fixture = support::temp_workspace("nested");
    let other_repo = support::temp_git_repo();
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
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
            managed_labels(other_root, &workspace.hash12, "collision-name"),
        ),
    );

    start_command(&harness, target, &[])
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
fn start_create_name_conflict_reports_the_conflicting_root() {
    let fixture = support::temp_workspace("nested");
    let other_repo = support::temp_git_repo();
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
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
                &git_root_hash12(Utf8Path::new(other_root)),
                &workspace.container_name,
            ),
        ),
    );

    start_command(&harness, target, &[])
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
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect"]
    );
}

#[test]
fn start_create_name_conflict_reports_conflicting_root_even_with_malformed_runtime_label() {
    let fixture = support::temp_workspace("nested");
    let other_repo = support::temp_git_repo();
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
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
            labels_with_runtime(
                managed_labels(
                    other_root,
                    &git_root_hash12(Utf8Path::new(other_root)),
                    &workspace.container_name,
                ),
                "future-runtime",
            ),
        ),
    );

    start_command(&harness, target, &[])
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
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect"]
    );
}

#[test]
fn start_create_name_conflict_reports_conflicting_root_even_with_missing_launch_directory() {
    let fixture = support::temp_workspace("nested");
    let other_repo = support::temp_git_repo();
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let other_root = other_repo.path().canonicalize().unwrap();
    let other_root = other_root.to_str().unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("run", "the container name is already in use", 125);
    let mut labels = managed_labels(
        other_root,
        &git_root_hash12(Utf8Path::new(other_root)),
        &workspace.container_name,
    );
    labels.remove(LABEL_LAUNCH_DIRECTORY);
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(&workspace.container_name, other_root, true, labels),
    );

    start_command(&harness, target, &[])
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
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect"]
    );
}

#[test]
fn start_create_name_conflict_with_malformed_runtime_label_reports_specific_drift() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("run", "the container name is already in use", 125);
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            labels_with_runtime(
                managed_labels(
                    workspace.canonical_git_root.as_str(),
                    &workspace.hash12,
                    &workspace.container_name,
                ),
                "future-runtime",
            ),
        ),
    );

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "unsupported or malformed `io.agentbox.runtime` label",
        ))
        .stderr(predicates::str::contains(
            "clean up or recreate it before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect"]
    );
}

#[test]
fn start_failure_includes_container_logs_when_available() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("run", "container failed to start", 125);
    harness.write_logs(&workspace.container_name, "runtime boot failed\n");

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "failed to start the runtime server command",
        ))
        .stderr(predicates::str::contains(format!(
            "podman logs --tail 80 {}",
            workspace.container_name
        )))
        .stderr(predicates::str::contains("runtime boot failed"));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect", "logs"]
    );
}

#[test]
fn start_readiness_failure_includes_container_logs_when_available() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.write_inspect(
        &workspace.container_name,
        &opencode_workspace_inspect_fixture(workspace, false, true),
    );
    harness.write_logs(&workspace.container_name, "runtime crashed before listen\n");

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "exited before the `opencode` runtime server became reachable",
        ))
        .stderr(predicates::str::contains(format!(
            "podman logs --tail 80 {}",
            workspace.container_name
        )))
        .stderr(predicates::str::contains("runtime crashed before listen"));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect", "logs"]
    );
}

#[test]
fn start_sigint_during_readiness_stops_container_and_removes_new_cache_volume() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            unused_local_port(),
        ),
    );

    let output = interrupt_start_after_first_inspect(&harness, workspace, target);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("start interrupted before managed session"));
    assert!(stderr.contains("removed newly-created cache volume"));
    assert!(stderr.contains("default runtime image was left untouched"));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("volume ")
            && line.contains(&format!("args=exists {}", workspace.container_name))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("stop ")
            && line.contains(&format!("args=--ignore {}", workspace.container_name))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("container-exists ")
            && line.contains(&format!("args={}", workspace.container_name))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("volume ")
            && line.contains(&format!("args=rm {}", workspace.container_name))
    }));
    assert!(!log.iter().any(|line| {
        line.starts_with("image ")
            && line.contains(&format!(
                "args=rm {}",
                RuntimeKind::Opencode.default_image()
            ))
    }));
}

#[test]
fn start_sigint_during_readiness_preserves_preexisting_cache_volume() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.mark_volume_exists(&workspace.container_name);
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            unused_local_port(),
        ),
    );

    let output = interrupt_start_after_first_inspect(&harness, workspace, target);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("start interrupted before managed session"));
    assert!(stderr.contains("preserved existing cache volume"));

    let log = harness.read_log();
    assert!(log.iter().any(|line| {
        line.starts_with("volume ")
            && line.contains(&format!("args=exists {}", workspace.container_name))
    }));
    assert!(log.iter().any(|line| {
        line.starts_with("stop ")
            && line.contains(&format!("args=--ignore {}", workspace.container_name))
    }));
    assert!(!log.iter().any(|line| {
        line.starts_with("volume ")
            && line.contains(&format!("args=rm {}", workspace.container_name))
    }));
}

#[test]
fn start_sigint_reports_partial_cleanup_when_cache_volume_removal_fails() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("volume-rm", "volume removal exploded", 125);
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            unused_local_port(),
        ),
    );

    let output = interrupt_start_after_first_inspect(&harness, workspace, target);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("partial cleanup failed"));
    assert!(stderr.contains("cache volume removal failed"));
    assert!(stderr.contains("volume removal exploded"));
    assert!(stderr.contains("default runtime image was left untouched"));
}

#[test]
fn start_failed_session_with_missing_labels_requires_cleanup_or_recreation() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "failed-id",
        workspace,
    )]));
    let mut labels = opencode_workspace_labels(workspace);
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
            opencode_workspace_labels(workspace)
                .into_iter()
                .filter(|(key, _)| key != LABEL_RUNTIME)
                .collect(),
        ),
    );

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required session labels"))
        .stderr(predicates::str::contains(
            "clean up or recreate it before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn start_failed_session_with_missing_cache_mount_requires_recreation() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "missing-cache-id",
        workspace,
    )]));
    harness.write_inspect(
        "missing-cache-id",
        &opencode_workspace_inspect_fixture(workspace, true, false),
    );
    harness.write_inspect(
        &workspace.container_name,
        &opencode_workspace_inspect_fixture(workspace, true, false),
    );

    start_command(&harness, target, &[])
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

#[test]
fn start_failed_session_with_cache_bind_mount_requires_recreation() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "cache-bind-id",
        workspace,
    )]));
    harness.write_inspect(
        "cache-bind-id",
        &opencode_workspace_inspect_fixture_with_cache_bind(workspace),
    );
    harness.write_inspect(
        &workspace.container_name,
        &opencode_workspace_inspect_fixture_with_cache_bind(workspace),
    );

    start_command(&harness, target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required cache mount"))
        .stderr(predicates::str::contains(
            REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn start_with_connect_reports_client_failure_without_cleaning_up_ready_session() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_operation("opencode", "client exploded", 42);
    let endpoint = support::ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    start_command(&harness, target, &["--connect"])
        .failure()
        .stderr(predicates::str::contains("client exploded"))
        .stderr(predicates::str::contains(
            "failed to connect to newly created managed session",
        ))
        .stderr(predicates::str::contains("The session remains running"))
        .stderr(predicates::str::contains(format!(
            "agentbox connect {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )));
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect", "opencode"]
    );
    assert!(!log.iter().any(|line| line.starts_with("stop ")));
    assert!(!log.iter().any(|line| line.starts_with("rm ")));
    assert!(
        !log.iter()
            .any(|line| line.starts_with("volume ") && line.contains("args=rm "))
    );
}

fn install_harness() -> Harness {
    Harness::new()
}

fn start_command(
    harness: &Harness,
    target: &Path,
    extra_args: &[&str],
) -> assert_cmd::assert::Assert {
    harness.start_assert_with_args(target, extra_args)
}

fn labels_with_runtime(
    mut labels: std::collections::BTreeMap<String, String>,
    runtime: &str,
) -> std::collections::BTreeMap<String, String> {
    labels.insert(LABEL_RUNTIME.to_string(), runtime.to_string());
    labels
}

fn interrupt_start_after_first_inspect(
    harness: &Harness,
    workspace: &agentbox::workspace::WorkspaceIdentity,
    target: &Path,
) -> Output {
    let mut command = harness.locked_agentbox_process_command(workspace);
    command
        .args(["start", "--runtime", "opencode"])
        .arg(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().unwrap();
    wait_for_log_line(harness, "inspect ");
    send_sigint(child.id());
    child.wait_with_output().unwrap()
}

fn wait_for_log_line(harness: &Harness, prefix: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if harness
            .read_log()
            .iter()
            .any(|line| line.starts_with(prefix))
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    panic!("timed out waiting for podman log line starting with `{prefix}`");
}

fn send_sigint(pid: u32) {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    assert_eq!(
        result,
        0,
        "failed to send SIGINT to child process {pid}: {}",
        std::io::Error::last_os_error(),
    );
}

fn unused_local_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

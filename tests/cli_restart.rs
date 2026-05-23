// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use agentbox::runtime::RuntimeKind;
use agentbox::workspace::resolve_workspace_identity;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, ReadyEndpoint, managed_inspect_fixture, managed_ps_entry,
    opencode_transient_run_labels as transient_run_labels, opencode_workspace_inspect_fixture,
    operation_names, ps_fixture, running_workspace_inspect_fixture,
    running_workspace_inspect_fixture_with_host_port, transient_run_ps_entry, workspace_ps_entry,
};

#[test]
fn restart_recreates_running_session_with_stored_runtime_and_launch_directory() {
    let repo = support::temp_git_repo();
    let launch_target = repo.path().join("launch");
    let request_target = repo.path().join("request");
    fs::create_dir(&launch_target).unwrap();
    fs::create_dir(&request_target).unwrap();
    let launch_workspace = resolve_workspace_identity(&launch_target).unwrap();
    let request_workspace = resolve_workspace_identity(&request_target).unwrap();
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "running-id",
        &launch_workspace.container_name,
        &launch_workspace.hash12,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(&launch_workspace, &image, RuntimeKind::Codex),
    );
    harness.write_inspect(
        &launch_workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            &launch_workspace,
            &image,
            RuntimeKind::Codex,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(&launch_workspace);
    command.arg("restart").arg(&request_target);

    let expected_endpoint = format!("ws://127.0.0.1:{}", endpoint.port());
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("INFO: resolving restart target")
                .and(predicate::str::contains(
                    "INFO: checking workspace prerequisites",
                ))
                .and(predicate::str::contains("INFO: building runtime image"))
                .and(predicate::str::contains("INFO: stopping managed session"))
                .and(predicate::str::contains(
                    "INFO: starting replacement container",
                ))
                .and(predicate::str::contains(format!(
                    "restarted and ready at `{expected_endpoint}`"
                ))),
        );
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        [
            "ps",
            "inspect",
            "image",
            "build",
            "stop",
            "container-exists",
            "run",
            "inspect"
        ]
    );
    commands.entry(0).assert_lock_held();
    commands.entry(4).assert_lock_held();
    let run = commands.first("run");
    run.assert_args_contain("--label io.agentbox.runtime=codex");
    run.assert_args_contain("--label io.agentbox.container_kind=managed-session");
    run.assert_args_contain(&format!("--workdir {}", launch_workspace.canonical_target));
    run.assert_args_contain(&format!(
        "--label io.agentbox.launch_directory={}",
        launch_workspace.canonical_target
    ));
    run.assert_args_do_not_contain(request_workspace.canonical_target.as_ref());
    run.assert_args_contain(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox app-server --listen ws://0.0.0.0:1455"
    ));
    run.assert_args_contain("--ws-auth capability-token");
    run.assert_args_contain("--ws-token-sha256 ");
    run.assert_args_contain(&format!(
        "type=volume,src={},dst=/home/user,U",
        launch_workspace.container_name
    ));
    let token = fs::read_to_string(harness.codex_attach_token_path(&launch_workspace)).unwrap();
    assert!(!token.trim().is_empty());
    run.assert_args_do_not_contain(token.trim());
}

#[test]
fn restart_passes_codex_home_to_replacement_server_container() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    let codex_home = harness.home_path().join("restart-codex-home");
    fs::create_dir(&codex_home).unwrap();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(workspace, &image, RuntimeKind::Codex),
    );
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Codex,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("CODEX_HOME", &codex_home)
        .arg("restart")
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let run = harness.command_log().first("run").clone();
    run.assert_args_contain(&format!(
        "type=bind,src={},dst={}",
        codex_home.display(),
        codex_home.display()
    ));
    run.assert_args_contain(&format!("--env CODEX_HOME={}", codex_home.display()));
    run.assert_args_do_not_contain("dst=/home/user/.codex");
}

#[test]
fn restart_stable_id_target_is_revalidated_under_the_git_root_lock() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(workspace, &image, RuntimeKind::Opencode),
    );
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(&workspace.hash12[..6]);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        [
            "ps",
            "inspect",
            "ps",
            "inspect",
            "image",
            "stop",
            "container-exists",
            "run",
            "inspect"
        ]
    );
    commands.entry(2).assert_lock_held();
    commands.entry(5).assert_lock_held();
}

#[test]
fn restart_re_evaluates_dev_environment_from_stored_launch_directory() {
    let repo = support::temp_git_repo();
    let launch_target = repo.path().join("launch");
    let request_target = repo.path().join("request");
    fs::create_dir(&launch_target).unwrap();
    fs::create_dir(&request_target).unwrap();
    support::write_envrc_at(&launch_target);
    let launch_workspace = resolve_workspace_identity(&launch_target).unwrap();
    let request_workspace = resolve_workspace_identity(&request_target).unwrap();
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "running-id",
        &launch_workspace.container_name,
        &launch_workspace.hash12,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(&launch_workspace, &image, RuntimeKind::Opencode),
    );
    harness.write_inspect(
        &launch_workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            &launch_workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(&launch_workspace);
    command.arg("restart").arg(&request_target);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    let run = commands.first("run");
    run.assert_args_contain(&format!("--workdir {}", launch_workspace.canonical_target));
    run.assert_args_contain("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096");
    run.assert_args_do_not_contain(request_workspace.canonical_target.as_ref());
}

#[test]
fn restart_dev_env_none_disables_wrapper_re_evaluation() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    fixture.write_envrc();
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(workspace, &image, RuntimeKind::Opencode),
    );
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["restart", "--dev-env", "none"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    let run = commands.first("run");
    run.assert_args_contain(" opencode serve --hostname 0.0.0.0 --port 4096");
    run.assert_args_do_not_contain("direnv exec .");
}

#[test]
fn restart_with_connect_runs_host_client_from_stored_launch_directory() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(workspace, &image, RuntimeKind::Opencode),
    );
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["restart", "--connect"]).arg(target);

    let expected_endpoint = format!("http://127.0.0.1:{}", endpoint.port());
    command
        .assert()
        .success()
        .stderr(predicate::str::contains("restarted and ready"));
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        [
            "ps",
            "inspect",
            "image",
            "stop",
            "container-exists",
            "run",
            "inspect",
            "opencode"
        ]
    );
    commands.entry(7).assert_lock_held();
    commands
        .entry(7)
        .assert_raw_contains(&format!("attach {expected_endpoint}"));
    commands
        .entry(7)
        .assert_raw_contains(&format!("cwd={}", workspace.canonical_target));
}

#[test]
fn restart_dev_env_failure_does_not_stop_existing_session() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    fixture.write_target_flake();
    let harness = Harness::new();
    harness.fail_nix_eval(&fixture.target, "flake exploded\n");
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to evaluate dev shell"))
        .stderr(predicate::str::contains("flake exploded"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "nix"]);
    assert!(!log.iter().any(|line| line.starts_with("stop ")));
    assert!(!log.iter().any(|line| line.starts_with("run ")));
}

#[test]
fn restart_stop_verification_failure_does_not_start_replacement() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.mark_container_exists(&workspace.container_name);
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("replacement was not started"))
        .stderr(predicate::str::contains(
            "container still exists after stop",
        ));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "image", "stop", "container-exists"]
    );
    assert!(!log.iter().any(|line| line.starts_with("run ")));
}

#[test]
fn restart_rejects_transient_run_container_target() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![transient_run_ps_entry(
        "run-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "run-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            transient_run_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                &workspace.container_name,
            ),
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("transient run container"))
        .stderr(predicate::str::contains("cannot be restarted"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("stop ")));
}

#[test]
fn restart_rejects_stopped_managed_session() {
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
        &opencode_workspace_inspect_fixture(workspace, false, true),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not running"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("stop ")));
}

#[test]
fn restart_replacement_readiness_failure_reports_old_session_may_be_gone() {
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
        &running_workspace_inspect_fixture(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
        ),
    );
    harness.write_inspect(
        &workspace.container_name,
        &opencode_workspace_inspect_fixture(workspace, false, true),
    );
    harness.write_logs(&workspace.container_name, "replacement crashed\n");

    let mut command = harness.locked_agentbox_command(workspace);
    command.arg("restart").arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("previous managed session"))
        .stderr(predicate::str::contains("may already be gone"))
        .stderr(predicate::str::contains("replacement crashed"));

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "inspect",
            "image",
            "stop",
            "container-exists",
            "run",
            "inspect",
            "logs"
        ]
    );
}

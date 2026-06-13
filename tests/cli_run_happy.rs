// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use agentbox::metadata::LABEL_SERVER_ARGS;
use agentbox::runtime::RuntimeKind;
use agentbox::runtime::default_image::default_image_context_hash;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, ReadyEndpoint, operation_names,
    running_workspace_inspect_fixture_with_host_port,
};

#[test]
fn run_launches_opencode_transient_server_and_host_client() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let context_hash = default_image_context_hash();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command.args(["run", "--runtime", "opencode"]).arg(target);

    let expected_endpoint = format!("http://127.0.0.1:{}", endpoint.port());
    command.assert().success().stderr(
        predicate::str::contains("INFO: checking workspace prerequisites")
            .and(predicate::str::contains(
                "INFO: checking existing agentbox containers",
            ))
            .and(predicate::str::contains("INFO: building runtime image"))
            .and(predicate::str::contains(
                "INFO: starting transient container",
            ))
            .and(predicate::str::contains(
                "INFO: waiting for `opencode` runtime server",
            ))
            .and(predicate::str::contains(format!(
                "INFO: transient container `{}` for `{}` is ready at `{expected_endpoint}`; connecting",
                workspace.container_name, workspace.canonical_git_root
            )))
            .and(predicate::str::contains(format!(
                "INFO: stopping transient container `{}`",
                workspace.container_name
            )))
            .and(predicate::str::contains("use `agentbox connect`").not()),
    );
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        [
            "ps",
            "image",
            "build",
            "run",
            "inspect",
            "opencode",
            "stop",
            "container-exists"
        ]
    );
    let build = commands.first("build");
    let run = commands.first("run");

    run.assert_lock_held();
    build.assert_args_contain(&format!("-t {image} -f"));
    build.assert_args_contain("--build-arg AGENTBOX_RUNTIME=opencode");
    build.assert_args_contain(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    ));
    run.assert_args_contain("--rm");
    run.assert_args_contain("--detach");
    run.assert_args_do_not_contain("--interactive");
    run.assert_args_do_not_contain("--tty");
    run.assert_args_contain("--publish 127.0.0.1::4096");
    run.assert_args_do_not_contain("--label io.agentbox.managed=true");
    run.assert_args_contain("--label io.agentbox.container_kind=transient-run");
    run.assert_args_contain(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    ));
    run.assert_args_contain(&format!(
        "--label io.agentbox.git_root_hash={}",
        workspace.hash12
    ));
    run.assert_args_contain("--label io.agentbox.runtime=opencode");
    run.assert_args_contain(&format!("--label io.agentbox.image={image}"));
    run.assert_args_contain(&format!(
        "--label io.agentbox.launch_directory={}",
        workspace.canonical_target
    ));
    run.assert_args_contain(&format!(
        "--label io.agentbox.logical_name={}",
        workspace.container_name
    ));
    run.assert_args_contain("--label io.agentbox.attach_scheme=http");
    run.assert_args_contain("--label io.agentbox.container_port=4096");
    run.assert_args_contain("--label io.agentbox.container_listen_ip=0.0.0.0");
    run.assert_args_contain(&format!("--name {}", workspace.container_name));
    run.assert_args_contain(&format!("--workdir {}", workspace.canonical_target));
    run.assert_args_contain(&format!(
        "type=volume,src={},dst=/home/user,U",
        workspace.container_name
    ));
    run.assert_args_contain(&format!(
        " {image} opencode serve --hostname 0.0.0.0 --port 4096"
    ));
    commands
        .entry(5)
        .assert_raw_contains(&format!("attach {expected_endpoint}"));
    commands
        .entry(5)
        .assert_raw_contains(&format!("cwd={}", workspace.canonical_target));
    commands
        .entry(6)
        .assert_args_contain(&format!("--ignore {}", workspace.container_name));
}

#[test]
fn run_applies_configured_resource_limits_and_cli_overrides() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.write_agentbox_config(r#"{"defaultResourceLimits":{"cpus":2,"memory":"8g"}}"#);
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command
        .args(["run", "--runtime", "opencode", "--cpus", "1.5"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let run = harness.command_log().first("run").clone();
    run.assert_args_contain("--cpus 1.5");
    run.assert_args_contain("--memory 8g");
}

#[test]
fn run_zero_resource_limit_disables_that_configured_limit_only() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.write_agentbox_config(r#"{"defaultResourceLimits":{"cpus":2,"memory":"8g"}}"#);
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command
        .args(["run", "--runtime", "opencode", "--cpus", "0"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let run = harness.command_log().first("run").clone();
    run.assert_args_do_not_contain("--cpus");
    run.assert_args_contain("--memory 8g");
}

#[test]
fn run_launches_codex_transient_server_and_host_client_in_yolo_mode() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
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
    command.args(["run", "--runtime", "codex"]).arg(target);

    let expected_endpoint = format!("ws://127.0.0.1:{}", endpoint.port());
    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "image",
            "run",
            "inspect",
            "codex",
            "stop",
            "container-exists"
        ]
    );
    let run = podman_run_command(&log);
    assert_runtime_user_args(run);
    assert!(run.contains(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox app-server --listen ws://0.0.0.0:1455"
    )));
    assert!(run.contains("--ws-auth capability-token"));
    assert!(run.contains("--ws-token-sha256 "));
    assert!(run.contains("--publish 127.0.0.1::1455"));
    assert!(run.contains(&format!(
        "type=bind,src={},dst=/home/user/.codex",
        harness.home_path().join(".codex").display()
    )));
    assert!(!run.contains("--env CODEX_HOME="));
    assert!(!run.contains("--label io.agentbox.managed=true"));
    assert!(run.contains("--label io.agentbox.container_kind=transient-run"));
    assert!(log[4].contains(&format!(
        "codex lock=held args=--dangerously-bypass-approvals-and-sandbox --remote {expected_endpoint}"
    )));
    assert!(log[4].contains("--remote-auth-token-env AGENTBOX_CODEX_REMOTE_TOKEN"));
    assert!(log[4].contains(&format!("cwd={}", workspace.canonical_target)));
}

#[test]
fn run_passes_agent_args_to_host_client_only() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    let endpoint_port = endpoint.port();
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &image,
            RuntimeKind::Opencode,
            endpoint_port,
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .args(["run", "--runtime", "opencode"])
        .arg(target)
        .args(["--", "--no-alt-screen"]);

    command.assert().success();
    endpoint.wait();

    let expected_endpoint = format!("http://127.0.0.1:{endpoint_port}");
    let commands = harness.command_log();
    let run = commands.first("run");
    run.assert_args_contain(&format!(
        " {image} opencode serve --hostname 0.0.0.0 --port 4096"
    ));
    run.assert_args_do_not_contain("--no-alt-screen");
    commands
        .entry(4)
        .assert_raw_contains(&format!("attach {expected_endpoint} --no-alt-screen"));
}

#[test]
fn run_passes_codex_home_to_codex_server_container() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    let codex_home = harness.home_path().join("custom-codex-home");
    fs::create_dir(&codex_home).unwrap();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
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
        .args(["run", "--runtime", "codex"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(run.contains(&format!(
        "type=bind,src={},dst={}",
        codex_home.display(),
        codex_home.display()
    )));
    assert!(run.contains(&format!("--env CODEX_HOME={}", codex_home.display())));
    assert!(!run.contains("dst=/home/user/.codex"));
}

#[test]
fn exec_launches_codex_exec_foreground_without_managed_metadata() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    let codex_home = harness.home_path().join("custom-codex-home");
    fs::create_dir(&codex_home).unwrap();

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("CODEX_HOME", &codex_home)
        .args(["exec", "--dev-env", "none"])
        .arg(target)
        .args(["--", "--json", "fix-tests"]);

    command.assert().success().stderr(
        predicate::str::contains("INFO: checking workspace prerequisites")
            .and(predicate::str::contains(
                "INFO: checking existing managed sessions",
            ))
            .and(predicate::str::contains(
                "INFO: starting foreground container",
            ))
            .and(predicate::str::contains("waiting for").not())
            .and(predicate::str::contains("is ready at").not()),
    );

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "run"]);
    let run = podman_run_command(&log);
    assert_runtime_user_args(run);
    assert!(run.contains("--rm"));
    assert!(run.contains("--interactive"));
    assert!(!run.contains("--detach"));
    assert!(!run.contains("--tty"));
    assert!(!run.contains("--publish"));
    assert!(!run.contains("--label io.agentbox.managed=true"));
    assert!(!run.contains("--label io.agentbox.attach_scheme"));
    assert!(!run.contains("--label io.agentbox.container_port"));
    assert!(run.contains(&format!("--name {}", workspace.container_name)));
    assert!(run.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(run.contains(&format!(
        "type=volume,src={},dst=/home/user,U",
        workspace.container_name
    )));
    assert!(run.contains(&format!(
        "type=bind,src={},dst=/home/user/.codex",
        harness.home_path().join(".codex").display()
    )));
    assert!(!run.contains(&format!("{}", codex_home.display())));
    assert!(!run.contains("--env CODEX_HOME="));
    assert!(run.contains(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox exec --disable codex_git_commit --json fix-tests"
    )));
    assert!(!run.contains("app-server"));
    assert!(!log.iter().any(|line| line.starts_with("codex ")));
}

#[test]
fn exec_uses_codex_git_identity_without_ssh_agent() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    harness.write_git_config("user.name", "Alice Agent\n");
    harness.write_git_config("user.email", "alice@example.test\n");

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .args(["exec", "--dev-env", "none"])
        .arg(target)
        .arg("--")
        .arg("fix-tests");

    command.assert().success();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(run.contains("--env GIT_CONFIG_COUNT=2"));
    assert!(run.contains("--env GIT_CONFIG_KEY_0=user.name"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Codex"));
    assert!(run.contains("--env GIT_CONFIG_KEY_1=user.email"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=noreply@openai.com"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_NAME=Codex"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL=noreply@openai.com"));
    assert!(!run.contains("Alice Agent"));
    assert!(!run.contains("alice@example.test"));
    assert!(!run.contains("/run/agentbox/ssh-agent.sock"));
}

#[test]
fn run_without_host_git_identity_does_not_set_identity_env() {
    let fixture = support::temp_workspace("nested");
    let harness = Harness::new();

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(!run.contains("--env GIT_CONFIG_COUNT="));
    assert!(!run.contains("--env AGENTBOX_GIT_IDENTITY_NAME="));
    assert!(!run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL="));
}

#[test]
fn run_mounts_host_git_excludes_file_and_injects_config() {
    let fixture = support::temp_workspace("nested");
    let harness = Harness::new();
    let git_ignore = fixture.repo.path().join("host-ignore");
    fs::write(&git_ignore, "target\n").unwrap();
    harness.write_git_config("user.name", "Alice Agent\n");
    harness.write_git_config("user.email", "alice@example.test\n");
    harness.write_git_config_path("core.excludesFile", &format!("{}\n", git_ignore.display()));

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!(
        "--mount type=bind,src={},dst=/run/agentbox/git-ignore,ro",
        git_ignore.display()
    )));
    assert!(run.contains("--env GIT_CONFIG_COUNT=3"));
    assert!(run.contains("--env GIT_CONFIG_KEY_0=user.name"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Alice Agent"));
    assert!(run.contains("--env GIT_CONFIG_KEY_1=user.email"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=alice@example.test"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_NAME=Alice Agent"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL=alice@example.test"));
    assert!(run.contains("--env GIT_CONFIG_KEY_2=core.excludesFile"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_2=/run/agentbox/git-ignore"));
}

#[test]
fn exec_mounts_host_git_excludes_file_with_codex_identity() {
    let fixture = support::temp_workspace("nested");
    let harness = Harness::new();
    let git_ignore = fixture.repo.path().join("host-ignore");
    fs::write(&git_ignore, "target\n").unwrap();
    harness.write_git_config_path("core.excludesFile", &format!("{}\n", git_ignore.display()));

    let log = exec_codex_success(&fixture, &harness, &["--dev-env", "none"], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!(
        "--mount type=bind,src={},dst=/run/agentbox/git-ignore,ro",
        git_ignore.display()
    )));
    assert!(run.contains("--env GIT_CONFIG_COUNT=3"));
    assert!(run.contains("--env GIT_CONFIG_KEY_0=user.name"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Codex"));
    assert!(run.contains("--env GIT_CONFIG_KEY_1=user.email"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=noreply@openai.com"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_NAME=Codex"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL=noreply@openai.com"));
    assert!(run.contains("--env GIT_CONFIG_KEY_2=core.excludesFile"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_2=/run/agentbox/git-ignore"));
}

#[cfg(unix)]
#[test]
fn run_mounts_ssh_agent_socket_git_excludes_and_minimal_git_signing_config() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let (_socket_dir, socket_path, _listener) = bind_test_socket();
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
    let git_ignore = fixture.repo.path().join("host-ignore");
    fs::write(&git_ignore, "target\n").unwrap();
    harness.write_git_config("user.name", "Alice Agent\n");
    harness.write_git_config("user.email", "alice@example.test\n");
    harness.write_git_config_path("core.excludesFile", &format!("{}\n", git_ignore.display()));
    harness.write_git_config("gpg.format", "ssh\n");
    harness.write_git_config("user.signingkey", "ssh-ed25519 AAAATEST alice\n");
    harness.write_git_config("commit.gpgsign", "true\n");
    let config_dir = harness.home_path().join(".config/agentbox");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.json"),
        r#"{"knownHosts":["github.com ssh-ed25519 AAAACONFIG"]}"#,
    )
    .unwrap();

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("SSH_AUTH_SOCK", socket_path.as_str())
        .args(["run", "--runtime", "opencode"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(run.contains(&format!(
        "--mount type=bind,src={},dst=/run/agentbox/ssh-agent.sock",
        socket_path
    )));
    assert!(run.contains(&format!(
        "--mount type=bind,src={},dst=/run/agentbox/git-ignore,ro",
        git_ignore.display()
    )));
    assert!(run.contains("--env SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock"));
    assert!(run.contains("dst=/run/agentbox/known_hosts,ro"));
    assert!(
        run.contains(
            "--env GIT_SSH_COMMAND=ssh -o UserKnownHostsFile=/run/agentbox/known_hosts -o StrictHostKeyChecking=yes"
        )
    );
    assert!(run.contains("--env GIT_CONFIG_COUNT=6"));
    assert!(run.contains("--env GIT_CONFIG_KEY_0=user.name"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Alice Agent"));
    assert!(run.contains("--env GIT_CONFIG_KEY_1=user.email"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=alice@example.test"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_NAME=Alice Agent"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL=alice@example.test"));
    assert!(run.contains("--env GIT_CONFIG_KEY_2=core.excludesFile"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_2=/run/agentbox/git-ignore"));
    assert!(run.contains("--env GIT_CONFIG_KEY_3=gpg.format"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_3=ssh"));
    assert!(run.contains("--env GIT_CONFIG_KEY_4=user.signingkey"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_4=ssh-ed25519 AAAATEST alice"));
    assert!(run.contains("--env GIT_CONFIG_KEY_5=commit.gpgsign"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_5=true"));
    assert!(!run.contains("credential.helper"));
    assert_eq!(
        harness.captured_known_hosts().as_deref(),
        Some("github.com ssh-ed25519 AAAACONFIG\n")
    );
}

#[cfg(unix)]
#[test]
fn run_adds_matching_host_known_hosts_entries_from_ssh_config_file() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let (_socket_dir, socket_path, _listener) = bind_test_socket();
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
    harness.write_git_remotes("origin\tgit@github.com:owner/repo.git (fetch)\n");
    let ssh_dir = harness.home_path().join(".ssh");
    fs::create_dir(&ssh_dir).unwrap();
    fs::write(
        ssh_dir.join("known_hosts.custom"),
        "github.com ssh-ed25519 AAAAHOST\n",
    )
    .unwrap();
    harness.write_fake_program(
        "ssh",
        r#"#!/bin/sh
set -eu
if [ "$1" = "-G" ] && [ "$2" = "--" ] && [ "$3" = "github.com" ]; then
  printf 'hostname github.com\n'
  printf 'port 22\n'
  printf 'userknownhostsfile %s/.ssh/known_hosts.custom\n' "$HOME"
  printf 'globalknownhostsfile none\n'
  exit 0
fi
exit 255
"#,
    );
    harness.write_fake_program(
        "ssh-keygen",
        r#"#!/bin/sh
set -eu
if [ "$1" = "-F" ] && [ "$2" = "github.com" ] && [ "$3" = "-f" ] && [ "$4" = "$HOME/.ssh/known_hosts.custom" ]; then
  printf '# Host github.com found: line 1\n'
  printf 'github.com ssh-ed25519 AAAAHOST\n'
  exit 0
fi
exit 1
"#,
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("SSH_AUTH_SOCK", socket_path.as_str())
        .args(["run", "--runtime", "opencode"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(run.contains("dst=/run/agentbox/known_hosts,ro"));
    assert_eq!(
        harness.captured_known_hosts().as_deref(),
        Some("github.com ssh-ed25519 AAAAHOST\n")
    );
}

#[cfg(unix)]
#[test]
fn run_warns_and_continues_when_remote_host_key_is_missing() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let (_socket_dir, socket_path, _listener) = bind_test_socket();
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
    harness.write_git_remotes("origin\tgit@gitlab.com:group/repo.git (fetch)\n");
    harness.write_fake_program(
        "ssh",
        r#"#!/bin/sh
set -eu
if [ "$1" = "-G" ] && [ "$2" = "--" ] && [ "$3" = "gitlab.com" ]; then
  printf 'hostname gitlab.com\n'
  printf 'port 22\n'
  printf 'userknownhostsfile %s/.ssh/known_hosts\n' "$HOME"
  printf 'globalknownhostsfile none\n'
  exit 0
fi
exit 255
"#,
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("SSH_AUTH_SOCK", socket_path.as_str())
        .args(["run", "--runtime", "opencode"])
        .arg(target);

    command.assert().success().stderr(predicate::str::contains(
        "WARNING: no known_hosts entry found for SSH remote host `gitlab.com`",
    ));
    endpoint.wait();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(!run.contains("/run/agentbox/known_hosts"));
    assert!(harness.captured_known_hosts().is_none());
}

#[cfg(unix)]
#[test]
fn run_backs_up_invalid_agentbox_config_and_continues_without_entries() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let (_socket_dir, socket_path, _listener) = bind_test_socket();
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
    let config_dir = harness.home_path().join(".config/agentbox");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.json"), r#"{"knownHosts":[""]}"#).unwrap();

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("SSH_AUTH_SOCK", socket_path.as_str())
        .args(["run", "--runtime", "opencode"])
        .arg(target);

    command.assert().success().stderr(
        predicate::str::contains("WARNING: agentbox config")
            .and(predicate::str::contains("backed it up")),
    );
    endpoint.wait();

    let backups = fs::read_dir(&config_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(!config_dir.join("config.json").exists());
    assert_eq!(backups.len(), 1);
    assert!(backups[0].starts_with("config.json.bak."));
    assert!(harness.captured_known_hosts().is_none());
}

#[cfg(unix)]
#[test]
fn start_mounts_ssh_agent_socket_when_available() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let (_socket_dir, socket_path, _listener) = bind_test_socket();
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
    command
        .env("SSH_AUTH_SOCK", socket_path.as_str())
        .args(["start", "--runtime", "opencode"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(run.contains(&format!(
        "--mount type=bind,src={},dst=/run/agentbox/ssh-agent.sock",
        socket_path
    )));
    assert!(run.contains("--env SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock"));
}

#[test]
fn exec_warns_and_skips_invalid_ssh_auth_sock() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let stale_socket = fixture.repo.path().join("missing-agent.sock");

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .env("SSH_AUTH_SOCK", &stale_socket)
        .args(["exec", "--dev-env", "none"])
        .arg(target)
        .arg("--")
        .arg("fix-tests");

    command.assert().success().stderr(predicate::str::contains(
        "WARNING: SSH_AUTH_SOCK does not reference a usable Unix socket",
    ));

    let log = harness.read_log();
    let run = podman_run_command(&log);
    assert!(!run.contains("/run/agentbox/ssh-agent.sock"));
    assert!(run.contains("--env GIT_CONFIG_COUNT=2"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Codex"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=noreply@openai.com"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_NAME=Codex"));
    assert!(run.contains("--env AGENTBOX_GIT_IDENTITY_EMAIL=noreply@openai.com"));
}

#[test]
fn run_wraps_server_command_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    fixture.write_envrc();
    let harness = Harness::new();

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(run.contains("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096"));
}

#[test]
fn exec_wraps_codex_exec_command_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    fixture.write_envrc();
    let harness = Harness::new();

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(
        run.contains(
            "direnv exec . codex --dangerously-bypass-approvals-and-sandbox exec --disable codex_git_commit fix-tests"
        )
    );
    assert!(!run.contains("app-server"));
}

#[test]
fn exec_wraps_codex_exec_command_with_devenv_when_selected() {
    let fixture = support::temp_workspace("nested");
    fixture.write_repo_devenv();
    let harness = Harness::new();

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert!(run.contains(
        "devenv shell --no-tui -- codex --dangerously-bypass-approvals-and-sandbox exec --disable codex_git_commit fix-tests"
    ));
    assert!(!run.contains("app-server"));
}

#[test]
fn exec_wraps_codex_exec_command_with_nix_develop_when_selected() {
    let fixture = support::temp_workspace("nested");
    fixture.write_target_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(&fixture.target, "default");

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert_eq!(operation_names(&log), ["ps", "nix", "image", "run"]);
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#default --command codex --dangerously-bypass-approvals-and-sandbox exec --disable codex_git_commit fix-tests",
        fixture.workspace.canonical_target
    )));
}

#[test]
fn run_wraps_server_command_with_devenv_when_selected() {
    let fixture = support::temp_workspace("nested");
    fixture.write_repo_devenv();
    let harness = Harness::new();

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains("devenv shell --no-tui -- opencode serve --hostname 0.0.0.0 --port 4096"));
}

#[test]
fn run_wraps_server_command_with_nix_develop_when_selected() {
    let fixture = support::temp_workspace("nested");
    fixture.write_target_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(&fixture.target, "default");

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert_eq!(
        operation_names(&log),
        [
            "ps",
            "nix",
            "image",
            "run",
            "inspect",
            "opencode",
            "stop",
            "container-exists"
        ]
    );
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#default --command opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_target
    )));
}

#[test]
fn start_creates_serves_waits_and_suggests_connect_for_a_new_session() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let context_hash = default_image_context_hash();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command.args(["start", "--runtime", "opencode"]).arg(target);

    let expected_endpoint = format!("http://127.0.0.1:{}", endpoint.port());
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("INFO: checking workspace prerequisites")
                .and(predicate::str::contains(
                    "INFO: checking existing agentbox containers",
                ))
                .and(predicate::str::contains("INFO: building runtime image"))
                .and(predicate::str::contains("INFO: starting container"))
                .and(predicate::str::contains(
                    "INFO: waiting for `opencode` runtime server",
                ))
                .and(predicate::str::contains(format!(
                    "INFO: managed session `{}` for `{}` is ready at `{expected_endpoint}`",
                    workspace.container_name, workspace.canonical_git_root
                ))),
        );
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        ["ps", "image", "build", "volume", "run", "inspect"]
    );

    for entry in commands.entries() {
        entry.assert_lock_held();
    }

    let build = commands.first("build");
    build.assert_args_contain(&format!("-t {image} -f"));
    build.assert_args_contain("--build-arg AGENTBOX_RUNTIME=opencode");
    build.assert_args_contain("--build-arg OPENCODE_NPM_VERSION=0.99.0");
    build.assert_args_contain("--label io.agentbox.default_runtime_image=true");
    build.assert_args_contain("--label io.agentbox.runtime=opencode");
    build.assert_args_contain(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    ));
    build.assert_args_contain("--label io.agentbox.opencode.package=opencode-ai");
    build.assert_args_contain("--label io.agentbox.opencode.version=0.99.0");

    commands
        .entry(3)
        .assert_args_contain(&format!("exists {}", workspace.container_name));
    let run = commands.first("run");
    run.assert_args_contain("--rm");
    run.assert_args_do_not_contain("--rmi");
    run.assert_args_contain("--detach");
    assert_runtime_user_args(run.raw());
    run.assert_args_do_not_contain("--interactive");
    run.assert_args_do_not_contain("--tty");
    run.assert_args_contain(&format!("--label io.agentbox.image={image}"));
    run.assert_args_contain("--label io.agentbox.container_kind=managed-session");
    run.assert_args_contain("--label io.agentbox.opencode.version=0.99.0");
    run.assert_args_contain("--label io.agentbox.attach_scheme=http");
    run.assert_args_contain("--label io.agentbox.container_port=4096");
    run.assert_args_contain(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    ));
    run.assert_args_contain(&format!("--name {}", workspace.container_name));
    run.assert_args_contain(&format!("--workdir {}", workspace.canonical_target));
    run.assert_args_contain(&image);
    run.assert_args_contain(" opencode serve --hostname 0.0.0.0 --port 4096");
    run.assert_args_contain("--publish 127.0.0.1::4096");
    run.assert_args_contain("--env OPENCODE_CONFIG_CONTENT={\"autoupdate\":false}");
    run.assert_args_contain("--env OPENCODE_PERMISSION={\"*\":\"allow\"}");
    run.assert_args_contain(&format!(
        "type=bind,src={},dst=/home/user/.config/opencode",
        harness.home_path().join(".config/opencode").display()
    ));
    run.assert_args_contain(&format!(
        "type=bind,src={},dst=/home/user/.local/share/opencode",
        harness.home_path().join(".local/share/opencode").display()
    ));
    run.assert_args_contain("type=volume");
    run.assert_args_contain("dst=/home/user,U");
    run.assert_args_do_not_contain("direnv exec .");
    run.assert_args_do_not_contain("sleep infinity");
    assert!(!commands.contains_operation("stop"));
    assert!(
        !commands
            .entries()
            .iter()
            .any(|entry| entry.operation() == "volume" && entry.args().contains("rm "))
    );

    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/opencode.json");
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"package\": \"opencode-ai\""));
    assert!(state.contains(&format!("\"image\": \"{image}\"")));
    assert!(state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
}

#[test]
fn start_wraps_server_command_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    fixture.write_envrc();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command.args(["start", "--runtime", "opencode"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = log.iter().find(|line| line.starts_with("run ")).unwrap();

    assert!(run.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(run.contains("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096"));
}

#[test]
fn start_prefers_envrc_over_devenv_and_flake() {
    let fixture = support::temp_workspace("nested");
    fixture.write_envrc();
    fixture.write_repo_devenv();
    fixture.write_repo_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(fixture.repo.path(), "nested");

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(!run.contains("devenv shell"));
    assert!(!run.contains("nix develop"));
    assert!(!log.iter().any(|line| line.starts_with("nix ")));
}

#[test]
fn start_uses_parent_devenv_without_changing_container_workdir() {
    let fixture = support::temp_workspace("nested");
    fixture.write_repo_devenv();
    let harness = Harness::new();

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(run.contains("devenv shell --no-tui -- opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(!log.iter().any(|line| line.starts_with("nix ")));
}

#[test]
fn start_uses_target_flake_default_dev_shell() {
    let fixture = support::temp_workspace("nested");
    fixture.write_target_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(&fixture.target, "default");

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert_eq!(
        operation_names(&log),
        ["ps", "nix", "image", "volume", "run", "inspect"]
    );
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#default --command opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_target
    )));
}

#[test]
fn start_uses_parent_flake_basename_dev_shell_before_default() {
    let fixture = support::temp_workspace("nested");
    fixture.write_repo_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(fixture.repo.path(), "nested");

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert_eq!(
        operation_names(&log),
        ["ps", "nix", "image", "volume", "run", "inspect"]
    );
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#nested --command opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_git_root
    )));
}

#[test]
fn start_falls_back_to_parent_flake_default_dev_shell() {
    let fixture = support::temp_workspace("nested");
    fixture.write_repo_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(fixture.repo.path(), "default");

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert_eq!(
        operation_names(&log),
        ["ps", "nix", "nix", "image", "volume", "run", "inspect"]
    );
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#default --command opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_git_root
    )));
}

#[test]
fn start_dev_env_none_disables_all_automatic_wrappers() {
    let fixture = support::temp_workspace("nested");
    fixture.write_envrc();
    fixture.write_repo_devenv();
    fixture.write_repo_flake();
    let harness = Harness::new();
    harness.mark_dev_shell(fixture.repo.path(), "nested");

    let log = start_opencode_success(&fixture, &harness, &["--dev-env", "none"]);
    let run = podman_run_command(&log);

    assert!(run.contains(" opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(!run.contains("direnv exec ."));
    assert!(!run.contains("devenv shell"));
    assert!(!run.contains("nix develop"));
    assert!(!log.iter().any(|line| line.starts_with("nix ")));
}

#[test]
fn start_uses_no_wrapper_when_flake_has_no_candidate_dev_shell() {
    let fixture = support::temp_workspace("nested");
    fixture.write_target_flake();
    let harness = Harness::new();

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert_eq!(
        operation_names(&log),
        ["ps", "nix", "image", "volume", "run", "inspect"]
    );
    assert!(run.contains(" opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(!run.contains("nix develop"));
}

#[test]
fn start_passes_agent_args_to_server_and_records_them() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    let endpoint_port = endpoint.port();
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint_port,
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .args(["start", "--runtime", "opencode"])
        .arg(target)
        .args(["--", "--server-flag", "value"]);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    let run = commands.first("run");
    run.assert_args_contain(" opencode serve --hostname 0.0.0.0 --port 4096 --server-flag value");
    run.assert_args_contain(&format!(
        "--label {LABEL_SERVER_ARGS}=[\"--server-flag\",\"value\"]"
    ));
}

#[test]
fn start_connect_does_not_pass_server_args_to_host_client() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    let endpoint_port = endpoint.port();
    harness.write_inspect(
        &workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint_port,
        ),
    );

    let mut command = harness.locked_agentbox_command(workspace);
    command
        .args(["start", "--connect", "--runtime", "opencode"])
        .arg(target)
        .args(["--", "--server-flag", "value"]);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    commands
        .first("run")
        .assert_args_contain(" opencode serve --hostname 0.0.0.0 --port 4096 --server-flag value");
    commands
        .entry(5)
        .assert_args_contain(&format!("attach http://127.0.0.1:{endpoint_port}"));
    commands
        .entry(5)
        .assert_args_do_not_contain("--server-flag");
}

#[test]
fn start_fails_clearly_when_flake_evaluation_fails() {
    let fixture = support::temp_workspace("nested");
    fixture.write_target_flake();
    let harness = Harness::new();
    harness.fail_nix_eval(&fixture.target, "flake exploded\n");

    let mut command = harness.locked_agentbox_command(&fixture.workspace);
    command
        .args(["start", "--runtime", "opencode"])
        .arg(&fixture.target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "failed to evaluate dev shell `path:{}`#default",
            fixture.workspace.canonical_target
        )))
        .stderr(predicate::str::contains("flake exploded"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "nix"]);
}

#[test]
fn start_launches_codex_server_in_yolo_mode() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let context_hash = default_image_context_hash();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
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
        .args([
            "start",
            "--runtime",
            "codex",
            "--cpus",
            "2",
            "--memory",
            "8g",
        ])
        .arg(target);

    let expected_endpoint = format!("ws://127.0.0.1:{}", endpoint.port());
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("INFO: managed session")
                .and(predicate::str::contains(format!(
                    "is ready at `{expected_endpoint}`"
                )))
                .and(predicate::str::contains("use `agentbox connect")),
        );
    endpoint.wait();

    let commands = harness.command_log();
    assert_eq!(
        commands.operation_names(),
        ["ps", "image", "build", "volume", "run", "inspect"]
    );

    let build = commands.first("build");
    build.assert_args_contain(&format!("-t {image} -f"));
    build.assert_args_contain("--build-arg AGENTBOX_RUNTIME=codex");
    build.assert_args_contain("--build-arg CODEX_NPM_VERSION=0.99.0");
    build.assert_args_contain("--label io.agentbox.default_runtime_image=true");
    build.assert_args_contain("--label io.agentbox.runtime=codex");
    build.assert_args_contain(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    ));
    build.assert_args_contain("--label io.agentbox.codex.package=@openai/codex");
    build.assert_args_contain("--label io.agentbox.codex.version=0.99.0");

    let run = commands.first("run");
    run.assert_args_contain("--label io.agentbox.runtime=codex");
    run.assert_args_contain("--label io.agentbox.container_kind=managed-session");
    run.assert_args_contain("--label io.agentbox.resource_limits.cpus=2");
    run.assert_args_contain("--label io.agentbox.resource_limits.memory=8g");
    run.assert_args_contain("--cpus 2");
    run.assert_args_contain("--memory 8g");
    assert_runtime_user_args(run.raw());
    run.assert_args_contain("--label io.agentbox.codex.version=0.99.0");
    run.assert_args_contain(&format!("--label io.agentbox.image={image}"));
    run.assert_args_contain("--label io.agentbox.attach_scheme=ws");
    run.assert_args_contain("--label io.agentbox.container_port=1455");
    run.assert_args_contain(&format!(
        "type=bind,src={},dst=/home/user/.codex",
        harness.home_path().join(".codex").display()
    ));
    run.assert_args_do_not_contain("--env CODEX_HOME=");
    run.assert_args_contain(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox app-server --listen ws://0.0.0.0:1455"
    ));
    run.assert_args_contain("--ws-auth capability-token");
    run.assert_args_contain("--ws-token-sha256 ");
    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/codex.json");
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"package\": \"@openai/codex\""));
    assert!(state.contains(&format!("\"image\": \"{image}\"")));
    assert!(state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    let token = fs::read_to_string(harness.codex_attach_token_path(workspace)).unwrap();
    assert!(!token.trim().is_empty());
    run.assert_args_do_not_contain(token.trim());
}

#[test]
fn start_passes_codex_home_to_codex_server_container() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();
    let codex_home = harness.home_path().join("managed-codex-home");
    fs::create_dir(&codex_home).unwrap();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Codex);
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
        .args(["start", "--runtime", "codex"])
        .arg(target);

    command.assert().success();
    endpoint.wait();

    let commands = harness.command_log();
    let run = commands.first("run");
    run.assert_args_contain(&format!(
        "type=bind,src={},dst={}",
        codex_home.display(),
        codex_home.display()
    ));
    run.assert_args_contain(&format!("--env CODEX_HOME={}", codex_home.display()));
    run.assert_args_do_not_contain("dst=/home/user/.codex");
}

#[test]
fn start_with_connect_runs_runtime_client_after_server_is_ready() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command
        .args(["start", "--connect", "--runtime", "opencode"])
        .arg(target);

    let expected_endpoint = format!("http://127.0.0.1:{}", endpoint.port());
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains(format!(
                "INFO: managed session `{}` for `{}` is ready at `{expected_endpoint}`; connecting",
                workspace.container_name, workspace.canonical_git_root,
            ))
            .and(predicate::str::contains("use `agentbox connect`").not()),
        );
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "volume", "run", "inspect", "opencode"]
    );
    assert!(log[5].contains("lock=held"));
    assert!(log[5].contains(&format!("attach {expected_endpoint}")));
    assert!(log[5].contains(&format!("cwd={}", workspace.canonical_target)));
}

fn assert_runtime_user_args(run: &str) {
    let gid = current_primary_gid().to_string();
    assert!(run.contains(&format!("--userns keep-id:uid=1000,gid={gid}")));
    assert!(run.contains(&format!("--user user:{gid}")));
    assert!(run.contains("--group-add keep-groups"));
}

fn current_primary_gid() -> libc::gid_t {
    // SAFETY: getgid has no preconditions and only returns the current process real GID.
    unsafe { libc::getgid() }
}

#[test]
fn start_skips_build_when_default_image_already_exists_locally() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command.args(["start", "--runtime", "opencode"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let operations = operation_names(&log);
    assert_eq!(
        &operations[..5],
        ["ps", "image", "volume", "run", "inspect"]
    );
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn start_builds_current_hash_image_when_only_legacy_local_image_exists() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    harness.mark_image_present("localhost/agentbox-opencode:local");
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command.args(["start", "--runtime", "opencode"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "build", "volume", "run", "inspect"]
    );
    assert!(log[1].contains(&format!("args=exists {image}")));
    assert!(log[2].contains(&format!("-t {image} -f")));
}

#[test]
fn start_verbose_traces_podman_commands_and_forwards_non_json_output() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
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
    command
        .args(["--verbose", "start", "--runtime", "opencode"])
        .arg(target);

    command.assert().success().stderr(
        predicate::str::contains("DEBUG: running podman ps")
            .and(predicate::str::contains("DEBUG: running podman build"))
            .and(predicate::str::contains("DEBUG: built"))
            .and(predicate::str::contains("DEBUG: running podman run"))
            .and(predicate::str::contains("DEBUG: started"))
            .and(predicate::str::contains(
                "DEBUG: running podman container inspect",
            ))
            .and(predicate::str::contains(
                "INFO: waiting for `opencode` runtime server",
            )),
    );
    endpoint.wait();
}

#[test]
fn start_reports_default_image_build_failures_clearly() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    harness.fail_operation("build", "podman build exploded", 125);

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["start", "--runtime", "opencode"]).arg(target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "failed to build default runtime image `{image}`"
        )))
        .stderr(predicate::str::contains("podman build exploded"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "build"]);
}

fn start_opencode_success(
    fixture: &support::TempWorkspace,
    harness: &Harness,
    extra_args: &[&str],
) -> Vec<String> {
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_inspect(
        &fixture.workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            &fixture.workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(&fixture.workspace);
    command
        .args(["start", "--runtime", "opencode"])
        .args(extra_args)
        .arg(&fixture.target);

    command.assert().success();
    endpoint.wait();

    harness.read_log()
}

fn run_opencode_success(
    fixture: &support::TempWorkspace,
    harness: &Harness,
    extra_args: &[&str],
) -> Vec<String> {
    let endpoint = ReadyEndpoint::start(RuntimeKind::Opencode);
    harness.write_inspect(
        &fixture.workspace.container_name,
        &running_workspace_inspect_fixture_with_host_port(
            &fixture.workspace,
            &RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            endpoint.port(),
        ),
    );

    let mut command = harness.locked_agentbox_command(&fixture.workspace);
    command
        .args(["run", "--runtime", "opencode"])
        .args(extra_args)
        .arg(&fixture.target);

    command.assert().success();
    endpoint.wait();

    harness.read_log()
}

fn exec_codex_success(
    fixture: &support::TempWorkspace,
    harness: &Harness,
    extra_args: &[&str],
    codex_args: &[&str],
) -> Vec<String> {
    let mut command = harness.locked_agentbox_command(&fixture.workspace);
    command
        .arg("exec")
        .args(extra_args)
        .arg(&fixture.target)
        .arg("--")
        .args(codex_args);

    command.assert().success();

    harness.read_log()
}

#[cfg(unix)]
fn bind_test_socket() -> (
    tempfile::TempDir,
    camino::Utf8PathBuf,
    std::os::unix::net::UnixListener,
) {
    let sandbox = tempfile::tempdir().unwrap();
    let socket_path =
        camino::Utf8PathBuf::from_path_buf(sandbox.path().join("agent.sock")).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(socket_path.as_std_path()).unwrap();
    (sandbox, socket_path, listener)
}

fn podman_run_command(log: &[String]) -> &str {
    log.iter()
        .find(|line| line.starts_with("run "))
        .expect("expected podman run invocation")
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

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
                "INFO: checking existing managed sessions",
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

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
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
    let run = podman_run_command(&log);

    assert!(run.contains("lock=held"));
    assert!(log[2].contains(&format!("-t {image} -f")));
    assert!(log[2].contains("--build-arg AGENTBOX_RUNTIME=opencode"));
    assert!(log[2].contains(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    )));
    assert!(run.contains("--rm"));
    assert!(run.contains("--detach"));
    assert!(!run.contains("--interactive"));
    assert!(!run.contains("--tty"));
    assert!(run.contains("--publish 127.0.0.1::4096"));
    assert!(!run.contains("--label io.agentbox.managed=true"));
    assert!(run.contains(&format!("--name {}", workspace.container_name)));
    assert!(run.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(run.contains(&format!(
        "type=volume,src={},dst=/home/user,U",
        workspace.container_name
    )));
    assert!(run.contains(&format!(
        " {image} opencode serve --hostname 0.0.0.0 --port 4096"
    )));
    assert!(log[5].contains(&format!("attach {expected_endpoint}")));
    assert!(log[5].contains(&format!("cwd={}", workspace.canonical_target)));
    assert!(log[6].contains(&format!("--ignore {}", workspace.container_name)));
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
    assert!(run.contains("--publish 127.0.0.1::1455"));
    assert!(!run.contains("--label io.agentbox.managed=true"));
    assert!(log[4].contains(&format!(
        "codex lock=held args=--dangerously-bypass-approvals-and-sandbox --remote {expected_endpoint}"
    )));
    assert!(log[4].contains(&format!("cwd={}", workspace.canonical_target)));
}

#[test]
fn exec_launches_codex_exec_foreground_without_managed_metadata() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let harness = Harness::new();

    let mut command = harness.locked_agentbox_command(workspace);
    command
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
    assert!(run.contains(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox exec --json fix-tests"
    )));
    assert!(!run.contains("app-server"));
    assert!(!log.iter().any(|line| line.starts_with("codex ")));
}

#[cfg(unix)]
#[test]
fn run_mounts_ssh_agent_socket_and_minimal_git_signing_config() {
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
    harness.write_git_config("user.name", "Alice Agent\n");
    harness.write_git_config("user.email", "alice@example.test\n");
    harness.write_git_config("gpg.format", "ssh\n");
    harness.write_git_config("user.signingkey", "ssh-ed25519 AAAATEST alice\n");
    harness.write_git_config("commit.gpgsign", "true\n");

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
    assert!(run.contains("--env SSH_AUTH_SOCK=/run/agentbox/ssh-agent.sock"));
    assert!(run.contains("--env GIT_CONFIG_COUNT=5"));
    assert!(run.contains("--env GIT_CONFIG_KEY_0=user.name"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_0=Alice Agent"));
    assert!(run.contains("--env GIT_CONFIG_KEY_1=user.email"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_1=alice@example.test"));
    assert!(run.contains("--env GIT_CONFIG_KEY_2=gpg.format"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_2=ssh"));
    assert!(run.contains("--env GIT_CONFIG_KEY_3=user.signingkey"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_3=ssh-ed25519 AAAATEST alice"));
    assert!(run.contains("--env GIT_CONFIG_KEY_4=commit.gpgsign"));
    assert!(run.contains("--env GIT_CONFIG_VALUE_4=true"));
    assert!(!run.contains("credential.helper"));
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
    assert!(!run.contains("GIT_CONFIG_COUNT"));
}

#[test]
fn run_wraps_server_command_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
    let harness = Harness::new();

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(run.contains("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096"));
}

#[test]
fn exec_wraps_codex_exec_command_with_direnv_when_envrc_applies() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
    let harness = Harness::new();

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(
        run.contains(
            "direnv exec . codex --dangerously-bypass-approvals-and-sandbox exec fix-tests"
        )
    );
    assert!(!run.contains("app-server"));
}

#[test]
fn exec_wraps_codex_exec_command_with_devenv_when_selected() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.repo.path().join("devenv.nix"), "{}\n").unwrap();
    let harness = Harness::new();

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!(
        "devenv shell --no-tui --from path:{} -- codex --dangerously-bypass-approvals-and-sandbox exec fix-tests",
        fixture.workspace.canonical_git_root
    )));
    assert!(!run.contains("app-server"));
}

#[test]
fn exec_wraps_codex_exec_command_with_nix_develop_when_selected() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.target.join("flake.nix"), "{}\n").unwrap();
    let harness = Harness::new();
    harness.mark_dev_shell(&fixture.target, "default");

    let log = exec_codex_success(&fixture, &harness, &[], &["fix-tests"]);
    let run = podman_run_command(&log);

    assert_eq!(operation_names(&log), ["ps", "nix", "image", "run"]);
    assert!(run.contains(&format!(
        "nix develop --no-write-lock-file path:{}#default --command codex --dangerously-bypass-approvals-and-sandbox exec fix-tests",
        fixture.workspace.canonical_target
    )));
}

#[test]
fn run_wraps_server_command_with_devenv_when_selected() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.repo.path().join("devenv.nix"), "{}\n").unwrap();
    let harness = Harness::new();

    let log = run_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!(
        "devenv shell --no-tui --from path:{} -- opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_git_root
    )));
}

#[test]
fn run_wraps_server_command_with_nix_develop_when_selected() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.target.join("flake.nix"), "{}\n").unwrap();
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
                    "INFO: checking existing managed sessions",
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

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "build", "volume", "run", "inspect"]
    );

    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[3].contains("lock=held"));
    assert!(log[4].contains("lock=held"));
    assert!(log[5].contains("lock=held"));

    assert!(log[2].contains(&format!("-t {image} -f")));
    assert!(log[2].contains("--build-arg AGENTBOX_RUNTIME=opencode"));
    assert!(log[2].contains("--build-arg OPENCODE_NPM_VERSION=0.99.0"));
    assert!(log[2].contains("--label io.agentbox.default_runtime_image=true"));
    assert!(log[2].contains("--label io.agentbox.runtime=opencode"));
    assert!(log[2].contains(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    )));
    assert!(log[2].contains("--label io.agentbox.opencode.package=opencode-ai"));
    assert!(log[2].contains("--label io.agentbox.opencode.version=0.99.0"));

    assert!(log[3].contains(&format!("args=exists {}", workspace.container_name)));
    assert!(log[4].contains("--rm"));
    assert!(!log[4].contains("--rmi"));
    assert!(log[4].contains("--detach"));
    assert_runtime_user_args(&log[4]);
    assert!(!log[4].contains("--interactive"));
    assert!(!log[4].contains("--tty"));
    assert!(log[4].contains(&format!("--label io.agentbox.image={image}")));
    assert!(log[4].contains("--label io.agentbox.opencode.version=0.99.0"));
    assert!(log[4].contains("--label io.agentbox.attach_scheme=http"));
    assert!(log[4].contains("--label io.agentbox.container_port=4096"));
    assert!(log[4].contains(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    )));
    assert!(log[4].contains(&format!("--name {}", workspace.container_name)));
    assert!(log[4].contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(log[4].contains(&image));
    assert!(log[4].contains(" opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(log[4].contains("--publish 127.0.0.1::4096"));
    assert!(log[4].contains("--env OPENCODE_CONFIG_CONTENT={\"autoupdate\":false}"));
    assert!(log[4].contains("--env OPENCODE_PERMISSION={\"*\":\"allow\"}"));
    assert!(log[4].contains(&format!(
        "type=bind,src={},dst=/home/user/.config/opencode",
        harness.home_path().join(".config/opencode").display()
    )));
    assert!(log[4].contains(&format!(
        "type=bind,src={},dst=/home/user/.local/share/opencode",
        harness.home_path().join(".local/share/opencode").display()
    )));
    assert!(log[4].contains("type=volume"));
    assert!(log[4].contains("dst=/home/user,U"));
    assert!(!log[4].contains("direnv exec ."));
    assert!(!log[4].contains("sleep infinity"));
    assert!(!log.iter().any(|line| line.starts_with("stop ")));
    assert!(
        !log.iter()
            .any(|line| line.starts_with("volume ") && line.contains("args=rm "))
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
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
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
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
    fs::write(fixture.repo.path().join("devenv.nix"), "{}\n").unwrap();
    fs::write(fixture.repo.path().join("flake.nix"), "{}\n").unwrap();
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
    fs::write(fixture.repo.path().join("devenv.nix"), "{}\n").unwrap();
    let harness = Harness::new();

    let log = start_opencode_success(&fixture, &harness, &[]);
    let run = podman_run_command(&log);

    assert!(run.contains(&format!("--workdir {}", fixture.workspace.canonical_target)));
    assert!(run.contains(&format!(
        "devenv shell --no-tui --from path:{} -- opencode serve --hostname 0.0.0.0 --port 4096",
        fixture.workspace.canonical_git_root
    )));
    assert!(!log.iter().any(|line| line.starts_with("nix ")));
}

#[test]
fn start_uses_target_flake_default_dev_shell() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.target.join("flake.nix"), "{}\n").unwrap();
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
    fs::write(fixture.repo.path().join("flake.nix"), "{}\n").unwrap();
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
    fs::write(fixture.repo.path().join("flake.nix"), "{}\n").unwrap();
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
    fs::write(fixture.repo.path().join(".envrc"), "use nix\n").unwrap();
    fs::write(fixture.repo.path().join("devenv.nix"), "{}\n").unwrap();
    fs::write(fixture.repo.path().join("flake.nix"), "{}\n").unwrap();
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
    fs::write(fixture.target.join("flake.nix"), "{}\n").unwrap();
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
fn start_fails_clearly_when_flake_evaluation_fails() {
    let fixture = support::temp_workspace("nested");
    fs::write(fixture.target.join("flake.nix"), "{}\n").unwrap();
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
    command.args(["start", "--runtime", "codex"]).arg(target);

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

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "build", "volume", "run", "inspect"]
    );

    let run = log.iter().find(|line| line.starts_with("run ")).unwrap();
    assert!(log[2].contains(&format!("-t {image} -f")));
    assert!(log[2].contains("--build-arg AGENTBOX_RUNTIME=codex"));
    assert!(log[2].contains("--build-arg CODEX_NPM_VERSION=0.99.0"));
    assert!(log[2].contains("--label io.agentbox.default_runtime_image=true"));
    assert!(log[2].contains("--label io.agentbox.runtime=codex"));
    assert!(log[2].contains(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    )));
    assert!(log[2].contains("--label io.agentbox.codex.package=@openai/codex"));
    assert!(log[2].contains("--label io.agentbox.codex.version=0.99.0"));
    assert!(run.contains("--label io.agentbox.runtime=codex"));
    assert_runtime_user_args(run);
    assert!(run.contains("--label io.agentbox.codex.version=0.99.0"));
    assert!(run.contains(&format!("--label io.agentbox.image={image}")));
    assert!(run.contains("--label io.agentbox.attach_scheme=ws"));
    assert!(run.contains("--label io.agentbox.container_port=1455"));
    assert!(run.contains(&format!(
        "type=bind,src={},dst=/home/user/.codex",
        harness.home_path().join(".codex").display()
    )));
    assert!(run.contains(&format!(
        " {image} codex --dangerously-bypass-approvals-and-sandbox app-server --listen ws://0.0.0.0:1455"
    )));
    let state_path = harness
        .state_home_path()
        .join("agentbox/runtime/codex.json");
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"package\": \"@openai/codex\""));
    assert!(state.contains(&format!("\"image\": \"{image}\"")));
    assert!(state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
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
            .and(predicate::str::contains("DEBUG: running podman inspect"))
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

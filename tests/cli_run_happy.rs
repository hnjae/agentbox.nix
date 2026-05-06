// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use agentbox::runtime::{RuntimeKind, default_image::OPENCODE_LEGACY_DEFAULT_IMAGE};
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, ReadyEndpoint, operation_names,
    running_workspace_inspect_fixture_with_host_port,
};

#[test]
fn run_creates_starts_serves_waits_and_attaches_for_a_new_session() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let context_hash = RuntimeKind::Opencode.default_image_context_hash();
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
        ["ps", "image", "build", "run", "inspect"]
    );

    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[3].contains("lock=held"));
    assert!(log[4].contains("lock=held"));

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

    assert!(log[3].contains("--rm"));
    assert!(!log[3].contains("--rmi"));
    assert!(log[3].contains("--detach"));
    assert_runtime_user_args(&log[3]);
    assert!(!log[3].contains("--interactive"));
    assert!(!log[3].contains("--tty"));
    assert!(log[3].contains(&format!("--label io.agentbox.image={image}")));
    assert!(log[3].contains("--label io.agentbox.opencode.version=0.99.0"));
    assert!(log[3].contains("--label io.agentbox.attach_scheme=http"));
    assert!(log[3].contains("--label io.agentbox.container_port=4096"));
    assert!(log[3].contains(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    )));
    assert!(log[3].contains(&format!("--name {}", workspace.container_name)));
    assert!(log[3].contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(log[3].contains(&image));
    assert!(log[3].contains(" opencode serve --hostname 0.0.0.0 --port 4096"));
    assert!(log[3].contains("--publish 127.0.0.1::4096"));
    assert!(log[3].contains("--env OPENCODE_CONFIG_CONTENT={\"autoupdate\":false}"));
    assert!(log[3].contains(&format!(
        "type=bind,src={},dst=/home/user/.config/opencode",
        harness.home_path().join(".config/opencode").display()
    )));
    assert!(log[3].contains(&format!(
        "type=bind,src={},dst=/home/user/.local/share/opencode",
        harness.home_path().join(".local/share/opencode").display()
    )));
    assert!(log[3].contains("type=volume"));
    assert!(log[3].contains("dst=/home/user/.cache/nix,U"));
    assert!(!log[3].contains("direnv exec ."));
    assert!(!log[3].contains("sleep infinity"));

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
fn run_wraps_server_command_with_direnv_when_envrc_applies() {
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
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let run = log.iter().find(|line| line.starts_with("run ")).unwrap();

    assert!(run.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(run.contains("direnv exec . opencode serve --hostname 0.0.0.0 --port 4096"));
}

#[test]
fn run_launches_codex_server_in_yolo_mode() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Codex.default_image();
    let context_hash = RuntimeKind::Codex.default_image_context_hash();
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
    command.args(["run", "--runtime", "codex"]).arg(target);

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
                .and(predicate::str::contains("use `agentbox attach")),
        );
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "build", "run", "inspect"]
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
fn run_skips_build_when_default_image_already_exists_locally() {
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
    command.args(["run", "--runtime", "opencode"]).arg(target);

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    let operations = operation_names(&log);
    assert_eq!(&operations[..4], ["ps", "image", "run", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn run_builds_current_hash_image_when_only_legacy_local_image_exists() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    harness.mark_image_present(OPENCODE_LEGACY_DEFAULT_IMAGE);
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

    command.assert().success();
    endpoint.wait();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image", "build", "run", "inspect"]
    );
    assert!(log[1].contains(&format!("args=exists {image}")));
    assert!(log[2].contains(&format!("-t {image} -f")));
}

#[test]
fn run_verbose_traces_podman_commands_and_forwards_non_json_output() {
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
        .args(["--verbose", "run", "--runtime", "opencode"])
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
fn run_reports_default_image_build_failures_clearly() {
    let fixture = support::temp_workspace("nested");
    let target = fixture.target.as_path();
    let workspace = &fixture.workspace;
    let image = RuntimeKind::Opencode.default_image();
    let harness = Harness::new();
    harness.mark_default_image_absent();
    harness.fail_operation("build", "podman build exploded", 125);

    let mut command = harness.locked_agentbox_command(workspace);
    command.args(["run", "--runtime", "opencode"]).arg(target);

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

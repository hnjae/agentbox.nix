// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use assert_cmd::Command as AssertCommand;

use agentbox::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::session::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use agentbox::workspace::resolve_workspace_identity;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as LiveHarness, managed_inspect_fixture, managed_labels, managed_ps_entry,
    path_with_prepend, ps_fixture, write_executable,
};

#[test]
fn helper_returns_live_roots_with_runtime_and_status_metadata() {
    let repo = support::temp_git_repo();
    let canonical = repo.path().canonicalize().unwrap();
    let harness = install_harness(repo.path());

    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .arg("__completion-roots")
        .arg("attach")
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();

    assert!(output.contains(canonical.to_str().unwrap()));
    assert!(output.contains("opencode"));
}

#[test]
fn helper_filters_attach_and_stop_candidates_by_command() {
    let running_repo = support::temp_git_repo();
    let failed_repo = support::temp_git_repo();
    let running_workspace = resolve_workspace_identity(running_repo.path()).unwrap();
    let failed_workspace = resolve_workspace_identity(failed_repo.path()).unwrap();
    let harness = LiveHarness::new();

    harness.write_ps(&ps_fixture(vec![
        managed_ps_entry(
            "running-id",
            &running_workspace.container_name,
            &running_workspace.hash12,
        ),
        managed_ps_entry(
            "failed-id",
            &failed_workspace.container_name,
            &failed_workspace.hash12,
        ),
    ]));
    harness.write_inspect(
        "running-id",
        &managed_inspect_fixture(
            &running_workspace.container_name,
            running_workspace.canonical_git_root.as_str(),
            true,
            true,
            managed_labels(
                running_workspace.canonical_git_root.as_str(),
                &running_workspace.hash12,
                "opencode",
                &running_workspace.container_name,
            ),
        ),
    );
    let mut failed_labels = managed_labels(
        failed_workspace.canonical_git_root.as_str(),
        &failed_workspace.hash12,
        "opencode",
        &failed_workspace.container_name,
    );
    failed_labels.remove(LABEL_ATTACH_SCHEME);
    harness.write_inspect(
        "failed-id",
        &managed_inspect_fixture(
            &failed_workspace.container_name,
            failed_workspace.canonical_git_root.as_str(),
            true,
            true,
            failed_labels,
        ),
    );

    let attach = harness
        .agentbox_command()
        .args(["__completion-roots", "attach"])
        .output()
        .unwrap();
    assert!(attach.status.success());
    let attach = String::from_utf8(attach.stdout).unwrap();
    assert!(attach.contains(running_workspace.canonical_git_root.as_str()));
    assert!(!attach.contains(failed_workspace.canonical_git_root.as_str()));

    let stop = harness
        .agentbox_command()
        .args(["__completion-roots", "stop"])
        .output()
        .unwrap();
    assert!(stop.status.success());
    let stop = String::from_utf8(stop.stdout).unwrap();
    assert!(stop.contains(running_workspace.canonical_git_root.as_str()));
    assert!(stop.contains(failed_workspace.canonical_git_root.as_str()));
    assert!(stop.contains("failed"));
}

#[test]
fn zsh_completion_script_wires_the_dynamic_callback_and_descriptions() {
    let script = capture_completion_script();

    assert!(script.contains("__completion-roots"));
    assert!(script.contains("compdef _agentbox agentbox"));
    assert!(script.contains("compadd -d descriptions -- \"${candidates[@]}\""));
    assert!(script.contains("runtime status"));
}

#[test]
fn fish_completion_script_keeps_helper_metadata_available() {
    let script = capture_completion_script_shell("fish");

    assert!(script.contains("agentbox __completion-roots $command 2>/dev/null"));
    assert!(script.contains("__fish_seen_subcommand_from attach"));
    assert!(script.contains("__fish_seen_subcommand_from stop"));
    assert!(script.contains("(__agentbox_completion_roots attach)"));
    assert!(script.contains("(__agentbox_completion_roots stop)"));
}

#[test]
fn installed_completion_script_uses_live_roots_for_directory_commands() {
    let script = capture_installed_completion_script("bash");

    assert!(script.contains("_agentbox()"));
    assert!(script.contains("run runtime attach ls stop completion help"));
    assert!(script.contains("__completion-roots"));
    assert!(script.contains("complete -F _agentbox agentbox"));
    assert!(!script.contains("__generate-completion"));
    assert!(!script.contains("__generate-man"));
    assert!(!script.contains("__generate-manpages"));
}

#[test]
fn installed_manpage_uses_clap_model_without_internal_helpers() {
    let manpage = capture_installed_manpage();

    assert!(manpage.contains(".TH agentbox 1"));
    assert!(manpage.contains("agentbox\\-run(1)"));
    assert!(!manpage.contains("agentbox\\-help(1)"));
    assert!(manpage.contains("Shell completion helpers"));
    assert!(!manpage.contains("__completion-roots"));
    assert!(!manpage.contains("__generate-completion"));
    assert!(!manpage.contains("__generate-man"));
    assert!(!manpage.contains("__generate-manpages"));
}

#[test]
fn installed_manpages_include_referenced_subcommands() {
    let directory = tempfile::tempdir().unwrap();
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-manpages")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for filename in [
        "agentbox.1",
        "agentbox-run.1",
        "agentbox-runtime.1",
        "agentbox-attach.1",
        "agentbox-ls.1",
        "agentbox-stop.1",
        "agentbox-completion.1",
    ] {
        assert!(
            directory.path().join(filename).is_file(),
            "missing generated manpage {filename}"
        );
    }
    assert!(!directory.path().join("agentbox-help.1").exists());

    let agentbox = fs::read_to_string(directory.path().join("agentbox.1")).unwrap();
    assert!(agentbox.contains("agentbox\\-run(1)"));
    assert!(!agentbox.contains("agentbox\\-help(1)"));

    let run = fs::read_to_string(directory.path().join("agentbox-run.1")).unwrap();
    assert!(run.contains(".TH agentbox-run 1"));
    assert!(run.contains("Runtime to launch for this run"));
}

fn capture_completion_script() -> String {
    capture_completion_script_shell("zsh")
}

fn capture_completion_script_shell(shell: &str) -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("completion")
        .arg(shell)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn capture_installed_completion_script(shell: &str) -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-completion")
        .arg(shell)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn capture_installed_manpage() -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-man")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn install_harness(repo_root: &std::path::Path) -> Harness {
    let fake_bin = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let state_home = tempfile::tempdir().unwrap();
    let original_path = std::env::var("PATH").unwrap();

    let root = repo_root.canonicalize().unwrap();
    let hash = agentbox::workspace::hash12(root.to_str().unwrap().as_bytes());
    let ps_json = format!(
        r#"[
  {{
    "Id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "Image": "ghcr.io/example/agentbox:latest",
    "Command": ["opencode"],
    "Created": 1713681300,
    "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
    "Names": ["agentbox-demo"],
    "Ports": [],
    "Status": "Up 2 minutes",
    "State": "running",
    "Labels": {{
      "{LABEL_MANAGED}": "{LABEL_MANAGED_VALUE}",
      "{LABEL_GIT_ROOT_HASH}": "{hash}"
    }},
    "Mounts": [],
    "Networks": ["podman"],
    "Namespaces": null
  }}
]"#
    );
    let inspect_json = format!(
        r#"[
  {{
    "Id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "Created": "2026-04-21T10:15:00.000000000Z",
    "Path": "/usr/bin/opencode",
    "Args": [],
    "State": {{
      "Status": "running",
      "Running": true,
      "ExitCode": 0,
      "Pid": 4321,
      "StartedAt": "2026-04-21T10:15:01.000000000Z",
      "FinishedAt": "0001-01-01T00:00:00Z"
    }},
    "ImageName": "ghcr.io/example/agentbox:latest",
    "Config": {{
      "User": "agent",
      "Env": [],
      "Cmd": ["opencode"],
      "WorkingDir": "/workspace",
      "Labels": {{
        "{LABEL_MANAGED}": "{LABEL_MANAGED_VALUE}",
        "{LABEL_SCHEMA}": "{LABEL_SCHEMA_VALUE}",
        "{LABEL_GIT_ROOT}": "{}",
        "{LABEL_GIT_ROOT_HASH}": "{hash}",
        "{LABEL_RUNTIME}": "opencode",
        "{LABEL_IMAGE}": "ghcr.io/example/agentbox:latest",
        "{LABEL_LAUNCH_DIRECTORY}": "{}",
        "{LABEL_LOGICAL_NAME}": "agentbox-demo",
        "{LABEL_ATTACH_SCHEME}": "http",
        "{LABEL_CONTAINER_PORT}": "4096",
        "{LABEL_CONTAINER_LISTEN_IP}": "0.0.0.0"
      }},
      "Entrypoint": ["/entrypoint"],
      "StopSignal": "SIGTERM"
    }},
    "HostConfig": {{
      "AutoRemove": false,
      "NetworkMode": "bridge",
      "Privileged": false
    }},
    "Mounts": [
      {{
        "Type": "bind",
        "Source": "/tmp/workspace",
        "Destination": "{REQUIRED_NIX_CACHE_MOUNT_DESTINATION}",
        "RW": true
      }}
    ],
    "NetworkSettings": {{
      "Networks": {{}},
      "Ports": {{
        "4096/tcp": [
          {{
            "HostIp": "127.0.0.1",
            "HostPort": "49152"
          }}
        ]
      }}
    }}
  }}
]"#,
        root.to_str().unwrap(),
        root.to_str().unwrap()
    );
    fs::write(fixtures.path().join("ps.json"), ps_json).unwrap();
    fs::write(
        fixtures
            .path()
            .join("inspect-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.json"),
        inspect_json,
    )
    .unwrap();
    write_executable(fake_bin.path().join("podman"), &fake_podman_script());

    Harness {
        fake_bin,
        fixtures,
        state_home,
        original_path,
    }
}

struct Harness {
    fake_bin: tempfile::TempDir,
    fixtures: tempfile::TempDir,
    state_home: tempfile::TempDir,
    original_path: String,
}

impl Harness {
    fn path_env(&self) -> String {
        path_with_prepend(self.fake_bin.path(), &self.original_path)
    }
}

fn fake_podman_script() -> String {
    r#"#!/bin/sh
set -eu
case "$1" in
  ps)
    cat "$AGENTBOX_TEST_FIXTURES/ps.json"
    ;;
  inspect)
    cat "$AGENTBOX_TEST_FIXTURES/inspect-$2.json"
    ;;
  *)
    exit 1
    ;;
esac
"#
    .to_string()
}

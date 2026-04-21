// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command as AssertCommand;

use agentbox::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};

#[path = "support/mod.rs"]
mod support;

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
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();

    assert!(output.contains(canonical.to_str().unwrap()));
    assert!(output.contains("opencode"));
    assert!(output.contains("running"));
}

#[test]
fn zsh_completion_script_wires_the_dynamic_callback() {
    let script = capture_completion_script();

    assert!(script.contains("__completion-roots"));
    assert!(script.contains("compdef _agentbox_completion_roots agentbox"));
}

fn capture_completion_script() -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("completion")
        .arg("zsh")
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
    "Command": ["sleep", "infinity"],
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
    "Path": "/usr/bin/sleep",
    "Args": ["infinity"],
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
      "Cmd": ["infinity"],
      "WorkingDir": "/workspace",
      "Labels": {{
        "{LABEL_MANAGED}": "{LABEL_MANAGED_VALUE}",
        "{LABEL_SCHEMA}": "{LABEL_SCHEMA_VALUE}",
        "{LABEL_GIT_ROOT}": "{}",
        "{LABEL_GIT_ROOT_HASH}": "{hash}",
        "{LABEL_RUNTIME}": "opencode",
        "{LABEL_IMAGE}": "ghcr.io/example/agentbox:latest",
        "{LABEL_LOGICAL_NAME}": "agentbox-demo"
      }},
      "Entrypoint": ["/usr/bin/sleep"],
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
    "NetworkSettings": {{"Networks": {{}}}}
  }}
]"#,
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
        format!("{}:{}", self.fake_bin.path().display(), self.original_path)
    }
}

fn write_executable(path: PathBuf, contents: &str) {
    fs::write(&path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
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

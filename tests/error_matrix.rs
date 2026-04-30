// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use agentbox::preflight::{PreflightSnapshot, check_host_prerequisites_with_snapshot};
use agentbox::runtime::opencode::DEFAULT_IMAGE;
use agentbox::session::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};
use agentbox::workspace::resolve_workspace_identity;
use assert_cmd::Command as AssertCommand;
use camino::Utf8Path;
use serde_json::{Value, json};

#[path = "support/mod.rs"]
mod support;

#[test]
fn required_error_cases_are_actionable() {
    workspace_identity_errors_are_actionable();
    host_preflight_errors_are_actionable();
    attach_side_drift_errors_are_actionable();
    runtime_command_failures_are_actionable();
}

fn workspace_identity_errors_are_actionable() {
    let non_git = tempfile::tempdir().unwrap();
    let error = resolve_workspace_identity(non_git.path()).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains(non_git.path().to_str().unwrap()),
        "{message}"
    );
    assert!(message.contains("git repository"), "{message}");
    assert!(message.contains("git init"), "{message}");

    let escaped_target = Utf8Path::new("/workspace/demo/escape");
    let git_root = Utf8Path::new("/workspace/demo");
    let message = agentbox::Error::escaped_git_target(escaped_target, git_root).to_string();
    assert!(
        message.contains("resolves outside the git root"),
        "{message}"
    );
    assert!(message.contains(escaped_target.as_str()), "{message}");
    assert!(message.contains(git_root.as_str()), "{message}");
}

fn host_preflight_errors_are_actionable() {
    let target = Utf8Path::new("/workspace/demo/nested");

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_git = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`git` was not found on PATH; install `git` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_podman = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`podman` was not found on PATH; install `podman` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| {
            snapshot.direnv_required = true;
            snapshot.has_direnv = false;
        }),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("`.envrc` applies to `/workspace/demo/nested`")
    );
    assert!(
        error
            .to_string()
            .contains("install `direnv` or add it to PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_nix_daemon_socket = false),
        Some(target),
    )
    .unwrap_err();
    assert!(error.to_string().contains(
        "Missing host nix-daemon socket at: /nix/var/nix/daemon-socket/socket. Mount /nix:/nix:ro."
    ));

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.nix_client_source = None),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Expected host-mounted nix not found in PATH")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_etc_nix_mount = false),
        Some(target),
    )
    .unwrap_err();
    assert!(error.to_string().contains("Missing /etc/nix host mount"));

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| snapshot.has_readable_nix_conf = false),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Missing readable host Nix config: /etc/nix/nix.conf")
    );

    let error = check_host_prerequisites_with_snapshot(
        &snapshot_with(|snapshot| {
            snapshot.nix_custom_conf_present = true;
            snapshot.has_readable_nix_custom_conf_target = false;
        }),
        Some(target),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Missing readable target for /etc/nix/nix.custom.conf")
    );
}

fn attach_side_drift_errors_are_actionable() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();

    let missing_labels = Harness::new();
    missing_labels.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-labels-id",
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
    missing_labels.write_inspect(
        "failed-labels-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            true,
            labels,
        ),
    );

    missing_labels
        .attach_assert(&target)
        .failure()
        .stderr(predicates::str::contains("missing required session labels"))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));

    let missing_cache = Harness::new();
    missing_cache.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-cache-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    missing_cache.write_inspect(
        "failed-cache-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    missing_cache
        .attach_assert(&target)
        .failure()
        .stderr(predicates::str::contains("missing required cache mount"))
        .stderr(predicates::str::contains(
            REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
        ))
        .stderr(predicates::str::contains(
            "recreate the container before retrying",
        ));
}

fn runtime_command_failures_are_actionable() {
    let run_failure = RunFailureCase {
        target_subdir: "run-failure",
        failure: FailureSpec::new("run", "container failed to start", 125),
        expected: vec![
            "failed to run the runtime server command",
            "container failed to start",
            "Verify the runtime image still provides `/entrypoint`",
        ],
    };
    assert_run_failure_case(run_failure);

    let missing_ca_bundle = RunFailureCase {
        target_subdir: "missing-ca-bundle",
        failure: FailureSpec::new(
            "run",
            "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt.",
            126,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Missing image-local CA bundle at /etc/ssl/certs/ca-certificates.crt.",
            "Verify the runtime image still provides `/entrypoint`",
        ],
    };
    assert_run_failure_case(missing_ca_bundle);

    let unusable_state_path = RunFailureCase {
        target_subdir: "unusable-state-path",
        failure: FailureSpec::new(
            "run",
            "Unusable Nix profile state path: /proc/agentbox-state/nix/profile. Ensure XDG_STATE_HOME or HOME points to a writable location.",
            125,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Unusable Nix profile state path: /proc/agentbox-state/nix/profile",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(unusable_state_path);

    let missing_foreground_command = RunFailureCase {
        target_subdir: "missing-foreground-command",
        failure: FailureSpec::new("run", "opencode: not found", 127),
        expected: vec![
            "failed to run the runtime server command",
            "opencode: not found",
            "Verify the runtime image still provides `/entrypoint` and the expected runtime tools",
        ],
    };
    assert_run_failure_case(missing_foreground_command);

    let entrypoint_failure = RunFailureCase {
        target_subdir: "entrypoint-failure",
        failure: FailureSpec::new("run", "/entrypoint: Permission denied", 126),
        expected: vec![
            "failed to run the runtime server command",
            "/entrypoint: Permission denied",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(entrypoint_failure);

    let permission_denied = RunFailureCase {
        target_subdir: "workspace-permission-denied",
        failure: FailureSpec::new(
            "run",
            "Permission denied: /tmp/agentbox-denied/workspace-permission-denied",
            13,
        ),
        expected: vec![
            "failed to run the runtime server command",
            "Permission denied: /tmp/agentbox-denied/workspace-permission-denied",
            "retry or recreate the session",
        ],
    };
    assert_run_failure_case(permission_denied);
}

fn assert_run_failure_case(case: RunFailureCase<'_>) {
    let repo = support::temp_git_repo();
    let target = repo.path().join(case.target_subdir);
    fs::create_dir(&target).unwrap();

    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.write_failure(
        case.failure.kind,
        case.failure.stderr,
        case.failure.exit_code,
    );

    let assert = harness.run_assert(&target);
    let mut assert = assert.failure();
    for expected in case.expected {
        assert = assert.stderr(predicates::str::contains(expected));
    }
}

fn snapshot_with(configure: impl FnOnce(&mut PreflightSnapshot)) -> PreflightSnapshot {
    let mut snapshot = PreflightSnapshot {
        has_git: true,
        has_podman: true,
        direnv_required: false,
        has_direnv: true,
        has_nix_daemon_socket: true,
        nix_client_source: Some("/run/current-system/sw/bin/nix".into()),
        has_etc_nix_mount: true,
        has_readable_nix_conf: true,
        nix_custom_conf_present: false,
        has_readable_nix_custom_conf_target: true,
        needs_static_nix_mount: false,
    };
    configure(&mut snapshot);
    snapshot
}

struct RunFailureCase<'a> {
    target_subdir: &'a str,
    failure: FailureSpec<'a>,
    expected: Vec<&'a str>,
}

struct FailureSpec<'a> {
    kind: &'a str,
    stderr: &'a str,
    exit_code: i32,
}

impl<'a> FailureSpec<'a> {
    fn new(kind: &'a str, stderr: &'a str, exit_code: i32) -> Self {
        Self {
            kind,
            stderr,
            exit_code,
        }
    }
}

struct Harness {
    fake_bin: tempfile::TempDir,
    fixtures: tempfile::TempDir,
    state_home: tempfile::TempDir,
    original_path: String,
}

impl Harness {
    fn new() -> Self {
        let fake_bin = tempfile::tempdir().unwrap();
        let fixtures = tempfile::tempdir().unwrap();
        let state_home = tempfile::tempdir().unwrap();
        let original_path = std::env::var("PATH").unwrap();

        fs::write(fixtures.path().join("image.exists"), "present\n").unwrap();
        fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
        write_executable(fake_bin.path().join("podman"), &fake_podman_script());

        Self {
            fake_bin,
            fixtures,
            state_home,
            original_path,
        }
    }

    fn path_env(&self) -> String {
        format!("{}:{}", self.fake_bin.path().display(), self.original_path)
    }

    fn write_ps(&self, json: &str) {
        fs::write(self.fixtures.path().join("ps.json"), json).unwrap();
    }

    fn write_inspect(&self, name: &str, json: &str) {
        fs::write(
            self.fixtures.path().join(format!("inspect-{name}.json")),
            json,
        )
        .unwrap();
    }

    fn write_failure(&self, kind: &str, stderr: &str, exit_code: i32) {
        fs::write(self.fixtures.path().join(format!("{kind}.stderr")), stderr).unwrap();
        fs::write(
            self.fixtures.path().join(format!("{kind}.exit")),
            format!("{exit_code}\n"),
        )
        .unwrap();
    }

    fn run_assert(&self, target: &Path) -> assert_cmd::assert::Assert {
        let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
        command
            .env("PATH", self.path_env())
            .env("XDG_STATE_HOME", self.state_home.path())
            .env("AGENTBOX_TEST_FIXTURES", self.fixtures.path())
            .args(["run", "--runtime", "opencode"])
            .arg(target);
        command.assert()
    }

    fn attach_assert(&self, target: &Path) -> assert_cmd::assert::Assert {
        let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
        command
            .env("PATH", self.path_env())
            .env("XDG_STATE_HOME", self.state_home.path())
            .env("AGENTBOX_TEST_FIXTURES", self.fixtures.path())
            .arg("attach")
            .arg(target);
        command.assert()
    }
}

fn managed_ps_entry(id: &str, name: &str, git_root_hash: &str) -> Value {
    json!({
        "Id": id,
        "Image": DEFAULT_IMAGE,
        "Command": ["opencode"],
        "Created": 1713681300,
        "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
        "Names": [name],
        "Ports": [],
        "Status": "Up 2 minutes",
        "State": "running",
        "Labels": {
            LABEL_MANAGED: LABEL_MANAGED_VALUE,
            LABEL_GIT_ROOT_HASH: git_root_hash,
        },
        "Mounts": [],
        "Networks": ["podman"],
        "Namespaces": null,
    })
}

fn ps_fixture(entries: Vec<Value>) -> String {
    serde_json::to_string(&entries).unwrap()
}

fn managed_labels(
    git_root: &str,
    git_root_hash: &str,
    runtime: &str,
    logical_name: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (LABEL_GIT_ROOT.to_string(), git_root.to_string()),
        (LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string()),
        (LABEL_RUNTIME.to_string(), runtime.to_string()),
        (LABEL_IMAGE.to_string(), DEFAULT_IMAGE.to_string()),
        (LABEL_LOGICAL_NAME.to_string(), logical_name.to_string()),
        (LABEL_ATTACH_SCHEME.to_string(), "http".to_string()),
        (LABEL_CONTAINER_PORT.to_string(), "4096".to_string()),
        (LABEL_CONTAINER_LISTEN_IP.to_string(), "0.0.0.0".to_string()),
    ])
}

fn managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
) -> String {
    let mut mounts = vec![json!({
        "Type": "bind",
        "Source": git_root,
        "Destination": git_root,
        "RW": true,
    })];
    if include_cache_mount {
        mounts.push(json!({
            "Type": "volume",
            "Source": container_name,
            "Destination": REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
            "RW": true,
        }));
    }

    serde_json::to_string(&vec![json!({
        "Id": container_name,
        "Created": "2026-04-21T10:15:00.000000000Z",
        "Path": "/usr/bin/opencode",
        "Args": [],
        "State": {
            "Status": if running { "running" } else { "exited" },
            "Running": running,
            "ExitCode": if running { 0 } else { 137 },
            "Pid": if running { 4321 } else { 0 },
            "StartedAt": "2026-04-21T10:15:01.000000000Z",
            "FinishedAt": null,
            "Health": null,
        },
        "ImageName": DEFAULT_IMAGE,
        "Config": {
            "User": "user",
            "Env": [],
            "Cmd": ["opencode"],
            "WorkingDir": git_root,
            "Labels": labels,
            "Entrypoint": ["/entrypoint"],
            "StopSignal": "SIGTERM",
        },
        "HostConfig": {
            "AutoRemove": false,
            "NetworkMode": "bridge",
            "Privileged": false,
        },
        "Mounts": mounts,
        "NetworkSettings": {
            "Networks": {},
            "Ports": {
                "4096/tcp": [
                    {
                        "HostIp": "127.0.0.1",
                        "HostPort": "49152"
                    }
                ]
            },
        },
    })])
    .unwrap()
}

fn write_executable(path: PathBuf, content: &str) {
    fs::write(&path, content).unwrap();

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

fixtures=${AGENTBOX_TEST_FIXTURES:?missing AGENTBOX_TEST_FIXTURES}

maybe_fail() {
  prefix=$1
  if [ -f "$fixtures/$prefix.exit" ]; then
    if [ -f "$fixtures/$prefix.stderr" ]; then
      cat "$fixtures/$prefix.stderr" >&2
    fi
    exit "$(tr -d '\n' < "$fixtures/$prefix.exit")"
  fi
}

cmd=$1
shift || true

case "$cmd" in
  ps)
    cat "$fixtures/ps.json"
    ;;
  image)
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
      exists)
        if [ -f "$fixtures/image.exists" ]; then
          exit 0
        fi
        exit 1
        ;;
      *)
        printf 'unexpected podman image invocation: %s %s\n' "$subcommand" "$*" >&2
        exit 97
        ;;
    esac
    ;;
  build)
    printf 'built\n'
    ;;
  inspect)
    target=${1:?missing inspect target}
    cat "$fixtures/inspect-$target.json"
    ;;
  create)
    maybe_fail create
    printf 'created\n'
    ;;
  start)
    maybe_fail start
    printf 'started\n'
    ;;
  run)
    maybe_fail run
    printf 'ok\n'
    ;;
  exec)
    mode=exec-attach
    case "$*" in
      --detach*)
        mode=exec-detach
        ;;
      *'/entrypoint curl '*)
        mode=exec-ready
        ;;
    esac
    maybe_fail "$mode"
    printf 'ok\n'
    ;;
  *)
    printf 'unexpected podman invocation: %s %s\n' "$cmd" "$*" >&2
    exit 97
    ;;
esac
"#
    .to_string()
}

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

use agentbox::runtime::opencode::DEFAULT_IMAGE;
use agentbox::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};
use agentbox::workspace::{hash12, resolve_workspace_identity};
use assert_cmd::Command as AssertCommand;
use serde_json::{Value, json};

#[path = "support/mod.rs"]
mod support;

#[test]
fn stop_removes_the_container_and_leaves_the_volume_and_workspace_untouched() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "session-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "session-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[]).success();

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "stop", "rm"]);
    assert!(log[2].contains("--ignore"));
    assert!(log[3].contains("--ignore"));
    assert!(target.exists(), "stop must not delete the user workspace");
    assert!(
        !log.iter().any(|line| line.starts_with("volume ")),
        "stop must not delete the matching cache volume"
    );
}

#[test]
fn stop_is_idempotent_when_the_container_disappears_during_cleanup() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "session-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "session-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                &workspace.container_name,
            ),
        ),
    );
    harness.mark_missing_during_cleanup();

    run_command(&harness, &target, &[]).success();

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "stop", "rm"]);
    assert!(log[2].contains("--ignore"));
    assert!(log[3].contains("--ignore"));
}

#[test]
fn stop_force_removes_all_exact_duplicate_root_matches() {
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

    run_command(&harness, &target, &["--force"]).success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "inspect", "inspect", "stop", "rm", "stop", "rm"]
    );
}

#[test]
fn stop_duplicate_root_requires_force_before_cleanup() {
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

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist",
        ))
        .stderr(predicates::str::contains("agentbox stop --force"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn stop_allows_exact_missing_path_match_for_orphaned_root_identity() {
    let repo = support::temp_git_repo();
    let root = repo.path().canonicalize().unwrap();
    let root_string = root.to_str().unwrap().to_string();
    let hash = hash12(root_string.as_bytes());
    let container_name = "orphaned-session";
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "orphaned-id",
        container_name,
        &hash,
    )]));
    harness.write_inspect(
        "orphaned-id",
        &managed_inspect_fixture(
            container_name,
            &root_string,
            true,
            managed_labels(&root_string, &hash, container_name),
        ),
    );
    drop(repo);

    run_command(&harness, Path::new(&root_string), &[]).success();

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "stop", "rm"]);
}

struct Harness {
    fake_bin: tempfile::TempDir,
    fixtures: tempfile::TempDir,
    state_home: tempfile::TempDir,
    log_path: PathBuf,
    original_path: String,
}

fn install_harness() -> Harness {
    let fake_bin = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let state_home = tempfile::tempdir().unwrap();
    let log_path = fixtures.path().join("podman.log");
    let original_path = std::env::var("PATH").unwrap();

    fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
    write_executable(fake_bin.path().join("podman"), &fake_podman_script());

    Harness {
        fake_bin,
        fixtures,
        state_home,
        log_path,
        original_path,
    }
}

impl Harness {
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

    fn mark_missing_during_cleanup(&self) {
        fs::write(self.fixtures.path().join("missing-during-cleanup"), "").unwrap();
    }

    fn read_log(&self) -> Vec<String> {
        match fs::read_to_string(&self.log_path) {
            Ok(contents) => contents.lines().map(|line| line.to_string()).collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("failed to read podman log: {error}"),
        }
    }
}

fn run_command(
    harness: &Harness,
    target: &Path,
    extra_args: &[&str],
) -> assert_cmd::assert::Assert {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .arg("stop")
        .args(extra_args)
        .arg(target);
    command.assert()
}

fn operation_names(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect()
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
    logical_name: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (LABEL_GIT_ROOT.to_string(), git_root.to_string()),
        (LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string()),
        (LABEL_RUNTIME.to_string(), "opencode".to_string()),
        (LABEL_IMAGE.to_string(), DEFAULT_IMAGE.to_string()),
        (LABEL_LOGICAL_NAME.to_string(), logical_name.to_string()),
    ])
}

fn managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    labels: BTreeMap<String, String>,
) -> String {
    serde_json::to_string(&vec![json!({
        "Id": container_name,
        "Created": "2026-04-21T10:15:00.000000000Z",
        "Path": "/usr/bin/opencode",
        "Args": [],
        "State": {
            "Status": if running { "running" } else { "exited" },
            "Running": running,
            "ExitCode": 0,
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
        "Mounts": [
            {
                "Type": "bind",
                "Source": git_root,
                "Destination": git_root,
                "RW": true,
            },
            {
                "Type": "volume",
                "Source": container_name,
                "Destination": REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
                "RW": true,
            }
        ],
        "NetworkSettings": {
            "Networks": {},
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
log_path=${AGENTBOX_TEST_LOG:?missing AGENTBOX_TEST_LOG}

has_flag() {
  flag=$1
  shift
  for arg in "$@"; do
    if [ "$arg" = "$flag" ]; then
      return 0
    fi
  done
  return 1
}

last_arg() {
  last=
  for arg in "$@"; do
    last=$arg
  done
  printf '%s\n' "$last"
}

cmd=$1
shift || true
printf '%s args=%s\n' "$cmd" "$*" >> "$log_path"

case "$cmd" in
  ps)
    cat "$fixtures/ps.json"
    ;;
  inspect)
    target=${1:?missing inspect target}
    fixture="$fixtures/inspect-$target.json"
    if [ ! -f "$fixture" ]; then
      printf 'no such object: %s\n' "$target" >&2
      exit 125
    fi
    cat "$fixture"
    ;;
  stop)
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
      printf 'no such object: %s\n' "$(last_arg "$@")" >&2
      exit 125
    fi
    printf 'stopped\n'
    ;;
  rm)
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
      printf 'no such object: %s\n' "$(last_arg "$@")" >&2
      exit 125
    fi
    printf 'removed\n'
    ;;
  *)
    printf 'unexpected podman invocation: %s %s\n' "$cmd" "$*" >&2
    exit 97
    ;;
esac
"#
    .to_string()
}

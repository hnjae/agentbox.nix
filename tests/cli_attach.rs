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

use agentbox::lock::lock_path_in_state_dir;
use agentbox::runtime::opencode::DEFAULT_IMAGE;
use agentbox::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};
use agentbox::workspace::resolve_workspace_identity;
use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin;
use serde_json::{Value, json};

#[path = "support/mod.rs"]
mod support;

#[test]
fn attach_to_a_running_session_attaches_to_container_stdio() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness(repo.path(), false);
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "running-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "running-id",
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

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env_with_direnv())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("attach")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "attach"]);
    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=released"));
    assert!(log[2].contains(&workspace.container_name));
    assert!(!log[2].contains("--tty"));
    assert!(!log[2].contains("opencode attach"));
    assert!(!log.iter().any(|line| line.starts_with("create ")));
}

#[test]
fn attach_to_a_stopped_session_reports_the_running_only_model() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness(repo.path(), false);
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "stopped-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "stopped-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                &workspace.container_name,
            ),
        ),
    );

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("attach")
        .arg(&target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains("is not running"))
        .stderr(predicates::str::contains(format!(
            "agentbox run {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn attach_without_an_existing_session_suggests_run() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let harness = install_harness(repo.path(), false);
    harness.write_ps(&ps_fixture(Vec::new()));

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .arg("attach")
        .arg(&target);

    command
        .assert()
        .failure()
        .stderr(predicates::str::contains(format!(
            "use `agentbox run {}` to create one",
            target.display()
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps"]);
}

struct Harness {
    fake_bin: tempfile::TempDir,
    fixtures: tempfile::TempDir,
    state_home: tempfile::TempDir,
    log_path: PathBuf,
    lock_probe_path: PathBuf,
    original_path: String,
}

fn install_harness(repo_root: &Path, include_direnv: bool) -> Harness {
    let fake_bin = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let state_home = tempfile::tempdir().unwrap();
    let log_path = repo_root.join("agentbox-attach.log");
    let lock_probe_path = cargo_bin("agentbox-lock-probe");
    let original_path = std::env::var("PATH").unwrap();

    fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
    write_executable(fake_bin.path().join("git"), &fake_git_script());
    write_executable(fake_bin.path().join("podman"), &fake_podman_script());
    if include_direnv {
        write_executable(fake_bin.path().join("direnv"), "#!/bin/sh\nexit 0\n");
    }

    Harness {
        fake_bin,
        fixtures,
        state_home,
        log_path,
        lock_probe_path,
        original_path,
    }
}

impl Harness {
    fn path_env(&self) -> String {
        format!("{}:{}", self.fake_bin.path().display(), self.original_path)
    }

    fn path_env_with_direnv(&self) -> String {
        self.path_env()
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

    fn read_log(&self) -> Vec<String> {
        match fs::read_to_string(&self.log_path) {
            Ok(contents) => contents.lines().map(|line| line.to_string()).collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("failed to read podman log: {error}"),
        }
    }

    fn lock_probe(&self) -> &Path {
        &self.lock_probe_path
    }
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
lock_path=${AGENTBOX_TEST_LOCK_PATH:-}
lock_probe=${AGENTBOX_TEST_LOCK_PROBE:-}

lock_state() {
  if [ -n "$lock_path" ] && [ -n "$lock_probe" ]; then
    "$lock_probe" "$lock_path"
  else
    printf 'unknown'
  fi
}

record() {
  op=$1
  shift
  printf '%s lock=%s args=%s\n' "$op" "$(lock_state)" "$*" >> "$log_path"
}

cmd=$1
shift || true

case "$cmd" in
  ps)
    record ps "$@"
    cat "$fixtures/ps.json"
    ;;
  inspect)
    target=${1:?missing inspect target}
    record inspect "$@"
    cat "$fixtures/inspect-$target.json"
    ;;
  attach)
    record attach "$@"
    printf 'executed\n'
    ;;
  *)
    printf 'unexpected podman invocation: %s %s\n' "$cmd" "$*" >&2
    exit 97
    ;;
esac
"#
    .to_string()
}

fn fake_git_script() -> String {
    r#"#!/bin/sh
set -eu

if [ "$1" = "-C" ] && [ "$3" = "rev-parse" ] && [ "$4" = "--show-toplevel" ]; then
    dir=$2
    while [ "$dir" != "/" ]; do
        if [ -d "$dir/.git" ]; then
            printf '%s\n' "$dir"
            exit 0
        fi
        dir=$(dirname "$dir")
    done

    printf 'fatal: not a git repository (or any of the parent directories): .git\n' >&2
    exit 128
fi

printf 'unsupported git invocation: %s\n' "$*" >&2
exit 1
"#
    .to_owned()
}

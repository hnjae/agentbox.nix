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
use agentbox::workspace::{hash12, resolve_workspace_identity};
use assert_cmd::Command as AssertCommand;
use serde_json::{Value, json};

#[path = "support/mod.rs"]
mod support;

#[test]
fn no_extra_host_metadata_is_written_beyond_locks() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness
        .run_assert(&["run", target.to_str().unwrap()])
        .success();

    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "completion-id",
        "agentbox-completion",
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "completion-id",
        &managed_inspect_fixture(
            "agentbox-completion",
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "agentbox-completion",
            ),
        ),
    );
    let completion = harness.run_output(&["__completion-roots"]).status.success();
    assert!(completion);

    let expected_lock = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);
    let files = harness.state_files();
    assert_eq!(
        files,
        vec![
            expected_lock
                .strip_prefix(harness.state_home.path())
                .unwrap()
                .to_path_buf()
        ]
    );
}

#[test]
fn stale_lock_file_is_reused_in_run_and_attach_flows() {
    let run_repo = support::temp_git_repo();
    let run_target = run_repo.path().join("run-nested");
    fs::create_dir(&run_target).unwrap();
    let run_workspace = resolve_workspace_identity(&run_target).unwrap();
    let run_harness = Harness::new();
    run_harness.write_ps(&ps_fixture(Vec::new()));
    let run_lock = lock_path_in_state_dir(run_harness.state_home.path(), &run_workspace.digest64);
    fs::create_dir_all(run_lock.parent().unwrap()).unwrap();
    fs::write(&run_lock, b"stale-lock").unwrap();

    run_harness
        .run_assert(&["run", run_target.to_str().unwrap()])
        .success();
    assert_eq!(fs::read(&run_lock).unwrap(), b"stale-lock");

    let attach_repo = support::temp_git_repo();
    let attach_target = attach_repo.path().join("attach-nested");
    fs::create_dir(&attach_target).unwrap();
    let attach_workspace = resolve_workspace_identity(&attach_target).unwrap();
    let attach_harness = Harness::new();
    let attach_lock =
        lock_path_in_state_dir(attach_harness.state_home.path(), &attach_workspace.digest64);
    fs::create_dir_all(attach_lock.parent().unwrap()).unwrap();
    fs::write(&attach_lock, b"stale-lock").unwrap();
    attach_harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "attach-id",
        &attach_workspace.container_name,
        &attach_workspace.hash12,
    )]));
    attach_harness.write_inspect(
        "attach-id",
        &managed_inspect_fixture(
            &attach_workspace.container_name,
            attach_workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                attach_workspace.canonical_git_root.as_str(),
                &attach_workspace.hash12,
                &attach_workspace.container_name,
            ),
        ),
    );

    attach_harness
        .run_assert(&["attach", attach_target.to_str().unwrap()])
        .success();
    assert_eq!(fs::read(&attach_lock).unwrap(), b"stale-lock");
}

#[test]
fn completion_uses_live_discovery_instead_of_cached_files() {
    let repo_a = support::temp_git_repo();
    let repo_b = support::temp_git_repo();
    let root_a = repo_a.path().canonicalize().unwrap();
    let root_b = repo_b.path().canonicalize().unwrap();

    let harness = Harness::new();
    let fake_cache = harness
        .state_home
        .path()
        .join("agentbox")
        .join("completion-cache.txt");
    fs::create_dir_all(fake_cache.parent().unwrap()).unwrap();
    fs::write(&fake_cache, root_a.to_str().unwrap()).unwrap();

    harness.write_live_session("live-a", root_a.to_str().unwrap());
    let first = harness.run_output(&["__completion-roots"]);
    assert!(first.status.success());
    let first = String::from_utf8(first.stdout).unwrap();
    assert!(first.contains(root_a.to_str().unwrap()));
    assert!(!first.contains(root_b.to_str().unwrap()));

    harness.write_live_session("live-b", root_b.to_str().unwrap());
    let second = harness.run_output(&["__completion-roots"]);
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    assert!(second.contains(root_b.to_str().unwrap()));
    assert!(!second.contains(root_a.to_str().unwrap()));
    assert_eq!(
        fs::read_to_string(&fake_cache).unwrap(),
        root_a.to_str().unwrap()
    );
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

        fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
        write_executable(fake_bin.path().join("git"), &fake_git_script());
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

    fn write_live_session(&self, name: &str, git_root: &str) {
        let git_root_hash = hash12(git_root.as_bytes());
        self.write_ps(&ps_fixture(vec![managed_ps_entry(
            name,
            name,
            &git_root_hash,
        )]));
        self.write_inspect(
            name,
            &managed_inspect_fixture(
                name,
                git_root,
                true,
                managed_labels(git_root, &git_root_hash, name),
            ),
        );
    }

    fn run_assert(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
        command
            .env("PATH", self.path_env())
            .env("XDG_STATE_HOME", self.state_home.path())
            .env("AGENTBOX_TEST_FIXTURES", self.fixtures.path())
            .args(args);
        command.assert()
    }

    fn run_output(&self, args: &[&str]) -> std::process::Output {
        AssertCommand::cargo_bin("agentbox")
            .unwrap()
            .env("PATH", self.path_env())
            .env("XDG_STATE_HOME", self.state_home.path())
            .env("AGENTBOX_TEST_FIXTURES", self.fixtures.path())
            .args(args)
            .output()
            .unwrap()
    }

    fn state_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_files(self.state_home.path(), self.state_home.path(), &mut files);
        files.sort();
        files
    }
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            files.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn managed_ps_entry(id: &str, name: &str, git_root_hash: &str) -> Value {
    json!({
        "Id": id,
        "Image": DEFAULT_IMAGE,
        "Command": ["sleep", "infinity"],
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
        "Path": "/usr/bin/sleep",
        "Args": ["infinity"],
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
            "Cmd": ["sleep", "infinity"],
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

case "$1" in
  ps)
    cat "$fixtures/ps.json"
    ;;
  inspect)
    cat "$fixtures/inspect-$2.json"
    ;;
  create)
    printf 'created\n'
    ;;
  start)
    printf 'started\n'
    ;;
  exec)
    printf 'ok\n'
    ;;
  *)
    printf 'unexpected podman invocation: %s\n' "$*" >&2
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
    .to_string()
}

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
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};
use agentbox::workspace::{hash12, resolve_workspace_identity};
use assert_cmd::Command as AssertCommand;
use serde_json::{Value, json};

#[path = "support/mod.rs"]
mod support;

#[test]
fn existing_managed_session_suggests_attach_and_ignores_image_override() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "existing-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "existing-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    let image_override = "registry.example/agentbox/custom:existing";
    let assert = run_command(&harness, &target, &["--image", image_override]);

    assert
        .failure()
        .stderr(predicates::str::contains(format!(
            "agentbox attach {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )))
        .stderr(predicates::str::contains(&workspace.container_name));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
    assert!(!log.iter().any(|line| line.starts_with("create ")));
    assert!(!log.iter().any(|line| line.starts_with("image ")));
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn duplicate_sessions_fail_closed() {
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
                "opencode",
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
                "opencode",
                "dup-b",
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "duplicate managed sessions exist",
        ))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "inspect"]);
}

#[test]
fn unsupported_runtime_label_requires_repair_or_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "mismatch-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "mismatch-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "other-runtime",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(
            "unsupported or malformed `io.agentbox.runtime` label",
        ))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));
}

#[test]
fn hash_collision_fails_closed() {
    let target_repo = support::temp_git_repo();
    let other_repo = support::temp_git_repo();
    let target = target_repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let other_root = other_repo.path().canonicalize().unwrap();
    let other_root = other_root.to_str().unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "collision-id",
        "collision-name",
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "collision-id",
        &managed_inspect_fixture(
            "collision-name",
            other_root,
            true,
            managed_labels(other_root, &workspace.hash12, "opencode", "collision-name"),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("hash12 prefilter"))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ))
        .stderr(predicates::str::contains(other_root));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn create_name_conflict_reports_the_conflicting_root() {
    let repo = support::temp_git_repo();
    let other_repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let other_root = other_repo.path().canonicalize().unwrap();
    let other_root = other_root.to_str().unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(Vec::new()));
    harness.fail_run("the container name is already in use", 125);
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            other_root,
            true,
            managed_labels(
                other_root,
                &hash12(other_root.as_bytes()),
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains(format!(
            "container name `{}` is already used by managed session",
            workspace.container_name
        )))
        .stderr(predicates::str::contains(other_root))
        .stderr(predicates::str::contains(
            workspace.canonical_git_root.as_str(),
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image", "run", "inspect"]);
}

#[test]
fn failed_session_with_missing_labels_requires_repair_or_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "failed-id",
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
    harness.write_inspect(
        "failed-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            labels,
        ),
    );
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            true,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            )
            .into_iter()
            .filter(|(key, _)| key != LABEL_RUNTIME)
            .collect(),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required session labels"))
        .stderr(predicates::str::contains(
            "repair or recreate it before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

#[test]
fn failed_session_with_missing_cache_mount_requires_recreation() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness();
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "missing-cache-id",
        &workspace.container_name,
        &workspace.hash12,
    )]));
    harness.write_inspect(
        "missing-cache-id",
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );
    harness.write_inspect(
        &workspace.container_name,
        &managed_inspect_fixture(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            false,
            managed_labels(
                workspace.canonical_git_root.as_str(),
                &workspace.hash12,
                "opencode",
                &workspace.container_name,
            ),
        ),
    );

    run_command(&harness, &target, &[])
        .failure()
        .stderr(predicates::str::contains("missing required cache mount"))
        .stderr(predicates::str::contains(
            REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
        ))
        .stderr(predicates::str::contains(
            "recreate the container before retrying",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
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

    fs::write(fixtures.path().join("image.exists"), "present\n").unwrap();
    fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
    write_executable(fake_bin.path().join("git"), &fake_git_script());
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

    fn fail_run(&self, stderr: &str, exit_code: i32) {
        fs::write(self.fixtures.path().join("run.stderr"), stderr).unwrap();
        fs::write(
            self.fixtures.path().join("run.exit"),
            format!("{exit_code}\n"),
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
        .args(["run", "--runtime", "opencode"])
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
            "Status": "running",
            "Running": true,
            "ExitCode": 0,
            "Pid": 4321,
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
log_path=${AGENTBOX_TEST_LOG:?missing AGENTBOX_TEST_LOG}

cmd=$1
shift || true
printf '%s args=%s\n' "$cmd" "$*" >> "$log_path"

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
    fixture="$fixtures/inspect-$target.json"
    if [ ! -f "$fixture" ]; then
      printf 'no such object: %s\n' "$target" >&2
      exit 125
    fi
    cat "$fixture"
    ;;
  run)
    if [ -f "$fixtures/run.exit" ]; then
      cat "$fixtures/run.stderr" >&2
      exit "$(tr -d '\n' < "$fixtures/run.exit")"
    fi
    printf 'started\n'
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
    .to_string()
}

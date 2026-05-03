// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::{Path, PathBuf};

use agentbox::lock::lock_path_in_state_dir;
use agentbox::metadata::LABEL_LAUNCH_DIRECTORY;
use agentbox::workspace::resolve_workspace_identity;
use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin;

#[path = "support/mod.rs"]
mod support;

use support::{
    fake_git_script, managed_inspect_fixture, managed_ps_entry,
    opencode_managed_labels as managed_labels, operation_names, path_with_prepend, ps_fixture,
    read_log_lines, write_executable,
};

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
            true,
            labels_with_launch_directory(
                managed_labels(
                    workspace.canonical_git_root.as_str(),
                    &workspace.hash12,
                    &workspace.container_name,
                ),
                workspace.canonical_target.as_str(),
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

    command.assert().success().stderr("");

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "opencode"]);
    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[2].contains("attach http://127.0.0.1:49152"));
    assert!(log[2].contains(&format!("cwd={}", workspace.canonical_target)));
    assert!(!log.iter().any(|line| line.starts_with("create ")));
    assert!(!log.iter().any(|line| line.starts_with("attach ")));
}

#[test]
fn attach_uses_stored_launch_directory_when_requesting_another_subdirectory() {
    let repo = support::temp_git_repo();
    let launch_target = repo.path().join("launch");
    let request_target = repo.path().join("request");
    fs::create_dir(&launch_target).unwrap();
    fs::create_dir(&request_target).unwrap();

    let launch_workspace = resolve_workspace_identity(&launch_target).unwrap();
    let request_workspace = resolve_workspace_identity(&request_target).unwrap();
    let harness = install_harness(repo.path(), false);
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &request_workspace.digest64);
    harness.write_ps(&ps_fixture(vec![managed_ps_entry(
        "running-id",
        &request_workspace.container_name,
        &request_workspace.hash12,
    )]));
    harness.write_inspect(
        "running-id",
        &managed_inspect_fixture(
            &request_workspace.container_name,
            request_workspace.canonical_git_root.as_str(),
            true,
            true,
            labels_with_launch_directory(
                managed_labels(
                    request_workspace.canonical_git_root.as_str(),
                    &request_workspace.hash12,
                    &request_workspace.container_name,
                ),
                launch_workspace.canonical_target.as_str(),
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
        .arg(&request_target);

    command
        .assert()
        .success()
        .stderr(predicates::str::contains("using stored launch directory"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect", "opencode"]);
    assert!(log[2].contains(&format!("cwd={}", launch_workspace.canonical_target)));
    assert!(!log[2].contains(&format!("cwd={}", request_workspace.canonical_target)));
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
            true,
            labels_with_launch_directory(
                managed_labels(
                    workspace.canonical_git_root.as_str(),
                    &workspace.hash12,
                    &workspace.container_name,
                ),
                workspace.canonical_target.as_str(),
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
            "agentbox run --runtime opencode {}",
            target.display()
        )))
        .stderr(predicates::str::contains(format!(
            "agentbox stop {}",
            target.display()
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "inspect"]);
}

fn labels_with_launch_directory(
    mut labels: std::collections::BTreeMap<String, String>,
    launch_directory: &str,
) -> std::collections::BTreeMap<String, String> {
    labels.insert(
        LABEL_LAUNCH_DIRECTORY.to_string(),
        launch_directory.to_string(),
    );
    labels
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
            "use `agentbox run --runtime <opencode|codex> {}` to create one",
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
    write_executable(fake_bin.path().join("git"), fake_git_script());
    write_executable(fake_bin.path().join("podman"), &fake_podman_script());
    write_executable(fake_bin.path().join("opencode"), &fake_opencode_script());
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
        path_with_prepend(self.fake_bin.path(), &self.original_path)
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
        read_log_lines(&self.log_path)
    }

    fn lock_probe(&self) -> &Path {
        &self.lock_probe_path
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

fn fake_opencode_script() -> String {
    r#"#!/bin/sh
set -eu

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

printf 'opencode lock=%s args=%s cwd=%s\n' "$(lock_state)" "$*" "$(pwd)" >> "$log_path"
"#
    .to_string()
}

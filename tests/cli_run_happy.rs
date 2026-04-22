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
use agentbox::runtime::opencode::DEFAULT_IMAGE;
use agentbox::workspace::resolve_workspace_identity;
use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

#[test]
fn run_creates_starts_serves_waits_and_attaches_for_a_new_session() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let workspace = resolve_workspace_identity(&target).unwrap();
    let harness = install_harness(repo.path());
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("run")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "image-exists", "build", "run"]
    );

    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[3].contains("lock=released"));

    assert!(log[2].contains(&format!("-t {DEFAULT_IMAGE} -f")));

    assert!(log[3].contains("--rm"));
    assert!(log[3].contains("--interactive"));
    assert!(!log[3].contains("--tty"));
    assert!(log[3].contains("--label io.agentbox.image=localhost/agentbox-opencode:local"));
    assert!(log[3].contains(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    )));
    assert!(log[3].contains(&format!("--name {}", workspace.container_name)));
    assert!(log[3].contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(log[3].contains(DEFAULT_IMAGE));
    assert!(log[3].contains(" opencode"));
    assert!(!log[3].contains("--publish"));
    assert!(!log[3].contains("direnv exec ."));
    assert!(!log[3].contains("sleep infinity"));
}

#[test]
fn run_wraps_foreground_command_with_direnv_when_envrc_applies() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();
    fs::write(repo.path().join(".envrc"), "use nix\n").unwrap();

    let harness = install_harness(repo.path());
    let workspace = resolve_workspace_identity(&target).unwrap();
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env_with_direnv())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("run")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    let run = log.iter().find(|line| line.starts_with("run ")).unwrap();

    assert!(run.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(run.contains("direnv exec . opencode"));
}

#[test]
fn run_uses_the_requested_image_override_exactly() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let harness = install_harness(repo.path());
    let workspace = resolve_workspace_identity(&target).unwrap();
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);
    let image = "registry.example/agentbox/custom:test";

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .args(["run", "--image", image])
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    let run = log.iter().find(|line| line.starts_with("run ")).unwrap();

    assert!(!log.iter().any(|line| line.starts_with("image-exists ")));
    assert!(!log.iter().any(|line| line.starts_with("build ")));
    assert!(run.contains(&format!("--label io.agentbox.image={image}")));
    assert!(run.contains(&format!(" {image} opencode")));
}

#[test]
fn run_skips_build_when_default_image_already_exists_locally() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let harness = install_harness(repo.path());
    harness.mark_default_image_present();
    let workspace = resolve_workspace_identity(&target).unwrap();
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("run")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    let operations = operation_names(&log);
    assert_eq!(&operations[..3], ["ps", "image-exists", "run"]);
    assert!(!log.iter().any(|line| line.starts_with("build ")));
}

#[test]
fn run_reports_default_image_build_failures_clearly() {
    let repo = support::temp_git_repo();
    let target = repo.path().join("nested");
    fs::create_dir(&target).unwrap();

    let harness = install_harness(repo.path());
    harness.fail_build("podman build exploded", 125);
    let workspace = resolve_workspace_identity(&target).unwrap();
    let lock_path = lock_path_in_state_dir(harness.state_home.path(), &workspace.digest64);

    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
    command
        .env("PATH", harness.path_env())
        .env("XDG_STATE_HOME", harness.state_home.path())
        .env("AGENTBOX_TEST_FIXTURES", harness.fixtures.path())
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .env("AGENTBOX_TEST_LOCK_PROBE", harness.lock_probe())
        .arg("run")
        .arg(&target);

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "failed to build default runtime image `{DEFAULT_IMAGE}`"
        )))
        .stderr(predicate::str::contains("podman build exploded"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["ps", "image-exists", "build"]);
}

struct Harness {
    fake_bin: tempfile::TempDir,
    fixtures: tempfile::TempDir,
    state_home: tempfile::TempDir,
    log_path: PathBuf,
    lock_probe_path: PathBuf,
    original_path: String,
}

fn install_harness(repo_root: &Path) -> Harness {
    let fake_bin = tempfile::tempdir().unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let state_home = tempfile::tempdir().unwrap();
    let log_path = repo_root.join("agentbox-run.log");
    let lock_probe_path = cargo_bin("agentbox-lock-probe");
    let original_path = std::env::var("PATH").unwrap();

    write_executable(fake_bin.path().join("podman"), &fake_podman_script());
    write_executable(fake_bin.path().join("git"), &fake_git_script());
    write_executable(fake_bin.path().join("direnv"), "#!/bin/sh\nexit 0\n");

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

    fn read_log(&self) -> Vec<String> {
        fs::read_to_string(&self.log_path)
            .unwrap()
            .lines()
            .map(|line| line.to_string())
            .collect()
    }

    fn lock_probe(&self) -> &Path {
        &self.lock_probe_path
    }

    fn fail_build(&self, stderr: &str, exit_code: i32) {
        fs::write(self.fixtures.path().join("build.stderr"), stderr).unwrap();
        fs::write(
            self.fixtures.path().join("build.exit"),
            format!("{exit_code}\n"),
        )
        .unwrap();
    }

    fn mark_default_image_present(&self) {
        fs::write(self.fixtures.path().join("image.exists"), "present\n").unwrap();
    }
}

fn operation_names(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect()
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

log_path=${AGENTBOX_TEST_LOG:?missing AGENTBOX_TEST_LOG}
lock_path=${AGENTBOX_TEST_LOCK_PATH:?missing AGENTBOX_TEST_LOCK_PATH}
lock_probe=${AGENTBOX_TEST_LOCK_PROBE:?missing AGENTBOX_TEST_LOCK_PROBE}

lock_state() {
  "$lock_probe" "$lock_path"
}

record() {
  op=$1
  shift
  printf '%s lock=%s args=%s\n' "$op" "$(lock_state)" "$*" >> "$log_path"
}

validate_build_context() {
  containerfile=
  context_dir=

  while [ "$#" -gt 0 ]; do
    case "$1" in
      -f)
        shift
        containerfile=${1:-}
        ;;
    esac

    context_dir=$1
    shift || true
  done

  [ -n "$containerfile" ] || {
    printf 'missing build containerfile argument\n' >&2
    exit 98
  }

  [ -n "$context_dir" ] || {
    printf 'missing build context directory argument\n' >&2
    exit 98
  }

  [ -r "$containerfile" ] || {
    printf 'unreadable build containerfile: %s\n' "$containerfile" >&2
    exit 98
  }

  [ "$containerfile" = "$context_dir/Containerfile" ] || {
    printf 'build containerfile %s did not match context %s\n' "$containerfile" "$context_dir" >&2
    exit 98
  }

  for relative_path in \
    Containerfile \
    bootstrap \
    entrypoint \
    lib/runtime-contract.sh \
    runtime-packages.nix
  do
    [ -r "$context_dir/$relative_path" ] || {
      printf 'missing embedded build file: %s\n' "$relative_path" >&2
      exit 98
    }
  done
}

case "$1" in
  ps)
    shift
    record ps "$@"
    printf '[]\n'
    ;;
  image)
    shift
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
      exists)
        record image-exists "$@"
        if [ -f "$AGENTBOX_TEST_FIXTURES/image.exists" ]; then
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
    shift
    validate_build_context "$@"
    record build "$@"
    if [ -f "$AGENTBOX_TEST_FIXTURES/build.exit" ]; then
      cat "$AGENTBOX_TEST_FIXTURES/build.stderr" >&2
      exit "$(tr -d '\n' < "$AGENTBOX_TEST_FIXTURES/build.exit")"
    fi
    printf 'built\n'
    ;;
  run)
    shift
    record run "$@"
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

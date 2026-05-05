// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command as AssertCommand;
use tempfile::TempDir;

use super::{path_with_prepend, read_log_lines, write_executable};

pub struct CliHarness {
    fake_bin: TempDir,
    fixtures: TempDir,
    state_home: TempDir,
    home: TempDir,
    log_path: std::path::PathBuf,
    original_path: String,
}

impl CliHarness {
    pub fn new() -> Self {
        let fake_bin = tempfile::tempdir().unwrap();
        let fixtures = tempfile::tempdir().unwrap();
        let state_home = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let log_path = fixtures.path().join("podman.log");
        let original_path = std::env::var("PATH").unwrap();

        fs::create_dir_all(home.path().join(".config/opencode")).unwrap();
        fs::create_dir_all(home.path().join(".local/share/opencode")).unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::write(fixtures.path().join("image.exists"), "present\n").unwrap();
        fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
        write_executable(fake_bin.path().join("podman"), &fake_podman_script());
        write_executable(
            fake_bin.path().join("npm"),
            "#!/bin/sh\nprintf '%s\\n' '0.99.0'\n",
        );
        write_executable(fake_bin.path().join("opencode"), "#!/bin/sh\nexit 0\n");
        write_executable(fake_bin.path().join("codex"), "#!/bin/sh\nexit 0\n");

        Self {
            fake_bin,
            fixtures,
            state_home,
            home,
            log_path,
            original_path,
        }
    }

    pub fn path_env(&self) -> String {
        path_with_prepend(self.fake_bin.path(), &self.original_path)
    }

    pub fn write_ps(&self, json: &str) {
        fs::write(self.fixtures.path().join("ps.json"), json).unwrap();
    }

    pub fn write_inspect(&self, name: &str, json: &str) {
        fs::write(
            self.fixtures.path().join(format!("inspect-{name}.json")),
            json,
        )
        .unwrap();
    }

    pub fn fail_operation(&self, kind: &str, stderr: &str, exit_code: i32) {
        fs::write(self.fixtures.path().join(format!("{kind}.stderr")), stderr).unwrap();
        fs::write(
            self.fixtures.path().join(format!("{kind}.exit")),
            format!("{exit_code}\n"),
        )
        .unwrap();
    }

    pub fn write_failure(&self, kind: &str, stderr: &str, exit_code: i32) {
        self.fail_operation(kind, stderr, exit_code);
    }

    pub fn write_logs(&self, name: &str, logs: &str) {
        fs::write(self.fixtures.path().join(format!("logs-{name}.txt")), logs).unwrap();
    }

    pub fn mark_missing_during_cleanup(&self) {
        fs::write(self.fixtures.path().join("missing-during-cleanup"), "").unwrap();
    }

    pub fn read_log(&self) -> Vec<String> {
        read_log_lines(&self.log_path)
    }

    pub fn state_home_path(&self) -> &Path {
        self.state_home.path()
    }

    pub fn state_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_files(self.state_home.path(), self.state_home.path(), &mut files);
        files.sort();
        files
    }

    pub fn agentbox_command(&self) -> AssertCommand {
        let mut command = AssertCommand::cargo_bin("agentbox").unwrap();
        command
            .env("PATH", self.path_env())
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("XDG_DATA_HOME", self.home.path().join(".local/share"))
            .env("XDG_STATE_HOME", self.state_home.path())
            .env("AGENTBOX_TEST_FIXTURES", self.fixtures.path())
            .env("AGENTBOX_TEST_LOG", &self.log_path);
        command
    }

    pub fn agentbox_assert(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command.args(args);
        command.assert()
    }

    pub fn agentbox_output(&self, args: &[&str]) -> std::process::Output {
        self.agentbox_command().args(args).output().unwrap()
    }

    pub fn run_assert(&self, target: &Path) -> assert_cmd::assert::Assert {
        self.run_assert_with_args(target, &[])
    }

    pub fn run_assert_with_args(
        &self,
        target: &Path,
        extra_args: &[&str],
    ) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command
            .args(["run", "--runtime", "opencode"])
            .args(extra_args)
            .arg(target);
        command.assert()
    }

    pub fn attach_assert(&self, target: &Path) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command.arg("attach").arg(target);
        command.assert()
    }

    pub fn stop_assert(&self, target: &Path, extra_args: &[&str]) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command.arg("stop").args(extra_args).arg(target);
        command.assert()
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

fn fake_podman_script() -> String {
    r#"#!/bin/sh
set -eu

fixtures=${AGENTBOX_TEST_FIXTURES:?missing AGENTBOX_TEST_FIXTURES}
log_path=${AGENTBOX_TEST_LOG:-}

record() {
  if [ -n "$log_path" ]; then
    op=$1
    shift
    printf '%s args=%s\n' "$op" "$*" >> "$log_path"
  fi
}

maybe_fail() {
  prefix=$1
  if [ -f "$fixtures/$prefix.exit" ]; then
    if [ -f "$fixtures/$prefix.stderr" ]; then
      cat "$fixtures/$prefix.stderr" >&2
    fi
    exit "$(tr -d '\n' < "$fixtures/$prefix.exit")"
  fi
}

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

case "$cmd" in
  ps)
    record ps "$@"
    cat "$fixtures/ps.json"
    ;;
  image)
    record image "$@"
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
  container)
    subcommand=${1:-}
    shift || true
    case "$subcommand" in
      exists)
        target=${1:?missing container exists target}
        record container-exists "$@"
        if [ -f "$fixtures/container-exists-$target" ]; then
          exit 0
        fi
        exit 1
        ;;
      *)
        printf 'unexpected podman container invocation: %s %s\n' "$subcommand" "$*" >&2
        exit 97
        ;;
    esac
    ;;
  build)
    record build "$@"
    maybe_fail build
    printf 'built\n'
    ;;
  inspect)
    target=${1:?missing inspect target}
    record inspect "$@"
    fixture="$fixtures/inspect-$target.json"
    if [ ! -f "$fixture" ]; then
      printf 'no such object: %s\n' "$target" >&2
      exit 125
    fi
    cat "$fixture"
    ;;
  run)
    record run "$@"
    maybe_fail run
    printf 'started\n'
    ;;
  logs)
    record logs "$@"
    target="$(last_arg "$@")"
    fixture="$fixtures/logs-$target.txt"
    if [ ! -f "$fixture" ]; then
      printf 'no logs for %s\n' "$target" >&2
      exit 125
    fi
    cat "$fixture"
    ;;
  stop)
    record stop "$@"
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
      printf 'no such object: %s\n' "$(last_arg "$@")" >&2
      exit 125
    fi
    maybe_fail stop
    printf 'stopped\n'
    ;;
  rm)
    record rm "$@"
    if [ -f "$fixtures/missing-during-cleanup" ] && ! has_flag --ignore "$@"; then
      printf 'no such object: %s\n' "$(last_arg "$@")" >&2
      exit 125
    fi
    maybe_fail rm
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

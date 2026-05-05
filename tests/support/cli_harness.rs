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
use agentbox::workspace::WorkspaceIdentity;
use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

use super::{fake_git_script, path_with_prepend, read_log_lines, write_executable};

pub struct CliHarness {
    fake_bin: TempDir,
    fixtures: TempDir,
    state_home: TempDir,
    home: TempDir,
    log_path: std::path::PathBuf,
    lock_probe_path: PathBuf,
    original_path: String,
}

impl CliHarness {
    pub fn new() -> Self {
        let fake_bin = tempfile::tempdir().unwrap();
        let fixtures = tempfile::tempdir().unwrap();
        let state_home = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let log_path = fixtures.path().join("podman.log");
        let lock_probe_path = cargo_bin("agentbox-lock-probe");
        let original_path = std::env::var("PATH").unwrap();

        fs::create_dir_all(home.path().join(".config/opencode")).unwrap();
        fs::create_dir_all(home.path().join(".local/share/opencode")).unwrap();
        fs::create_dir(home.path().join(".codex")).unwrap();
        fs::write(fixtures.path().join("image.exists"), "present\n").unwrap();
        fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
        write_executable(fake_bin.path().join("git"), fake_git_script());
        write_executable(fake_bin.path().join("direnv"), fake_direnv_script());
        write_executable(fake_bin.path().join("podman"), fake_podman_script());
        write_executable(
            fake_bin.path().join("npm"),
            "#!/bin/sh\nprintf '%s\\n' '0.99.0'\n",
        );
        write_executable(
            fake_bin.path().join("opencode"),
            &fake_client_script("opencode"),
        );
        write_executable(fake_bin.path().join("codex"), &fake_client_script("codex"));

        Self {
            fake_bin,
            fixtures,
            state_home,
            home,
            log_path,
            lock_probe_path,
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

    pub fn mark_default_image_absent(&self) {
        match fs::remove_file(self.fixtures.path().join("image.exists")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to remove image.exists fixture: {error}"),
        }
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

    pub fn home_path(&self) -> &Path {
        self.home.path()
    }

    pub fn lock_path(&self, workspace: &WorkspaceIdentity) -> PathBuf {
        lock_path_in_state_dir(self.state_home.path(), &workspace.digest64)
    }

    pub fn locked_agentbox_command(&self, workspace: &WorkspaceIdentity) -> AssertCommand {
        let mut command = self.agentbox_command();
        command
            .env("AGENTBOX_TEST_LOCK_PATH", self.lock_path(workspace))
            .env("AGENTBOX_TEST_LOCK_PROBE", &self.lock_probe_path);
        command
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

fn fake_podman_script() -> &'static str {
    include_str!("../fixtures/bin/podman.sh")
}

fn fake_client_script(name: &str) -> String {
    format!(
        r#"#!/bin/sh
set -eu

log_path=${{AGENTBOX_TEST_LOG:-}}

lock_state() {{
  lock_path=${{AGENTBOX_TEST_LOCK_PATH:-}}
  lock_probe=${{AGENTBOX_TEST_LOCK_PROBE:-}}
  if [ -n "$lock_path" ] && [ -n "$lock_probe" ]; then
    "$lock_probe" "$lock_path"
  else
    printf 'unknown'
  fi
}}

if [ -n "$log_path" ]; then
  printf '{name} lock=%s args=%s cwd=%s\n' "$(lock_state)" "$*" "$(pwd)" >> "$log_path"
fi
"#
    )
}

fn fake_direnv_script() -> &'static str {
    include_str!("../fixtures/bin/direnv.sh")
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use agentbox::lock::lock_path_in_state_dir;
use agentbox::runtime::RuntimeKind;
use agentbox::workspace::WorkspaceIdentity;
use assert_cmd::Command as AssertCommand;
use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

use super::{
    CommandLog, default_runtime_images_fixture, fake_git_script, path_with_prepend, read_log_lines,
    write_executable,
};

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
        for runtime in RuntimeKind::variants() {
            fs::write(
                image_exists_fixture_path(fixtures.path(), &runtime.default_image()),
                "present\n",
            )
            .unwrap();
        }
        fs::write(
            fixtures.path().join("images.json"),
            default_runtime_images_fixture(),
        )
        .unwrap();
        fs::write(fixtures.path().join("ps.json"), "[]\n").unwrap();
        fs::write(fixtures.path().join("volumes.json"), "[]\n").unwrap();
        write_executable(fake_bin.path().join("git"), fake_git_script());
        write_executable(fake_bin.path().join("cat"), fake_cat_script());
        write_executable(fake_bin.path().join("dirname"), fake_dirname_script());
        write_executable(fake_bin.path().join("direnv"), fake_direnv_script());
        write_executable(fake_bin.path().join("nix"), fake_nix_script());
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

    pub fn fake_bin_only_path_env(&self) -> String {
        self.fake_bin.path().display().to_string()
    }

    pub fn remove_fake_program(&self, name: &str) {
        remove_file_if_exists(self.fake_bin.path().join(name));
    }

    pub fn write_ps(&self, json: &str) {
        fs::write(self.fixtures.path().join("ps.json"), json).unwrap();
    }

    pub fn write_volumes(&self, json: &str) {
        fs::write(self.fixtures.path().join("volumes.json"), json).unwrap();
    }

    pub fn write_images(&self, json: &str) {
        fs::write(self.fixtures.path().join("images.json"), json).unwrap();
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
        remove_file_if_exists(self.fixtures.path().join("image.exists"));
        for entry in fs::read_dir(self.fixtures.path()).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("image-exists-") {
                remove_file_if_exists(entry.path());
            }
        }
    }

    pub fn mark_image_present(&self, image: &str) {
        fs::write(
            image_exists_fixture_path(self.fixtures.path(), image),
            "present\n",
        )
        .unwrap();
    }

    pub fn write_logs(&self, name: &str, logs: &str) {
        fs::write(self.fixtures.path().join(format!("logs-{name}.txt")), logs).unwrap();
    }

    pub fn write_codex_attach_token(&self, workspace: &WorkspaceIdentity, token: &str) -> PathBuf {
        let path = self.codex_attach_token_path(workspace);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, token).unwrap();
        path
    }

    pub fn codex_attach_token_path(&self, workspace: &WorkspaceIdentity) -> PathBuf {
        self.state_home
            .path()
            .join("agentbox/codex/ws-tokens")
            .join(format!("{}.token", workspace.digest64))
    }

    pub fn write_git_config(&self, key: &str, value: &str) {
        fs::write(
            self.fixtures
                .path()
                .join(format!("git-config-{}", safe_git_config_key(key))),
            value,
        )
        .unwrap();
    }

    pub fn write_git_config_path(&self, key: &str, value: &str) {
        fs::write(
            self.fixtures
                .path()
                .join(format!("git-config-path-{}", safe_git_config_key(key))),
            value,
        )
        .unwrap();
    }

    pub fn write_git_remotes(&self, remotes: &str) {
        fs::write(self.fixtures.path().join("git-remotes"), remotes).unwrap();
    }

    pub fn write_fake_program(&self, name: &str, content: &str) {
        write_executable(self.fake_bin.path().join(name), content);
    }

    pub fn write_agentbox_config(&self, contents: &str) {
        let path = self.home.path().join(".config/agentbox/config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    pub fn captured_known_hosts(&self) -> Option<String> {
        match fs::read_to_string(self.fixtures.path().join("known-hosts-captured")) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("failed to read captured known_hosts fixture: {error}"),
        }
    }

    pub fn mark_dev_shell(&self, flake_root: &Path, attr: &str) {
        fs::write(
            self.fixtures
                .path()
                .join(dev_shell_fixture_name(flake_root, attr)),
            "present\n",
        )
        .unwrap();
    }

    pub fn fail_nix_eval(&self, flake_root: &Path, stderr: &str) {
        fs::write(
            self.fixtures.path().join(format!(
                "nix-eval-fail-{}.stderr",
                safe_path_name(flake_root)
            )),
            stderr,
        )
        .unwrap();
    }

    pub fn mark_missing_during_cleanup(&self) {
        fs::write(self.fixtures.path().join("missing-during-cleanup"), "").unwrap();
    }

    pub fn mark_container_exists(&self, name: &str) {
        fs::write(
            self.fixtures
                .path()
                .join(format!("container-exists-{name}")),
            "",
        )
        .unwrap();
    }

    pub fn mark_volume_exists(&self, name: &str) {
        fs::write(
            self.fixtures.path().join(format!("volume-exists-{name}")),
            "",
        )
        .unwrap();
    }

    pub fn read_log(&self) -> Vec<String> {
        read_log_lines(&self.log_path)
    }

    pub fn command_log(&self) -> CommandLog {
        CommandLog::from_lines(self.read_log())
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
        self.configure_lock_probe_env(&mut command, workspace);
        command
    }

    pub fn locked_agentbox_process_command(&self, workspace: &WorkspaceIdentity) -> Command {
        let mut command = self.agentbox_process_command();
        self.configure_lock_probe_env(&mut command, workspace);
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
        self.configure_agentbox_env(&mut command);
        command
    }

    pub fn agentbox_process_command(&self) -> Command {
        let mut command = Command::new(cargo_bin("agentbox"));
        self.configure_agentbox_env(&mut command);
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

    pub fn agentbox_success_stdout(&self, args: &[&str]) -> String {
        let output = self.agentbox_output(args);
        let stderr = String::from_utf8(output.stderr).unwrap();

        assert!(
            output.status.success(),
            "agentbox {args:?} exited with {}; stderr:\n{stderr}",
            output.status,
        );
        assert!(
            stderr.is_empty(),
            "agentbox {args:?} wrote unexpected stderr:\n{stderr}"
        );

        String::from_utf8(output.stdout).unwrap()
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

    pub fn start_assert_with_args(
        &self,
        target: &Path,
        extra_args: &[&str],
    ) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command
            .args(["start", "--runtime", "opencode"])
            .args(extra_args)
            .arg(target);
        command.assert()
    }

    pub fn connect_assert(&self, target: &Path) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command.arg("connect").arg(target);
        command.assert()
    }

    pub fn stop_assert(&self, target: &Path, extra_args: &[&str]) -> assert_cmd::assert::Assert {
        let mut command = self.agentbox_command();
        command.arg("stop").args(extra_args).arg(target);
        command.assert()
    }

    fn configure_agentbox_env(&self, command: &mut impl CommandEnv) {
        for (key, value) in self.agentbox_env() {
            command.set_env(key, &value);
        }
        command.remove_env("CODEX_HOME");
        command.remove_env("SSH_AUTH_SOCK");
    }

    fn configure_lock_probe_env(
        &self,
        command: &mut impl CommandEnv,
        workspace: &WorkspaceIdentity,
    ) {
        command.set_env(
            "AGENTBOX_TEST_LOCK_PATH",
            self.lock_path(workspace).as_os_str(),
        );
        command.set_env("AGENTBOX_TEST_LOCK_PROBE", self.lock_probe_path.as_os_str());
    }

    fn agentbox_env(&self) -> [(&'static str, OsString); 7] {
        [
            ("PATH", OsString::from(self.path_env())),
            ("HOME", self.home.path().as_os_str().to_os_string()),
            (
                "XDG_CONFIG_HOME",
                self.home.path().join(".config").into_os_string(),
            ),
            (
                "XDG_DATA_HOME",
                self.home.path().join(".local/share").into_os_string(),
            ),
            (
                "XDG_STATE_HOME",
                self.state_home.path().as_os_str().to_os_string(),
            ),
            (
                "AGENTBOX_TEST_FIXTURES",
                self.fixtures.path().as_os_str().to_os_string(),
            ),
            (
                "AGENTBOX_TEST_LOG",
                self.log_path.as_os_str().to_os_string(),
            ),
        ]
    }
}

trait CommandEnv {
    fn set_env(&mut self, key: &'static str, value: &OsStr);
    fn remove_env(&mut self, key: &'static str);
}

impl CommandEnv for AssertCommand {
    fn set_env(&mut self, key: &'static str, value: &OsStr) {
        self.env(key, value);
    }

    fn remove_env(&mut self, key: &'static str) {
        self.env_remove(key);
    }
}

impl CommandEnv for Command {
    fn set_env(&mut self, key: &'static str, value: &OsStr) {
        self.env(key, value);
    }

    fn remove_env(&mut self, key: &'static str) {
        self.env_remove(key);
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

fn remove_file_if_exists(path: impl AsRef<Path>) {
    match fs::remove_file(path.as_ref()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!(
            "failed to remove fixture file `{}`: {error}",
            path.as_ref().display()
        ),
    }
}

fn image_exists_fixture_path(root: &Path, image: &str) -> PathBuf {
    root.join(format!("image-exists-{}", safe_image_name(image)))
}

fn safe_image_name(image: &str) -> String {
    image
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn dev_shell_fixture_name(flake_root: &Path, attr: &str) -> String {
    format!(
        "devshell-{}-{}",
        safe_path_name(flake_root),
        safe_image_name(attr)
    )
}

fn safe_path_name(path: &Path) -> String {
    safe_image_name(&path.to_string_lossy())
}

fn safe_git_config_key(key: &str) -> String {
    key.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn fake_podman_script() -> &'static str {
    include_str!("../fixtures/bin/podman.sh")
}

fn fake_nix_script() -> &'static str {
    include_str!("../fixtures/bin/nix.sh")
}

fn fake_cat_script() -> &'static str {
    r#"#!/bin/sh
set -eu

emit() {
    while IFS= read -r line || [ -n "$line" ]; do
        printf '%s\n' "$line"
    done
}

if [ "$#" -eq 0 ]; then
    emit
else
    for path in "$@"; do
        emit < "$path"
    done
fi
"#
}

fn fake_dirname_script() -> &'static str {
    r#"#!/bin/sh
set -eu

path=${1%/}
case "$path" in
    */*)
        dir=${path%/*}
        if [ -z "$dir" ]; then
            dir=/
        fi
        ;;
    *)
        dir=.
        ;;
esac

printf '%s\n' "$dir"
"#
}

fn fake_client_script(name: &str) -> String {
    format!(
        r#"#!/bin/sh
set -eu

log_path=${{AGENTBOX_TEST_LOG:-}}
fixtures=${{AGENTBOX_TEST_FIXTURES:-}}

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
  printf '{name} lock=%s args=%s cwd=%s env=NO_PROXY=%s no_proxy=%s http_proxy=%s HTTPS_PROXY=%s all_proxy=%s\n' "$(lock_state)" "$*" "$(pwd)" "${{NO_PROXY-}}" "${{no_proxy-}}" "${{http_proxy-}}" "${{HTTPS_PROXY-}}" "${{all_proxy-}}" >> "$log_path"
fi

if [ -n "$fixtures" ] && [ -f "$fixtures/{name}.exit" ]; then
  if [ -f "$fixtures/{name}.stderr" ]; then
    cat "$fixtures/{name}.stderr" >&2
  fi
  exit "$(tr -d '\n' < "$fixtures/{name}.exit")"
fi
"#
    )
}

fn fake_direnv_script() -> &'static str {
    include_str!("../fixtures/bin/direnv.sh")
}

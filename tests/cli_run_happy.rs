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
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .arg("run")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    assert_eq!(
        operation_names(&log),
        ["ps", "create", "start", "serve", "ready", "attach"]
    );

    assert!(log[0].contains("lock=held"));
    assert!(log[1].contains("lock=held"));
    assert!(log[2].contains("lock=held"));
    assert!(log[3].contains("lock=held"));
    assert!(log[4].contains("lock=held"));
    assert!(log[5].contains("lock=released"));

    assert!(log[1].contains("--label io.agentbox.image=localhost/agentbox-opencode:local"));
    assert!(log[1].contains(&format!(
        "--label io.agentbox.git_root={}",
        workspace.canonical_git_root
    )));
    assert!(log[1].contains(&format!("--name {}", workspace.container_name)));
    assert!(log[1].contains(DEFAULT_IMAGE));
    assert!(!log[1].contains("--publish"));

    assert!(log[3].contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(log[3].contains("/entrypoint opencode serve --hostname 127.0.0.1 --port 4096"));
    assert!(!log[3].contains("direnv exec ."));
    assert!(
        log[4].contains("/entrypoint curl --max-time 2 -sf http://127.0.0.1:4096/global/health")
    );
    assert!(log[5].contains(&format!(
        "/entrypoint opencode attach http://127.0.0.1:4096 --dir {}",
        workspace.canonical_target
    )));
}

#[test]
fn run_wraps_server_start_with_direnv_when_envrc_applies() {
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
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .arg("run")
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    let serve = log.iter().find(|line| line.starts_with("serve ")).unwrap();
    let attach = log.iter().find(|line| line.starts_with("attach ")).unwrap();

    assert!(serve.contains(&format!("--workdir {}", workspace.canonical_target)));
    assert!(
        serve.contains("direnv exec . /entrypoint opencode serve --hostname 127.0.0.1 --port 4096")
    );
    assert!(!attach.contains("direnv exec ."));
}

#[test]
fn run_persists_the_first_create_image_override_exactly() {
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
        .env("AGENTBOX_TEST_LOG", &harness.log_path)
        .env("AGENTBOX_TEST_LOCK_PATH", &lock_path)
        .args(["run", "--image", image])
        .arg(&target);

    command.assert().success();

    let log = harness.read_log();
    let create = log.iter().find(|line| line.starts_with("create ")).unwrap();

    assert!(create.contains(&format!("--label io.agentbox.image={image}")));
    assert!(create.contains(&format!(" {image} sleep infinity")));
}

struct Harness {
    fake_bin: tempfile::TempDir,
    state_home: tempfile::TempDir,
    log_path: PathBuf,
    original_path: String,
}

fn install_harness(repo_root: &Path) -> Harness {
    let fake_bin = tempfile::tempdir().unwrap();
    let state_home = tempfile::tempdir().unwrap();
    let log_path = repo_root.join("agentbox-run.log");
    let original_path = std::env::var("PATH").unwrap();

    write_executable(fake_bin.path().join("podman"), &fake_podman_script());
    write_executable(fake_bin.path().join("direnv"), "#!/bin/sh\nexit 0\n");

    Harness {
        fake_bin,
        state_home,
        log_path,
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

lock_state() {
  python3 - "$lock_path" <<'PY'
import fcntl
import os
import sys

fd = os.open(sys.argv[1], os.O_RDWR | os.O_CREAT, 0o666)
try:
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print("held")
    else:
        print("released")
finally:
    os.close(fd)
PY
}

record() {
  op=$1
  shift
  printf '%s lock=%s args=%s\n' "$op" "$(lock_state)" "$*" >> "$log_path"
}

case "$1" in
  ps)
    shift
    record ps "$@"
    printf '[]\n'
    ;;
  create)
    shift
    record create "$@"
    printf 'created\n'
    ;;
  start)
    shift
    record start "$@"
    printf 'started\n'
    ;;
  exec)
    shift
    op=attach
    case "$*" in
      --detach*)
        op=serve
        ;;
      *'/entrypoint curl '* )
        op=ready
        ;;
    esac
    record "$op" "$@"
    ;;
  *)
    printf 'unexpected podman invocation: %s\n' "$*" >&2
    exit 97
    ;;
esac
"#
    .to_string()
}

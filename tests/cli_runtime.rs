// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use agentbox::runtime::RuntimeKind;
use agentbox::runtime::default_image::default_image_context_hash;
use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{CliHarness, operation_names};

#[test]
fn runtime_update_codex_rebuilds_and_records_state_when_state_is_missing() {
    let harness = CliHarness::new();
    let image = RuntimeKind::Codex.default_image();
    let context_hash = default_image_context_hash();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "codex"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "INFO: updated codex runtime image `{image}` to 0.99.0"
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image", "build"]);
    assert!(log[1].contains(&format!("-t {image}")));
    assert!(log[1].contains("--build-arg AGENTBOX_RUNTIME=codex"));
    assert!(log[1].contains("--build-arg CODEX_NPM_VERSION=0.99.0"));
    assert!(log[1].contains("--label io.agentbox.default_runtime_image=true"));
    assert!(log[1].contains("--label io.agentbox.runtime=codex"));
    assert!(log[1].contains(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    )));
    assert!(log[1].contains("--label io.agentbox.codex.package=@openai/codex"));
    assert!(log[1].contains("--label io.agentbox.codex.version=0.99.0"));

    let state = fs::read_to_string(codex_state_path(&harness)).unwrap();
    assert!(state.contains("\"runtime\": \"codex\""));
    assert!(state.contains("\"package\": \"@openai/codex\""));
    assert!(state.contains("\"install_source\": \"npm\""));
    assert!(state.contains(&format!("\"image\": \"{image}\"")));
    assert!(state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"latest_seen_version\": \"0.99.0\""));
}

#[test]
fn runtime_update_opencode_rebuilds_and_records_state_when_state_is_missing() {
    let harness = CliHarness::new();
    let image = RuntimeKind::Opencode.default_image();
    let context_hash = default_image_context_hash();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "opencode"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "INFO: updated opencode runtime image `{image}` to 0.99.0"
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image", "build"]);
    assert!(log[1].contains(&format!("-t {image}")));
    assert!(log[1].contains("--build-arg AGENTBOX_RUNTIME=opencode"));
    assert!(log[1].contains("--build-arg OPENCODE_NPM_VERSION=0.99.0"));
    assert!(log[1].contains("--label io.agentbox.default_runtime_image=true"));
    assert!(log[1].contains("--label io.agentbox.runtime=opencode"));
    assert!(log[1].contains(&format!(
        "--label io.agentbox.image_context_hash={context_hash}"
    )));
    assert!(log[1].contains("--label io.agentbox.opencode.package=opencode-ai"));
    assert!(log[1].contains("--label io.agentbox.opencode.version=0.99.0"));

    let state = fs::read_to_string(opencode_state_path(&harness)).unwrap();
    assert!(state.contains("\"runtime\": \"opencode\""));
    assert!(state.contains("\"package\": \"opencode-ai\""));
    assert!(state.contains("\"install_source\": \"npm\""));
    assert!(state.contains(&format!("\"image\": \"{image}\"")));
    assert!(state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"latest_seen_version\": \"0.99.0\""));
}

#[test]
fn runtime_update_all_rebuilds_and_records_state_when_states_are_missing() {
    let harness = CliHarness::new();
    let opencode_image = RuntimeKind::Opencode.default_image();
    let codex_image = RuntimeKind::Codex.default_image();
    let context_hash = default_image_context_hash();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "--all"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains(format!(
                "INFO: updated opencode runtime image `{opencode_image}` to 0.99.0"
            ))
            .and(predicate::str::contains(format!(
                "INFO: updated codex runtime image `{codex_image}` to 0.99.0"
            ))),
        );

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image", "build", "image", "build"]);
    assert!(log[1].contains(&format!("-t {opencode_image}")));
    assert!(log[1].contains("--build-arg AGENTBOX_RUNTIME=opencode"));
    assert!(log[3].contains(&format!("-t {codex_image}")));
    assert!(log[3].contains("--build-arg AGENTBOX_RUNTIME=codex"));

    let opencode_state = fs::read_to_string(opencode_state_path(&harness)).unwrap();
    assert!(opencode_state.contains("\"runtime\": \"opencode\""));
    assert!(opencode_state.contains("\"package\": \"opencode-ai\""));
    assert!(opencode_state.contains(&format!("\"image\": \"{opencode_image}\"")));
    assert!(opencode_state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(opencode_state.contains("\"installed_version\": \"0.99.0\""));
    assert!(opencode_state.contains("\"latest_seen_version\": \"0.99.0\""));

    let codex_state = fs::read_to_string(codex_state_path(&harness)).unwrap();
    assert!(codex_state.contains("\"runtime\": \"codex\""));
    assert!(codex_state.contains("\"package\": \"@openai/codex\""));
    assert!(codex_state.contains(&format!("\"image\": \"{codex_image}\"")));
    assert!(codex_state.contains(&format!("\"image_context_hash\": \"{context_hash}\"")));
    assert!(codex_state.contains("\"installed_version\": \"0.99.0\""));
    assert!(codex_state.contains("\"latest_seen_version\": \"0.99.0\""));
}

#[test]
fn runtime_update_all_stops_before_later_runtimes_when_first_update_fails() {
    let harness = CliHarness::new();
    harness.fail_operation("build", "podman build exploded", 125);

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "--all"]);
    command
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("podman build exploded"));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image", "build"]);
    assert!(log[1].contains("--build-arg AGENTBOX_RUNTIME=opencode"));
    assert!(!opencode_state_path(&harness).exists());
    assert!(!codex_state_path(&harness).exists());
}

#[test]
fn runtime_update_non_tty_uses_stable_stderr_logs_and_preserves_stdout() {
    let harness = CliHarness::new();
    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "codex"]);

    let output = command.output().unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.contains(&b'\r'));
    assert!(!output.stderr.contains(&0x1b));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("INFO:") && line.contains("resolving"))
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("INFO:") && line.contains("building"))
    );
}

#[test]
fn runtime_update_verbose_streams_both_build_streams_before_exit_and_retains_failure() {
    let harness = CliHarness::new();
    harness.hold_build_with_output("stdout-before-release\n", "stderr-before-release\n");
    harness.fail_operation("build", "captured-build-failure\n", 125);
    let mut command = harness.agentbox_process_command();
    command
        .args(["--verbose", "runtime", "update", "codex"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (lines_tx, lines_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut lines = Vec::new();
        for line in BufReader::new(stderr).lines() {
            let line = line.unwrap();
            lines_tx.send(line.clone()).unwrap();
            lines.push(line);
        }
        lines
    });

    harness.wait_for_build_output();
    let mut saw_stdout = false;
    let mut saw_stderr = false;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !(saw_stdout && saw_stderr) && Instant::now() < deadline {
        match lines_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => {
                saw_stdout |= line.contains("DEBUG:") && line.contains("stdout-before-release");
                saw_stderr |= line.contains("DEBUG:") && line.contains("stderr-before-release");
            }
            Err(_) => break,
        }
    }
    let still_running = child.try_wait().unwrap().is_none();
    harness.release_build();
    let status = child.wait().unwrap();
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let stderr = reader.join().unwrap().join("\n");

    assert!(
        saw_stdout,
        "stdout was not forwarded before release; stderr:\n{stderr}"
    );
    assert!(
        saw_stderr,
        "stderr was not forwarded before release; stderr:\n{stderr}"
    );
    assert!(still_running, "build exited before the release signal");
    assert!(!status.success());
    assert!(stdout.is_empty());
    assert!(stderr.contains("captured-build-failure"));
}

#[test]
fn runtime_update_codex_skips_rebuild_when_image_and_state_are_current() {
    let harness = CliHarness::new();
    let image = RuntimeKind::Codex.default_image();
    let context_hash = default_image_context_hash();
    let state_path = codex_state_path(&harness);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        format!(
            r#"{{
  "runtime": "codex",
  "package": "@openai/codex",
  "install_source": "npm",
  "image": "{image}",
  "image_context_hash": "{context_hash}",
  "installed_version": "0.99.0",
  "latest_seen_version": "0.99.0",
  "latest_checked_at": 1,
  "image_built_at": 1
}}
"#
        ),
    )
    .unwrap();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "codex"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "INFO: codex runtime image `{image}` is already up to date at 0.99.0"
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image"]);
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"image_built_at\": 1"));
}

#[test]
fn runtime_update_opencode_skips_rebuild_when_image_and_state_are_current() {
    let harness = CliHarness::new();
    let image = RuntimeKind::Opencode.default_image();
    let context_hash = default_image_context_hash();
    let state_path = opencode_state_path(&harness);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        format!(
            r#"{{
  "runtime": "opencode",
  "package": "opencode-ai",
  "install_source": "npm",
  "image": "{image}",
  "image_context_hash": "{context_hash}",
  "installed_version": "0.99.0",
  "latest_seen_version": "0.99.0",
  "latest_checked_at": 1,
  "image_built_at": 1
}}
"#
        ),
    )
    .unwrap();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "opencode"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "INFO: opencode runtime image `{image}` is already up to date at 0.99.0"
        )));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image"]);
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"image_built_at\": 1"));
}

#[test]
fn runtime_update_rejects_state_without_image_context_hash() {
    let harness = CliHarness::new();
    let image = RuntimeKind::Opencode.default_image();
    let state_path = opencode_state_path(&harness);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        format!(
            r#"{{
  "runtime": "opencode",
  "package": "opencode-ai",
  "install_source": "npm",
  "image": "{image}",
  "installed_version": "0.99.0",
  "latest_seen_version": "0.99.0",
  "latest_checked_at": 1,
  "image_built_at": 1
}}
"#
        ),
    )
    .unwrap();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "opencode"]);
    command
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "failed to parse opencode runtime image state",
        ))
        .stderr(predicate::str::contains(
            "missing field `image_context_hash`",
        ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image"]);
}

fn codex_state_path(harness: &CliHarness) -> std::path::PathBuf {
    runtime_state_path(harness, "codex")
}

fn opencode_state_path(harness: &CliHarness) -> std::path::PathBuf {
    runtime_state_path(harness, "opencode")
}

fn runtime_state_path(harness: &CliHarness, runtime: &str) -> std::path::PathBuf {
    harness
        .state_home_path()
        .join(format!("agentbox/runtime/{runtime}.json"))
}

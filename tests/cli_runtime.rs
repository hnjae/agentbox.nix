// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use predicates::prelude::*;

#[path = "support/mod.rs"]
mod support;

use support::{CliHarness, operation_names};

#[test]
fn runtime_update_codex_rebuilds_and_records_state_when_state_is_missing() {
    let harness = CliHarness::new();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "codex"]);
    command.assert().success().stdout(predicate::str::contains(
        "updated codex runtime image `localhost/agentbox-codex:local` to 0.99.0",
    ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image", "build"]);
    assert!(log[1].contains("-t localhost/agentbox-codex:local"));
    assert!(log[1].contains("--build-arg AGENTBOX_RUNTIME=codex"));
    assert!(log[1].contains("--build-arg CODEX_NPM_VERSION=0.99.0"));
    assert!(log[1].contains("--label io.agentbox.codex.package=@openai/codex"));
    assert!(log[1].contains("--label io.agentbox.codex.version=0.99.0"));

    let state = fs::read_to_string(codex_state_path(&harness)).unwrap();
    assert!(state.contains("\"runtime\": \"codex\""));
    assert!(state.contains("\"package\": \"@openai/codex\""));
    assert!(state.contains("\"install_source\": \"npm\""));
    assert!(state.contains("\"image\": \"localhost/agentbox-codex:local\""));
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"latest_seen_version\": \"0.99.0\""));
}

#[test]
fn runtime_update_codex_skips_rebuild_when_image_and_state_are_current() {
    let harness = CliHarness::new();
    let state_path = codex_state_path(&harness);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        r#"{
  "runtime": "codex",
  "package": "@openai/codex",
  "install_source": "npm",
  "image": "localhost/agentbox-codex:local",
  "installed_version": "0.99.0",
  "latest_seen_version": "0.99.0",
  "latest_checked_at": 1,
  "image_built_at": 1
}
"#,
    )
    .unwrap();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "codex"]);
    command.assert().success().stdout(predicate::str::contains(
        "codex runtime image `localhost/agentbox-codex:local` is already up to date at 0.99.0",
    ));

    let log = harness.read_log();
    assert_eq!(operation_names(&log), ["image"]);
    let state = fs::read_to_string(state_path).unwrap();
    assert!(state.contains("\"installed_version\": \"0.99.0\""));
    assert!(state.contains("\"image_built_at\": 1"));
}

#[test]
fn runtime_update_rejects_unsupported_runtime() {
    let harness = CliHarness::new();

    let mut command = harness.agentbox_command();
    command.args(["runtime", "update", "opencode"]);
    command.assert().failure().stderr(predicate::str::contains(
        "`agentbox runtime update` supports `codex` only in the MVP",
    ));
}

fn codex_state_path(harness: &CliHarness) -> std::path::PathBuf {
    harness
        .state_home_path()
        .join("agentbox/runtime/codex.json")
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod support;

use std::fs;

use predicates::prelude::*;
use support::CliHarness;

const DEFAULT_CONFIG: &str = "{\n  \"knownHosts\": [],\n  \"defaultResourceLimits\": {}\n}\n";

#[test]
fn config_init_writes_default_config() {
    let harness = CliHarness::new();
    let config_path = harness.home_path().join(".config/agentbox/config.json");

    harness
        .agentbox_assert(&["config", "init"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "wrote agentbox config `{}`",
            config_path.display()
        )));

    assert_eq!(fs::read_to_string(config_path).unwrap(), DEFAULT_CONFIG);
}

#[test]
fn config_init_refuses_to_overwrite_existing_config() {
    let harness = CliHarness::new();
    let config_path = harness.home_path().join(".config/agentbox/config.json");
    harness.write_agentbox_config("{\"knownHosts\":[\"existing.example ssh-ed25519 AAAA\"]}\n");

    harness
        .agentbox_assert(&["config", "init"])
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "agentbox config `{}` already exists",
            config_path.display()
        )));

    assert_eq!(
        fs::read_to_string(config_path).unwrap(),
        "{\"knownHosts\":[\"existing.example ssh-ed25519 AAAA\"]}\n"
    );
}

#[test]
fn config_init_force_overwrites_existing_config() {
    let harness = CliHarness::new();
    let config_path = harness.home_path().join(".config/agentbox/config.json");
    harness.write_agentbox_config("{\"knownHosts\":[\"existing.example ssh-ed25519 AAAA\"]}\n");

    harness
        .agentbox_assert(&["config", "init", "--force"])
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "wrote agentbox config `{}`",
            config_path.display()
        )));

    assert_eq!(fs::read_to_string(config_path).unwrap(), DEFAULT_CONFIG);
}

#[test]
fn config_init_requires_config_home() {
    let harness = CliHarness::new();
    let mut command = harness.agentbox_command();
    command
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .args(["config", "init"]);

    command
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "cannot determine agentbox config path because neither XDG_CONFIG_HOME nor HOME is set",
        ));
}

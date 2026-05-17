// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use assert_cmd::Command as AssertCommand;

use agentbox::cli::{CompletionShell, DevEnvMode, OutputFormat};
use agentbox::metadata::LABEL_ATTACH_SCHEME;
use agentbox::runtime::RuntimeKind;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as LiveHarness, managed_inspect_fixture, opencode_transient_run_labels,
    opencode_workspace_inspect_fixture, opencode_workspace_labels, ps_fixture,
    transient_run_ps_entry, workspace_ps_entry,
};

#[test]
fn helper_returns_live_roots_with_runtime_and_status_metadata() {
    let fixture = support::temp_workspace("nested");
    let workspace = &fixture.workspace;
    let harness = LiveHarness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &opencode_workspace_inspect_fixture(workspace, true, true),
    );

    let output = harness
        .agentbox_command()
        .args(["__completion-roots", "connect"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let output = String::from_utf8(output.stdout).unwrap();

    assert!(output.contains(workspace.canonical_git_root.as_str()));
    assert!(output.contains("opencode"));
}

#[test]
fn helper_filters_connect_and_stop_candidates_by_command() {
    let running_fixture = support::temp_workspace("running");
    let run_fixture = support::temp_workspace("run");
    let failed_fixture = support::temp_workspace("failed");
    let running_workspace = &running_fixture.workspace;
    let transient_workspace = &run_fixture.workspace;
    let failed_workspace = &failed_fixture.workspace;
    let harness = LiveHarness::new();

    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("running-id", running_workspace),
        transient_run_ps_entry(
            "run-id",
            &transient_workspace.container_name,
            &transient_workspace.hash12,
        ),
        workspace_ps_entry("failed-id", failed_workspace),
    ]));
    harness.write_inspect(
        "running-id",
        &opencode_workspace_inspect_fixture(running_workspace, true, true),
    );
    let mut failed_labels = opencode_workspace_labels(failed_workspace);
    failed_labels.remove(LABEL_ATTACH_SCHEME);
    harness.write_inspect(
        "run-id",
        &managed_inspect_fixture(
            &transient_workspace.container_name,
            transient_workspace.canonical_git_root.as_str(),
            true,
            true,
            opencode_transient_run_labels(
                transient_workspace.canonical_git_root.as_str(),
                &transient_workspace.hash12,
                &transient_workspace.container_name,
            ),
        ),
    );
    harness.write_inspect(
        "failed-id",
        &managed_inspect_fixture(
            &failed_workspace.container_name,
            failed_workspace.canonical_git_root.as_str(),
            true,
            true,
            failed_labels,
        ),
    );

    let connect = harness
        .agentbox_command()
        .args(["__completion-roots", "connect"])
        .output()
        .unwrap();
    assert!(connect.status.success());
    assert!(connect.stderr.is_empty());
    let connect = String::from_utf8(connect.stdout).unwrap();
    assert_eq!(
        first_candidate_value(&connect),
        running_workspace.canonical_git_root.as_str()
    );
    assert!(connect.contains(running_workspace.canonical_git_root.as_str()));
    assert!(!connect.contains(transient_workspace.canonical_git_root.as_str()));
    assert!(!connect.contains(failed_workspace.canonical_git_root.as_str()));

    let stop = harness
        .agentbox_command()
        .args(["__completion-roots", "stop"])
        .output()
        .unwrap();
    assert!(stop.status.success());
    assert!(stop.stderr.is_empty());
    let stop = String::from_utf8(stop.stdout).unwrap();
    assert_eq!(first_candidate_value(&stop), running_workspace.hash12);
    assert!(stop.contains(&running_workspace.hash12));
    assert!(stop.contains(running_workspace.canonical_git_root.as_str()));
    assert!(stop.contains(&transient_workspace.hash12));
    assert!(stop.contains(transient_workspace.canonical_git_root.as_str()));
    assert!(stop.contains("run"));
    assert!(stop.contains(&failed_workspace.hash12));
    assert!(stop.contains(failed_workspace.canonical_git_root.as_str()));
    assert!(stop.contains("failed"));

    let health = harness
        .agentbox_command()
        .args(["__completion-roots", "health"])
        .output()
        .unwrap();
    assert!(health.status.success());
    assert!(health.stderr.is_empty());
    let health = String::from_utf8(health.stdout).unwrap();
    assert_eq!(first_candidate_value(&health), running_workspace.hash12);
    assert!(health.contains(&running_workspace.hash12));
    assert!(!health.contains(&transient_workspace.hash12));
    assert!(health.contains(&failed_workspace.hash12));
}

#[test]
fn zsh_completion_script_wires_the_dynamic_callback_and_descriptions() {
    let script = capture_completion_script();

    assert!(script.contains("__completion-roots"));
    assert!(script.contains("compdef _agentbox agentbox"));
    assert!(script.contains("compadd -d descriptions -- \"${candidates[@]}\""));
    assert!(script.contains("runtime status"));
}

#[test]
fn fish_completion_script_keeps_helper_metadata_available() {
    let script = capture_completion_script_shell("fish");

    assert!(script.contains("agentbox __completion-roots $command 2>/dev/null"));
    assert!(script.contains("__fish_seen_subcommand_from connect"));
    assert!(script.contains("__fish_seen_subcommand_from health"));
    assert!(script.contains("__fish_seen_subcommand_from stop"));
    assert!(script.contains("(__agentbox_completion_roots connect)"));
    assert!(script.contains("(__agentbox_completion_roots health)"));
    assert!(script.contains("(__agentbox_completion_roots stop)"));
}

#[test]
fn installed_completion_script_uses_live_roots_for_directory_commands() {
    let script = capture_installed_completion_script("bash");

    assert!(script.contains("_agentbox()"));
    assert!(script.contains("run exec start runtime connect ls health stop clean completion help"));
    assert!(script.contains("__completion-roots"));
    assert!(script.contains("complete -F _agentbox agentbox"));
    assert!(!script.contains("__generate-completion"));
    assert!(!script.contains("__generate-man"));
    assert!(!script.contains("__generate-manpages"));
}

#[test]
fn completion_scripts_offer_ls_and_health_output_formats() {
    let output_values = OutputFormat::supported_values().join(" ");

    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("ls)"));
    assert!(bash.contains("health)"));
    assert!(bash.contains("--output -o"));
    assert!(bash.contains(&output_values));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("ls)"));
    assert!(zsh.contains("health)"));
    assert!(zsh.contains("--output[select output format]"));
    assert!(zsh.contains("-o[select output format]"));
    assert!(zsh.contains(&output_values));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__fish_seen_subcommand_from ls"));
    assert!(fish.contains("__fish_seen_subcommand_from health"));
    assert!(fish.contains("-s o -l output"));
    assert!(fish.contains(&output_values));
}

#[test]
fn completion_scripts_offer_run_and_start_flags() {
    let dev_env_values = DevEnvMode::supported_values().join(" ");

    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("--runtime --dev-env"));
    assert!(bash.contains("--runtime --dev-env --connect -c"));
    assert!(bash.contains(&dev_env_values));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("--dev-env[select development environment loading mode]"));
    assert!(zsh.contains(&dev_env_values));
    assert!(zsh.contains("--connect[connect after the new session is ready]"));
    assert!(zsh.contains("-c[connect after the new session is ready]"));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__fish_seen_subcommand_from run"));
    assert!(fish.contains("-l dev-env"));
    assert!(fish.contains(&dev_env_values));
    assert!(!fish.contains("__fish_seen_subcommand_from run\" -s c -l connect"));
    assert!(fish.contains("__fish_seen_subcommand_from start"));
    assert!(fish.contains("__fish_seen_subcommand_from start\" -s c -l connect"));
}

#[test]
fn completion_scripts_offer_exec_dev_env_flag_only() {
    let dev_env_values = DevEnvMode::supported_values().join(" ");

    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("exec)"));
    assert!(bash.contains("compgen -W \"--dev-env\""));
    assert!(bash.contains(&dev_env_values));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("exec)"));
    assert!(zsh.contains("--dev-env[select development environment loading mode]"));
    assert!(zsh.contains(&dev_env_values));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__fish_seen_subcommand_from exec"));
    assert!(fish.contains("__fish_seen_subcommand_from exec\" -l dev-env"));
    assert!(!fish.contains("__fish_seen_subcommand_from exec\" -l runtime"));
}

#[test]
fn completion_scripts_offer_stop_all() {
    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("--force --all"));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("--all[stop every running managed session]"));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__fish_seen_subcommand_from stop"));
    assert!(fish.contains("-l all"));
}

#[test]
fn completion_scripts_offer_stop_candidates_at_every_target_position() {
    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("COMP_WORDS[*]:2:COMP_CWORD-2"));
    assert!(bash.contains("_agentbox_completion_roots stop"));
    assert!(!bash.contains("\"$COMP_CWORD\" -eq 3 && \"${COMP_WORDS[2]}\" == \"--force\""));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("CURRENT >= 3"));
    assert!(zsh.contains("stop_words_before"));
    assert!(zsh.contains("_agentbox_completion_roots stop"));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__agentbox_stop_all_seen"));
    assert!(fish.contains("and not __agentbox_stop_all_seen"));
}

#[test]
fn completion_scripts_offer_clean_flags() {
    let bash = capture_completion_script_shell("bash");
    assert!(bash.contains("clean)"));
    assert!(bash.contains("--dry-run --yes --images --volumes"));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("clean)"));
    assert!(zsh.contains("--dry-run[print cleanup candidates without deleting]"));
    assert!(zsh.contains("--volumes[consider workspace cache volumes]"));

    let fish = capture_completion_script_shell("fish");
    assert!(fish.contains("__fish_seen_subcommand_from clean"));
    assert!(fish.contains("-l dry-run"));
    assert!(fish.contains("-l volumes"));
}

#[test]
fn completion_scripts_expand_shared_value_placeholders() {
    let runtime_values = RuntimeKind::supported_values().join(" ");
    let dev_env_values = DevEnvMode::supported_values().join(" ");
    let output_values = OutputFormat::supported_values().join(" ");
    let shell_values = CompletionShell::supported_values().join(" ");

    for shell in CompletionShell::supported_values() {
        let script = capture_completion_script_shell(shell);

        for placeholder in [
            "@RUNTIME_VALUES@",
            "@DEV_ENV_VALUES@",
            "@OUTPUT_VALUES@",
            "@SHELL_VALUES@",
            "@SUBCOMMAND_NAMES@",
            "@ZSH_SUBCOMMAND_SPECS@",
        ] {
            assert!(
                !script.contains(placeholder),
                "{shell} completion still contains {placeholder}"
            );
        }

        assert!(script.contains(&runtime_values));
        assert!(script.contains(&dev_env_values));
        assert!(script.contains(&output_values));
        assert!(script.contains(&shell_values));
    }
}

#[test]
fn installed_manpage_uses_clap_model_without_internal_helpers() {
    let manpage = capture_installed_manpage();

    assert!(manpage.contains(".TH agentbox 1"));
    assert!(manpage.contains("agentbox\\-run(1)"));
    assert!(manpage.contains("agentbox\\-exec(1)"));
    assert!(manpage.contains("agentbox\\-start(1)"));
    assert!(manpage.contains("agentbox\\-health(1)"));
    assert!(manpage.contains("agentbox\\-clean(1)"));
    assert!(!manpage.contains("agentbox\\-help(1)"));
    assert!(manpage.contains("Shell completion helpers"));
    assert!(!manpage.contains("__completion-roots"));
    assert!(!manpage.contains("__generate-completion"));
    assert!(!manpage.contains("__generate-man"));
    assert!(!manpage.contains("__generate-manpages"));
}

#[test]
fn installed_manpages_include_referenced_subcommands() {
    let directory = tempfile::tempdir().unwrap();
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-manpages")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    for filename in [
        "agentbox.1",
        "agentbox-run.1",
        "agentbox-exec.1",
        "agentbox-start.1",
        "agentbox-runtime.1",
        "agentbox-connect.1",
        "agentbox-ls.1",
        "agentbox-health.1",
        "agentbox-stop.1",
        "agentbox-clean.1",
        "agentbox-completion.1",
    ] {
        assert!(
            directory.path().join(filename).is_file(),
            "missing generated manpage {filename}"
        );
    }
    assert!(!directory.path().join("agentbox-help.1").exists());

    let agentbox = fs::read_to_string(directory.path().join("agentbox.1")).unwrap();
    assert!(agentbox.contains("agentbox\\-run(1)"));
    assert!(agentbox.contains("agentbox\\-exec(1)"));
    assert!(agentbox.contains("agentbox\\-start(1)"));
    assert!(agentbox.contains("agentbox\\-health(1)"));
    assert!(agentbox.contains("agentbox\\-clean(1)"));
    assert!(!agentbox.contains("agentbox\\-help(1)"));

    let run = fs::read_to_string(directory.path().join("agentbox-run.1")).unwrap();
    assert!(run.contains(".TH agentbox-run 1"));
    assert!(run.contains("Runtime to launch for this run"));
    assert!(!run.contains("Connect after the new session is ready"));
    let exec = fs::read_to_string(directory.path().join("agentbox-exec.1")).unwrap();
    assert!(exec.contains(".TH agentbox-exec 1"));
    assert!(exec.contains("Arguments passed to codex exec"));
    assert!(exec.contains("Development environment loading mode"));
    assert!(!exec.contains("Runtime to launch"));
    let start = fs::read_to_string(directory.path().join("agentbox-start.1")).unwrap();
    assert!(start.contains(".TH agentbox-start 1"));
    assert!(start.contains("Runtime to launch for this session"));
    assert!(start.contains("Connect after the new session is ready"));
}

fn capture_completion_script() -> String {
    capture_completion_script_shell("zsh")
}

fn capture_completion_script_shell(shell: &str) -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("completion")
        .arg(shell)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn capture_installed_completion_script(shell: &str) -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-completion")
        .arg(shell)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn capture_installed_manpage() -> String {
    let output = AssertCommand::cargo_bin("agentbox")
        .unwrap()
        .arg("__generate-man")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn first_candidate_value(output: &str) -> &str {
    output.lines().next().unwrap().split('\t').next().unwrap()
}

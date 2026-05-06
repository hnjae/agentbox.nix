// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use assert_cmd::Command as AssertCommand;

use agentbox::cli::{CompletionShell, OutputFormat};
use agentbox::metadata::LABEL_ATTACH_SCHEME;
use agentbox::runtime::RuntimeKind;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as LiveHarness, managed_inspect_fixture, opencode_workspace_inspect_fixture,
    opencode_workspace_labels, ps_fixture, workspace_ps_entry,
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
        .args(["__completion-roots", "attach"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();

    assert!(output.contains(workspace.canonical_git_root.as_str()));
    assert!(output.contains("opencode"));
}

#[test]
fn helper_filters_attach_and_stop_candidates_by_command() {
    let running_fixture = support::temp_workspace("running");
    let failed_fixture = support::temp_workspace("failed");
    let running_workspace = &running_fixture.workspace;
    let failed_workspace = &failed_fixture.workspace;
    let harness = LiveHarness::new();

    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("running-id", running_workspace),
        workspace_ps_entry("failed-id", failed_workspace),
    ]));
    harness.write_inspect(
        "running-id",
        &opencode_workspace_inspect_fixture(running_workspace, true, true),
    );
    let mut failed_labels = opencode_workspace_labels(failed_workspace);
    failed_labels.remove(LABEL_ATTACH_SCHEME);
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

    let attach = harness
        .agentbox_command()
        .args(["__completion-roots", "attach"])
        .output()
        .unwrap();
    assert!(attach.status.success());
    let attach = String::from_utf8(attach.stdout).unwrap();
    assert!(attach.contains(running_workspace.canonical_git_root.as_str()));
    assert!(!attach.contains(failed_workspace.canonical_git_root.as_str()));

    let stop = harness
        .agentbox_command()
        .args(["__completion-roots", "stop"])
        .output()
        .unwrap();
    assert!(stop.status.success());
    let stop = String::from_utf8(stop.stdout).unwrap();
    assert!(stop.contains(running_workspace.canonical_git_root.as_str()));
    assert!(stop.contains(failed_workspace.canonical_git_root.as_str()));
    assert!(stop.contains("failed"));
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
    assert!(script.contains("__fish_seen_subcommand_from attach"));
    assert!(script.contains("__fish_seen_subcommand_from stop"));
    assert!(script.contains("(__agentbox_completion_roots attach)"));
    assert!(script.contains("(__agentbox_completion_roots stop)"));
}

#[test]
fn installed_completion_script_uses_live_roots_for_directory_commands() {
    let script = capture_installed_completion_script("bash");

    assert!(script.contains("_agentbox()"));
    assert!(script.contains("run runtime attach ls health stop completion help"));
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
    assert!(bash.contains("ls|health"));
    assert!(bash.contains("--output -o"));
    assert!(bash.contains(&output_values));

    let zsh = capture_completion_script_shell("zsh");
    assert!(zsh.contains("ls|health"));
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
fn completion_scripts_expand_shared_value_placeholders() {
    let runtime_values = RuntimeKind::supported_values().join(" ");
    let output_values = OutputFormat::supported_values().join(" ");
    let shell_values = CompletionShell::supported_values().join(" ");

    for shell in CompletionShell::supported_values() {
        let script = capture_completion_script_shell(shell);

        for placeholder in [
            "@RUNTIME_VALUES@",
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
        assert!(script.contains(&output_values));
        assert!(script.contains(&shell_values));
    }
}

#[test]
fn installed_manpage_uses_clap_model_without_internal_helpers() {
    let manpage = capture_installed_manpage();

    assert!(manpage.contains(".TH agentbox 1"));
    assert!(manpage.contains("agentbox\\-run(1)"));
    assert!(manpage.contains("agentbox\\-health(1)"));
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
        "agentbox-runtime.1",
        "agentbox-attach.1",
        "agentbox-ls.1",
        "agentbox-health.1",
        "agentbox-stop.1",
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
    assert!(agentbox.contains("agentbox\\-health(1)"));
    assert!(!agentbox.contains("agentbox\\-help(1)"));

    let run = fs::read_to_string(directory.path().join("agentbox-run.1")).unwrap();
    assert!(run.contains(".TH agentbox-run 1"));
    assert!(run.contains("Runtime to launch for this run"));
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

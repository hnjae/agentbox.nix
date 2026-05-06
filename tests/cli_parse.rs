// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::cli::{
    Cli, Command, CompletionArgs, CompletionShell, DirectoryArgs, HealthArgs, LsArgs, OutputFormat,
    RunArgs, RuntimeArgs, RuntimeCommand, RuntimeUpdateArgs, StopArgs,
};
use agentbox::runtime::RuntimeKind;
use assert_cmd::Command as AssertCommand;
use clap::Parser;
use predicates::prelude::*;

#[test]
fn help_lists_core_commands() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("run")
            .and(predicate::str::contains("runtime"))
            .and(predicate::str::contains("attach"))
            .and(predicate::str::contains("ls"))
            .and(predicate::str::contains("health"))
            .and(predicate::str::contains("stop"))
            .and(predicate::str::contains("completion"))
            .and(predicate::str::contains("detached runtime server")),
    );
}

#[test]
fn unknown_subcommand_fails() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.arg("bogus");

    command
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand 'bogus'"));
}

#[test]
fn core_commands_parse_into_expected_variants() {
    let run = Cli::try_parse_from(["agentbox", "run", "--runtime", "opencode", "/tmp/workspace"])
        .unwrap();
    let attach = Cli::try_parse_from(["agentbox", "attach", "/tmp/workspace"]).unwrap();
    let ls = Cli::try_parse_from(["agentbox", "ls"]).unwrap();
    let health = Cli::try_parse_from(["agentbox", "health"]).unwrap();
    let stop = Cli::try_parse_from(["agentbox", "stop", "/tmp/workspace"]).unwrap();
    let runtime = Cli::try_parse_from(["agentbox", "runtime", "update", "codex"]).unwrap();
    let completion = Cli::try_parse_from(["agentbox", "completion", "bash"]).unwrap();

    assert_eq!(
        run.command,
        Command::Run(RunArgs {
            runtime: RuntimeKind::Opencode,
            directory: "/tmp/workspace".into(),
        })
    );
    assert_eq!(
        attach.command,
        Command::Attach(DirectoryArgs {
            directory: "/tmp/workspace".into(),
        })
    );
    assert_eq!(
        ls.command,
        Command::Ls(LsArgs {
            output: OutputFormat::Table,
        })
    );
    assert_eq!(
        health.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Table,
            target: None,
        })
    );
    assert_eq!(
        stop.command,
        Command::Stop(StopArgs {
            force: false,
            target: "/tmp/workspace".into(),
        })
    );
    assert_eq!(
        runtime.command,
        Command::Runtime(RuntimeArgs {
            command: RuntimeCommand::Update(RuntimeUpdateArgs {
                runtime: RuntimeKind::Codex,
            }),
        })
    );
    assert_eq!(
        completion.command,
        Command::Completion(CompletionArgs {
            shell: CompletionShell::Bash,
        })
    );
}

#[test]
fn global_verbose_flag_is_available_before_or_after_subcommands() {
    let before = Cli::try_parse_from([
        "agentbox",
        "--verbose",
        "run",
        "--runtime",
        "opencode",
        "/tmp/workspace",
    ])
    .unwrap();
    let after = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--verbose",
        "/tmp/workspace",
    ])
    .unwrap();

    assert!(before.verbose);
    assert!(after.verbose);
}

#[test]
fn stop_accepts_force_cleanup_flag() {
    let cli = Cli::try_parse_from(["agentbox", "stop", "--force", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Stop(StopArgs {
            force: true,
            target: "/tmp/workspace".into(),
        })
    );
}

#[test]
fn ls_accepts_output_format_selection() {
    for args in [
        vec!["agentbox", "ls", "--output", "json"],
        vec!["agentbox", "ls", "--output=json"],
        vec!["agentbox", "ls", "-o", "json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(
            cli.command,
            Command::Ls(LsArgs {
                output: OutputFormat::Json,
            })
        );
    }

    let cli = Cli::try_parse_from(["agentbox", "ls", "--output", "table"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Ls(LsArgs {
            output: OutputFormat::Table,
        })
    );
}

#[test]
fn health_accepts_output_format_selection() {
    for args in [
        vec!["agentbox", "health", "--output", "json"],
        vec!["agentbox", "health", "--output=json"],
        vec!["agentbox", "health", "-o", "json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(
            cli.command,
            Command::Health(HealthArgs {
                output: OutputFormat::Json,
                target: None,
            })
        );
    }

    let cli = Cli::try_parse_from(["agentbox", "health", "--output", "table"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Table,
            target: None,
        })
    );
}

#[test]
fn health_accepts_stable_id_target_with_output_selection() {
    let cli = Cli::try_parse_from(["agentbox", "health", "--output", "json", "abc123"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Json,
            target: Some("abc123".to_string()),
        })
    );
}

#[test]
fn output_format_rejects_unknown_values() {
    for args in [
        vec!["agentbox", "ls", "--output", "yaml"],
        vec!["agentbox", "health", "-o", "yaml"],
    ] {
        let error = Cli::try_parse_from(args).unwrap_err();

        assert_eq!(error.exit_code(), 2);
        assert!(
            error.to_string().contains("invalid value 'yaml'"),
            "expected clap to reject the unsupported output format"
        );
    }
}

#[test]
fn directory_commands_require_a_path_argument() {
    let error = Cli::try_parse_from(["agentbox", "run", "--runtime", "opencode"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("<DIRECTORY>"),
        "expected clap to mention the missing directory argument"
    );
}

#[test]
fn run_rejects_image_override() {
    let error = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--image",
        "registry.example/agentbox:test",
        "/tmp/workspace",
    ])
    .unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("unexpected argument '--image'"),
        "expected clap to reject the removed image option"
    );
}

#[test]
fn run_requires_runtime_selection() {
    let error = Cli::try_parse_from(["agentbox", "run", "/tmp/workspace"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("--runtime <RUNTIME>"),
        "expected clap to mention the missing runtime option"
    );
}

#[test]
fn run_accepts_runtime_selection() {
    let cli =
        Cli::try_parse_from(["agentbox", "run", "--runtime", "codex", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Run(RunArgs {
            runtime: RuntimeKind::Codex,
            directory: "/tmp/workspace".into(),
        })
    );
}

#[test]
fn attach_rejects_runtime_and_image_flags() {
    let runtime_error =
        Cli::try_parse_from(["agentbox", "attach", "--runtime", "codex", "/tmp/workspace"])
            .unwrap_err();
    assert_eq!(runtime_error.exit_code(), 2);
    assert!(
        runtime_error
            .to_string()
            .contains("unexpected argument '--runtime'")
    );

    let image_error = Cli::try_parse_from([
        "agentbox",
        "attach",
        "--image",
        "example:test",
        "/tmp/workspace",
    ])
    .unwrap_err();
    assert_eq!(image_error.exit_code(), 2);
    assert!(
        image_error
            .to_string()
            .contains("unexpected argument '--image'")
    );
}

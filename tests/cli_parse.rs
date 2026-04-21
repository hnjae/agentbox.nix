// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::cli::{Cli, Command, DirectoryArgs};
use assert_cmd::Command as AssertCommand;
use clap::Parser;
use predicates::prelude::*;

#[test]
fn help_lists_core_commands() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("run")
            .and(predicate::str::contains("attach"))
            .and(predicate::str::contains("ls"))
            .and(predicate::str::contains("rm"))
            .and(predicate::str::contains("completion")),
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
    let run = Cli::try_parse_from(["agentbox", "run", "/tmp/workspace"]).unwrap();
    let attach = Cli::try_parse_from(["agentbox", "attach", "/tmp/workspace"]).unwrap();
    let ls = Cli::try_parse_from(["agentbox", "ls"]).unwrap();
    let rm = Cli::try_parse_from(["agentbox", "rm", "/tmp/workspace"]).unwrap();
    let completion = Cli::try_parse_from(["agentbox", "completion"]).unwrap();

    assert_eq!(
        run.command,
        Command::Run(DirectoryArgs {
            directory: "/tmp/workspace".into(),
        })
    );
    assert_eq!(
        attach.command,
        Command::Attach(DirectoryArgs {
            directory: "/tmp/workspace".into(),
        })
    );
    assert_eq!(ls.command, Command::Ls);
    assert_eq!(
        rm.command,
        Command::Rm(DirectoryArgs {
            directory: "/tmp/workspace".into(),
        })
    );
    assert_eq!(completion.command, Command::Completion);
}

#[test]
fn directory_commands_require_a_path_argument() {
    let error = Cli::try_parse_from(["agentbox", "run"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("<DIRECTORY>"),
        "expected clap to mention the missing directory argument"
    );
    assert!(
        error
            .to_string()
            .contains("Usage: agentbox run <DIRECTORY>"),
        "expected clap to show the command usage"
    );
}

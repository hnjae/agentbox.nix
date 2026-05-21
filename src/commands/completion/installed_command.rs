// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::CommandFactory;

use crate::cli::Cli;

pub(super) fn command() -> clap::Command {
    let command = Cli::command();
    let mut installed = clap::Command::new("agentbox")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage agentbox sessions")
        .disable_help_subcommand(true)
        .subcommand_required(true);

    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        installed = installed.subcommand(subcommand.clone());
    }

    installed
}

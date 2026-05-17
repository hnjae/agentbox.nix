// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::cli::Command;
use crate::error::Result;

pub mod clean;
pub mod completion;
pub mod connect;
mod container_cleanup;
mod container_launch;
mod detached_server;
pub mod exec;
pub mod health;
mod launch_policy;
pub mod ls;
mod managed_server;
mod output;
pub mod restart;
pub mod run;
pub mod runtime;
mod runtime_command;
mod server_readiness;
mod session_output;
mod session_targets;
pub mod start;
pub mod stop;
mod transient_run;
mod workspace_flow;

pub use output::OutputFormat;

pub fn dispatch(command: Command, verbose: bool) -> Result<()> {
    tracing::debug!(?command, "dispatching CLI command");

    match command {
        Command::Run(args) => run::run(args, verbose),
        Command::Exec(args) => exec::run(args, verbose),
        Command::Start(args) => start::run(args, verbose),
        Command::Restart(args) => restart::run(args, verbose),
        Command::Runtime(args) => runtime::run(args, verbose),
        Command::Connect(args) => connect::run(args),
        Command::Ls(args) => ls::run(args),
        Command::Health(args) => health::run(args),
        Command::Stop(args) => stop::run(args),
        Command::Clean(args) => clean::run(args),
        Command::Completion(args) => completion::run(args.shell),
        Command::CompletionRoots(args) => {
            print!("{}", completion::live_roots_output(args.command)?);
            Ok(())
        }
        Command::GenerateCompletion(args) => completion::generate_installed(args.shell),
        Command::GenerateMan => completion::generate_manpage(),
        Command::GenerateManpages(args) => completion::generate_manpages(&args.directory),
    }
}

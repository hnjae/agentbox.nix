// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::Command;
use crate::error::Result;

pub mod clean;
pub mod completion;
pub mod connect;
mod container_cleanup;
pub mod health;
pub mod ls;
mod output;
pub mod run;
pub mod runtime;
mod runtime_command;
mod server_readiness;
mod session_output;
mod session_targets;
pub mod start;
pub mod stop;
mod workspace_flow;

pub fn dispatch(command: Command, verbose: bool) -> Result<()> {
    tracing::debug!(?command, "dispatching CLI command");

    match command {
        Command::Run(args) => run::run(args, verbose),
        Command::Start(args) => start::run(args, verbose),
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

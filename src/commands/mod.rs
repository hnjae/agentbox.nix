// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::Command;
use crate::error::{Error, Result};

pub mod attach;
pub mod ls;
pub mod run;

pub fn dispatch(command: Command) -> Result<()> {
    tracing::debug!(?command, "dispatching CLI command");

    match command {
        Command::Run(args) => run::run(args),
        Command::Attach(args) => attach::run(args),
        Command::Ls => ls::run(),
        Command::Rm(_) => Err(Error::not_yet_implemented("rm")),
        Command::Completion => Err(Error::not_yet_implemented("completion")),
    }
}

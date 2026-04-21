// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

pub mod cli;
pub mod commands;
pub mod error;
pub mod lock;
pub mod workspace;

use cli::Cli;
pub use error::{Error, Result};
pub use lock::{WorkspaceLock, WorkspaceLockGuard, lock_path_for_digest, lock_workspace};

pub fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Cli(error)) => error.exit(),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

pub fn try_main() -> Result<()> {
    run(std::env::args_os())
}

pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    commands::dispatch(cli.command)
}

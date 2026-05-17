#![allow(clippy::multiple_crate_versions)]

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

pub mod cli;
pub mod commands;
pub mod dev_env;
pub mod diagnostic;
mod digest;
pub mod error;
pub mod git;
pub mod lock;
pub mod metadata;
mod paths;
pub mod podman;
pub mod preflight;
pub mod process;
pub mod prompt;
pub mod runtime;
pub mod session;
mod ssh_signing;
mod state;
pub mod workspace;

use cli::Cli;
pub use error::{Error, Result};
pub use lock::{WorkspaceLock, WorkspaceLockGuard, lock_path_for_digest, lock_workspace};

pub fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Cli(error)) => error.exit(),
        Err(Error::ExitCode(code)) => ExitCode::from(code),
        Err(Error::ExitCodeWithMessage { code, message }) => {
            diagnostic::error(message);
            ExitCode::from(code)
        }
        Err(error) => {
            diagnostic::error(error.to_string());
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
    commands::dispatch(cli.command, cli.verbose)
}

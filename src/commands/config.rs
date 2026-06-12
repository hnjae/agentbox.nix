// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::{Args, Subcommand};

use crate::config;
use crate::{Error, Result, diagnostic};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Write the default config file.
    Init(ConfigInitArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ConfigInitArgs {
    /// Overwrite an existing config file.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Init(args) => init(args),
    }
}

fn init(args: ConfigInitArgs) -> Result<()> {
    match config::write_default_config(args.force) {
        Ok(path) => {
            diagnostic::info(format!("wrote agentbox config `{}`", path.display()));
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(Error::msg(format!(
                "agentbox config `{}` already exists; rerun `agentbox config init --force` to overwrite it",
                config::config_file_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "config.json".to_string())
            )))
        }
        Err(error) => Err(error.into()),
    }
}

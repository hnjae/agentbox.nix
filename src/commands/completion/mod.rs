// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};

use crate::error::Result;

mod installed_command;
mod live_roots;
mod manpage;
mod scripts;

pub use live_roots::live_roots_output;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CompletionArgs {
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CompletionRootsArgs {
    pub command: CompletionRootCommand,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct GenerateManpagesArgs {
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    fn variants() -> &'static [Self] {
        <Self as ValueEnum>::value_variants()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
        }
    }

    pub fn supported_values() -> Vec<&'static str> {
        Self::variants()
            .iter()
            .map(|shell| shell.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionRootCommand {
    Connect,
    Health,
    Restart,
    Stop,
}

pub fn run(shell: CompletionShell) -> Result<()> {
    print!("{}", scripts::render(shell));

    Ok(())
}

pub fn generate_installed(shell: CompletionShell) -> Result<()> {
    run(shell)
}

pub fn generate_manpage() -> Result<()> {
    manpage::generate_stdout()
}

pub fn generate_manpages(directory: &Path) -> Result<()> {
    manpage::generate_all_to(directory)
}

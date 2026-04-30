// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::runtime::RuntimeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl std::str::FromStr for CompletionShell {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            other => Err(format!("unsupported shell `{other}`")),
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "agentbox")]
#[command(version)]
#[command(about = "Manage agentbox sessions", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Run a managed session in the foreground.
    Run(RunArgs),
    /// Attach to a running managed session.
    Attach(DirectoryArgs),
    /// List managed sessions.
    Ls,
    /// Stop a managed session.
    Stop(StopArgs),
    /// Shell completion helpers.
    Completion(CompletionArgs),

    #[command(name = "__completion-roots", hide = true)]
    CompletionRoots,
    #[command(name = "__generate-completion", hide = true)]
    GenerateCompletion(CompletionArgs),
    #[command(name = "__generate-man", hide = true)]
    GenerateMan,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CompletionArgs {
    pub shell: CompletionShell,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Runtime to launch for this run.
    #[arg(long, value_enum, required = true)]
    pub runtime: RuntimeKind,

    /// Runtime image to use for this run.
    #[arg(long)]
    pub image: Option<String>,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DirectoryArgs {
    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct StopArgs {
    /// Clean up all duplicate exact matches instead of failing.
    #[arg(long)]
    pub force: bool,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

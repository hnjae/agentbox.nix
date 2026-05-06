// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{fmt, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::runtime::RuntimeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

impl CompletionShell {
    fn variants() -> &'static [Self] {
        &[Self::Bash, Self::Zsh, Self::Fish]
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

impl std::str::FromStr for CompletionShell {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::variants()
            .iter()
            .copied()
            .find(|shell| shell.as_str() == value)
            .ok_or_else(|| format!("unsupported shell `{value}`"))
    }
}

#[derive(Debug, Parser)]
#[command(name = "agentbox")]
#[command(version)]
#[command(about = "Manage agentbox sessions", long_about = None)]
pub struct Cli {
    /// Print diagnostic progress and command details to stderr.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Run a managed session as a detached runtime server.
    Run(RunArgs),
    /// Manage default runtime images.
    Runtime(RuntimeArgs),
    /// Attach to a running managed session.
    Attach(DirectoryArgs),
    /// List managed sessions.
    Ls(LsArgs),
    /// Check running managed session runtime health.
    Health(HealthArgs),
    /// Stop a managed session.
    Stop(StopArgs),
    /// Remove unused agentbox-owned Podman resources.
    Clean(CleanArgs),
    /// Shell completion helpers.
    Completion(CompletionArgs),

    #[command(name = "__completion-roots", hide = true)]
    CompletionRoots(CompletionRootsArgs),
    #[command(name = "__generate-completion", hide = true)]
    GenerateCompletion(CompletionArgs),
    #[command(name = "__generate-man", hide = true)]
    GenerateMan,
    #[command(name = "__generate-manpages", hide = true)]
    GenerateManpages(GenerateManpagesArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CompletionArgs {
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
pub enum CompletionRootCommand {
    Attach,
    Health,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

impl OutputFormat {
    fn variants() -> &'static [Self] {
        &[Self::Table, Self::Json]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Json => "json",
        }
    }

    pub fn supported_values() -> Vec<&'static str> {
        Self::variants()
            .iter()
            .map(|format| format.as_str())
            .collect()
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Runtime to launch for this run.
    #[arg(long, value_enum, required = true)]
    pub runtime: RuntimeKind,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RuntimeArgs {
    #[command(subcommand)]
    pub command: RuntimeCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum RuntimeCommand {
    /// Update a default runtime image.
    Update(RuntimeUpdateArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RuntimeUpdateArgs {
    /// Runtime image to update.
    #[arg(value_enum)]
    pub runtime: RuntimeKind,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct DirectoryArgs {
    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct LsArgs {
    /// Output format.
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct HealthArgs {
    /// Output format.
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    /// Stable session id prefix to probe.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct StopArgs {
    /// Stop every running managed session.
    #[arg(long, conflicts_with = "targets")]
    pub all: bool,

    /// Clean up duplicate or failed exact matches instead of failing.
    #[arg(long)]
    pub force: bool,

    /// Workspace directory, exact orphan path, or stable session id prefix.
    #[arg(value_name = "TARGET")]
    pub targets: Vec<PathBuf>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CleanArgs {
    /// Print cleanup candidates without deleting anything.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,

    /// Delete cleanup candidates without prompting.
    #[arg(long)]
    pub yes: bool,

    /// Consider unused default runtime images.
    #[arg(long)]
    pub images: bool,

    /// Consider unused workspace cache volumes.
    #[arg(long)]
    pub volumes: bool,
}

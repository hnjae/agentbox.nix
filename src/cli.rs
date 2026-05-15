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
    /// Run a runtime container in the foreground.
    Run(RunArgs),
    /// Run Codex exec in a foreground container.
    Exec(ExecArgs),
    /// Start a managed session as a detached runtime server.
    Start(StartArgs),
    /// Manage default runtime images.
    Runtime(RuntimeArgs),
    /// Connect to a running managed session.
    Connect(ConnectArgs),
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
pub enum CompletionRootCommand {
    Connect,
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
        <Self as ValueEnum>::value_variants()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum DevEnvMode {
    Auto,
    None,
}

impl DevEnvMode {
    fn variants() -> &'static [Self] {
        <Self as ValueEnum>::value_variants()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }

    pub fn supported_values() -> Vec<&'static str> {
        Self::variants().iter().map(|mode| mode.as_str()).collect()
    }
}

impl fmt::Display for DevEnvMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Runtime to launch for this run.
    #[arg(long, value_enum)]
    pub runtime: Option<RuntimeKind>,

    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ExecArgs {
    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,

    /// Arguments passed to codex exec.
    #[arg(value_name = "CODEX_EXEC_ARG", last = true)]
    pub codex_args: Vec<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct StartArgs {
    /// Runtime to launch for this session.
    #[arg(long, value_enum)]
    pub runtime: Option<RuntimeKind>,

    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Connect after the new session is ready.
    #[arg(short = 'c', long = "connect")]
    pub connect: bool,

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
pub struct ConnectArgs {
    /// Workspace directory inside a git repository.
    pub directory: Option<PathBuf>,
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

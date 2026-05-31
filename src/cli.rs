// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::{Parser, Subcommand};

pub use crate::commands::OutputFormat;
pub use crate::commands::clean::CleanArgs;
pub use crate::commands::completion::{
    CompletionArgs, CompletionRootCommand, CompletionRootsArgs, CompletionShell,
    GenerateManpagesArgs,
};
pub use crate::commands::connect::ConnectArgs;
pub use crate::commands::exec::ExecArgs;
pub use crate::commands::health::HealthArgs;
pub use crate::commands::ps::PsArgs;
pub use crate::commands::restart::RestartArgs;
pub use crate::commands::run::RunArgs;
pub use crate::commands::runtime::{RuntimeArgs, RuntimeCommand, RuntimeUpdateArgs};
pub use crate::commands::start::StartArgs;
pub use crate::commands::stop::StopArgs;
pub use crate::dev_env::DevEnvMode;

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
    /// Run a transient runtime server and host client.
    Run(RunArgs),
    /// Run Codex exec in a foreground container.
    Exec(ExecArgs),
    /// Start a managed session as a detached runtime server.
    Start(StartArgs),
    /// Restart a running managed session.
    Restart(RestartArgs),
    /// Manage default runtime images.
    Runtime(RuntimeArgs),
    /// Connect to a running managed session.
    Connect(ConnectArgs),
    /// Show agentbox container status.
    Ps(PsArgs),
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

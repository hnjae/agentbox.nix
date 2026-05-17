// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::CommandFactory;
use std::path::Path;

use crate::cli::{Cli, CompletionRootCommand, CompletionShell, DevEnvMode, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::runtime::RuntimeKind;
use crate::session::{SessionDiscoveryQuery, SessionRecord, SessionTargetKind};

use super::session_targets::completion_line;

pub fn run(shell: CompletionShell) -> Result<()> {
    match shell {
        CompletionShell::Bash => print!("{}", bash_script()),
        CompletionShell::Zsh => print!("{}", zsh_script()),
        CompletionShell::Fish => print!("{}", fish_script()),
    }

    Ok(())
}

pub fn generate_installed(shell: CompletionShell) -> Result<()> {
    run(shell)
}

pub fn generate_manpage() -> Result<()> {
    let command = installed_asset_command();
    let mut stdout = std::io::stdout().lock();

    clap_mangen::Man::new(command).render(&mut stdout)?;
    Ok(())
}

pub fn generate_manpages(directory: &Path) -> Result<()> {
    let command = installed_asset_command();

    clap_mangen::generate_to(command, directory)?;
    Ok(())
}

pub fn live_roots(command: CompletionRootCommand) -> Result<Vec<SessionRecord>> {
    let podman = Podman::new();
    let target_kind = completion_target_kind(command);
    let sessions = completion_sessions(&podman, command)?
        .into_iter()
        .filter(|session| target_kind.matches(session))
        .collect();

    Ok(sessions)
}

fn completion_sessions(
    podman: &Podman,
    command: CompletionRootCommand,
) -> Result<Vec<SessionRecord>> {
    match command {
        CompletionRootCommand::Stop => {
            SessionDiscoveryQuery::agentbox_containers().discover(podman)
        }
        CompletionRootCommand::Connect
        | CompletionRootCommand::Health
        | CompletionRootCommand::Restart => {
            SessionDiscoveryQuery::managed_sessions().discover(podman)
        }
    }
}

pub fn live_roots_output(command: CompletionRootCommand) -> Result<String> {
    let target_kind = completion_target_kind(command);
    let sessions = live_roots(command)?;
    let lines = target_kind
        .candidates(&sessions)
        .map(completion_line)
        .collect::<Vec<_>>();

    Ok(lines.join("\n"))
}

fn completion_target_kind(command: CompletionRootCommand) -> SessionTargetKind {
    match command {
        CompletionRootCommand::Connect => SessionTargetKind::ConnectRoot,
        CompletionRootCommand::Restart => SessionTargetKind::RestartStableId,
        CompletionRootCommand::Health | CompletionRootCommand::Stop => SessionTargetKind::StableId,
    }
}

fn bash_script() -> String {
    completion_script(include_str!("completion/bash.template"))
}

fn zsh_script() -> String {
    completion_script(include_str!("completion/zsh.template"))
}

fn fish_script() -> String {
    completion_script(include_str!("completion/fish.template"))
}

fn completion_script(template: &str) -> String {
    let subcommands = completion_subcommands();
    let runtime_values = RuntimeKind::supported_values().join(" ");
    let dev_env_values = DevEnvMode::supported_values().join(" ");
    let output_values = OutputFormat::supported_values().join(" ");
    let shell_values = CompletionShell::supported_values().join(" ");
    let subcommand_names = completion_subcommand_names(&subcommands);
    let zsh_subcommand_specs = zsh_subcommand_specs(&subcommands);

    template
        .replace("@RUNTIME_VALUES@", &runtime_values)
        .replace("@DEV_ENV_VALUES@", &dev_env_values)
        .replace("@OUTPUT_VALUES@", &output_values)
        .replace("@SHELL_VALUES@", &shell_values)
        .replace("@SUBCOMMAND_NAMES@", &subcommand_names)
        .replace("@ZSH_SUBCOMMAND_SPECS@", &zsh_subcommand_specs)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletionSubcommand {
    name: String,
    description: String,
}

fn completion_subcommands() -> Vec<CompletionSubcommand> {
    let mut subcommands = installed_asset_command()
        .get_subcommands()
        .map(|command| CompletionSubcommand {
            name: command.get_name().to_string(),
            description: completion_subcommand_description(command),
        })
        .collect::<Vec<_>>();
    subcommands.push(CompletionSubcommand {
        name: "help".to_string(),
        description: "Show help".to_string(),
    });
    subcommands
}

fn completion_subcommand_description(command: &clap::Command) -> String {
    match command.get_name() {
        "run" => "Run a transient runtime server and host client".to_string(),
        "exec" => "Run Codex exec in a foreground container".to_string(),
        "start" => "Start a detached runtime server session".to_string(),
        "restart" => "Restart a running managed session".to_string(),
        "completion" => "Generate shell completion".to_string(),
        _ => command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default(),
    }
}

fn completion_subcommand_names(subcommands: &[CompletionSubcommand]) -> String {
    subcommands
        .iter()
        .map(|subcommand| subcommand.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn zsh_subcommand_specs(subcommands: &[CompletionSubcommand]) -> String {
    subcommands
        .iter()
        .map(|subcommand| {
            zsh_single_quoted(&format!("{}:{}", subcommand.name, subcommand.description))
        })
        .map(|entry| format!("    {entry}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn zsh_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn installed_asset_command() -> clap::Command {
    let command = Cli::command();
    let mut installed = clap::Command::new("agentbox")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage agentbox sessions")
        .disable_help_subcommand(true)
        .subcommand_required(true);

    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        installed = installed.subcommand(subcommand.clone());
    }

    installed
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::dev_env::DevEnvMode;
use crate::runtime::RuntimeKind;

use super::super::output::OutputFormat;
use super::CompletionShell;
use super::installed_command;

pub(super) fn render(shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash => render_template(include_str!("bash.template")),
        CompletionShell::Zsh => render_template(include_str!("zsh.template")),
        CompletionShell::Fish => render_template(include_str!("fish.template")),
    }
}

fn render_template(template: &str) -> String {
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
    let mut subcommands = installed_command::command()
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

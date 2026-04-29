use clap::CommandFactory;

use crate::cli::{Cli, CompletionShell};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions};

pub fn run(shell: CompletionShell) -> Result<()> {
    match shell {
        CompletionShell::Bash => print!("{}", bash_script()),
        CompletionShell::Zsh => print!("{}", zsh_script()),
        CompletionShell::Fish => print!("{}", fish_script()),
    }

    Ok(())
}

pub fn generate_installed(shell: CompletionShell) -> Result<()> {
    let mut command = installed_asset_command();
    let bin_name = command.get_name().to_string();
    let shell = match shell {
        CompletionShell::Bash => clap_complete::Shell::Bash,
        CompletionShell::Zsh => clap_complete::Shell::Zsh,
        CompletionShell::Fish => clap_complete::Shell::Fish,
    };
    let mut stdout = std::io::stdout().lock();

    clap_complete::generate(shell, &mut command, bin_name, &mut stdout);
    Ok(())
}

pub fn generate_manpage() -> Result<()> {
    let command = installed_asset_command();
    let mut stdout = std::io::stdout().lock();

    clap_mangen::Man::new(command).render(&mut stdout)?;
    Ok(())
}

pub fn live_roots() -> Result<Vec<SessionRecord>> {
    let podman = Podman::new();
    discover_managed_sessions(&podman)
}

pub fn live_roots_output() -> Result<String> {
    let mut lines = Vec::new();
    for session in live_roots()? {
        if let Some(root) = session.canonical_git_root {
            lines.push(format!(
                "{}\t{}\t{}\t{}",
                root,
                session.runtime.as_deref().unwrap_or("-"),
                status_label(session.status),
                session.container_name,
            ));
        }
    }
    Ok(lines.join("\n"))
}

fn bash_script() -> String {
    r#"_agentbox_completion_roots() {
    local candidates
    candidates="$({ agentbox __completion-roots 2>/dev/null; } || true)"
    COMPREPLY=( $(compgen -W "$(printf '%s\n' "$candidates" | cut -f1)" -- "${COMP_WORDS[COMP_CWORD]}") )
}
complete -F _agentbox_completion_roots agentbox
"#
    .to_string()
}

fn zsh_script() -> String {
    r#"#compdef agentbox

_agentbox_completion_roots() {
  local line root runtime status container
  local -a candidates descriptions
  for line in ${(f)"$({ agentbox __completion-roots 2>/dev/null; } || true)"}; do
    IFS=$'\t' read -r root runtime status container <<< "$line"
    candidates+=("$root")
    descriptions+=("${runtime} ${status}")
  done
  compadd -d descriptions -- "${candidates[@]}"
}

compdef _agentbox_completion_roots agentbox
"#
    .to_string()
}

fn fish_script() -> String {
    r#"function __agentbox_completion_roots
    agentbox __completion-roots 2>/dev/null
end
complete -c agentbox -f -a "(__agentbox_completion_roots)"
"#
    .to_string()
}

fn status_label(status: crate::session::SessionStatus) -> &'static str {
    match status {
        crate::session::SessionStatus::Running => "running",
        crate::session::SessionStatus::Orphaned => "orphaned",
        crate::session::SessionStatus::Duplicate => "duplicate",
        crate::session::SessionStatus::Failed => "failed",
    }
}

fn installed_asset_command() -> clap::Command {
    let command = Cli::command();
    let mut installed = clap::Command::new("agentbox")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage agentbox sessions")
        .subcommand_required(true);

    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        installed = installed.subcommand(subcommand.clone());
    }

    installed
}

use clap::CommandFactory;
use std::path::Path;

use crate::cli::{Cli, CompletionRootCommand, CompletionShell, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::runtime::RuntimeKind;
use crate::session::{SessionRecord, SessionStatus, discover_managed_sessions};

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
    let sessions = discover_managed_sessions(&podman)?
        .into_iter()
        .filter(|session| completion_candidate_matches(command, session))
        .collect();

    Ok(sessions)
}

pub fn live_roots_output(command: CompletionRootCommand) -> Result<String> {
    let mut lines = Vec::new();
    for session in live_roots(command)? {
        if let Some(root) = session.canonical_git_root() {
            lines.push(format!(
                "{}\t{}\t{}\t{}",
                root,
                session.runtime().unwrap_or("unknown"),
                session.status.as_str(),
                session.container_name,
            ));
        }
    }
    Ok(lines.join("\n"))
}

fn completion_candidate_matches(command: CompletionRootCommand, session: &SessionRecord) -> bool {
    match command {
        CompletionRootCommand::Attach => session.status == SessionStatus::Running,
        CompletionRootCommand::Stop => session.canonical_git_root().is_some(),
    }
}

fn bash_script() -> String {
    completion_script(
        r#"_agentbox_completion_roots() {
    local command
    command="${1:?missing command}"
    local candidates
    candidates="$({ agentbox __completion-roots "$command" 2>/dev/null; } || true)"
    COMPREPLY=( $(compgen -W "$(printf '%s\n' "$candidates" | cut -f1)" -- "${COMP_WORDS[COMP_CWORD]}") )
}

_agentbox() {
    local cur subcommand
    cur="${COMP_WORDS[COMP_CWORD]}"

    if [[ "$COMP_CWORD" -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "@SUBCOMMAND_NAMES@" -- "$cur") )
        return 0
    fi

    subcommand="${COMP_WORDS[1]}"
    case "$subcommand" in
        attach)
            if [[ "$COMP_CWORD" -eq 2 ]]; then
                _agentbox_completion_roots attach
            fi
            ;;
        stop)
            if [[ "$COMP_CWORD" -eq 2 ]]; then
                if [[ "$cur" == --* ]]; then
                    COMPREPLY=( $(compgen -W "--force" -- "$cur") )
                else
                    _agentbox_completion_roots stop
                fi
            elif [[ "$COMP_CWORD" -eq 3 && "${COMP_WORDS[2]}" == "--force" ]]; then
                _agentbox_completion_roots stop
            fi
        ;;
        run)
            if [[ "${COMP_WORDS[COMP_CWORD-1]}" == "--runtime" ]]; then
                COMPREPLY=( $(compgen -W "@RUNTIME_VALUES@" -- "$cur") )
            elif [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--runtime" -- "$cur") )
            fi
            ;;
        ls|health)
            if [[ "${COMP_WORDS[COMP_CWORD-1]}" == "--output" || "${COMP_WORDS[COMP_CWORD-1]}" == "-o" ]]; then
                COMPREPLY=( $(compgen -W "@OUTPUT_VALUES@" -- "$cur") )
            elif [[ "$cur" == -* ]]; then
                COMPREPLY=( $(compgen -W "--output -o" -- "$cur") )
            fi
            ;;
        runtime)
            if [[ "$COMP_CWORD" -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "update" -- "$cur") )
            elif [[ "$COMP_CWORD" -eq 3 && "${COMP_WORDS[2]}" == "update" ]]; then
                COMPREPLY=( $(compgen -W "@RUNTIME_VALUES@" -- "$cur") )
            fi
            ;;
        completion)
            if [[ "$COMP_CWORD" -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "@SHELL_VALUES@" -- "$cur") )
            fi
            ;;
    esac
}

complete -F _agentbox agentbox
"#,
    )
}

fn zsh_script() -> String {
    completion_script(
        r#"#compdef agentbox

_agentbox_completion_roots() {
  local command
  command="${1:?missing command}"
  local line root runtime status container
  local -a candidates descriptions
  for line in ${(f)"$({ agentbox __completion-roots "$command" 2>/dev/null; } || true)"}; do
    IFS=$'\t' read -r root runtime status container <<< "$line"
    candidates+=("$root")
    descriptions+=("${runtime} ${status}")
  done
  compadd -d descriptions -- "${candidates[@]}"
}

_agentbox() {
  local -a subcommands
  subcommands=(
@ZSH_SUBCOMMAND_SPECS@
  )

  if (( CURRENT == 2 )); then
    _describe 'command' subcommands
    return
  fi

  case "$words[2]" in
    attach)
      (( CURRENT == 3 )) && _agentbox_completion_roots attach
      ;;
    stop)
      if (( CURRENT == 3 )); then
        _values 'option' '--force[clean up duplicate or failed exact matches]'
        _agentbox_completion_roots stop
      elif (( CURRENT == 4 && "$words[3]" == "--force" )); then
        _agentbox_completion_roots stop
      fi
      ;;
    run)
      if (( CURRENT > 2 && "$words[CURRENT-1]" == "--runtime" )); then
        _values 'runtime' @RUNTIME_VALUES@
      else
        _values 'option' '--runtime[select runtime]'
      fi
      ;;
    ls|health)
      if [[ $CURRENT -gt 2 && ( "$words[CURRENT-1]" == "--output" || "$words[CURRENT-1]" == "-o" ) ]]; then
        _values 'output' @OUTPUT_VALUES@
      else
        _values 'option' '--output[select output format]' '-o[select output format]'
      fi
      ;;
    runtime)
      if (( CURRENT == 3 )); then
        _values 'runtime command' 'update[Update a default runtime image]'
      elif (( CURRENT == 4 && "$words[3]" == "update" )); then
        _values 'runtime' @RUNTIME_VALUES@
      fi
      ;;
    completion)
      (( CURRENT == 3 )) && _values 'shell' @SHELL_VALUES@
      ;;
  esac
}

compdef _agentbox agentbox
"#,
    )
}

fn fish_script() -> String {
    completion_script(
        r#"function __agentbox_has_subcommand
    set -l tokens (commandline -opc)
    for token in $tokens[2..-1]
        switch $token
            case @SUBCOMMAND_NAMES@
                return 0
        end
    end
    return 1
end

function __agentbox_completion_roots --argument-names command
    agentbox __completion-roots $command 2>/dev/null | while read -l root runtime status container
        printf "%s\t%s %s\n" "$root" "$runtime" "$status"
    end
end

complete -c agentbox -f -n "not __agentbox_has_subcommand" -a "@SUBCOMMAND_NAMES@"
complete -c agentbox -f -n "__fish_seen_subcommand_from attach" -a "(__agentbox_completion_roots attach)"
complete -c agentbox -f -n "__fish_seen_subcommand_from ls" -s o -l output -r -a "@OUTPUT_VALUES@"
complete -c agentbox -f -n "__fish_seen_subcommand_from health" -s o -l output -r -a "@OUTPUT_VALUES@"
complete -c agentbox -f -n "__fish_seen_subcommand_from stop" -l force -d "Clean up duplicate or failed exact matches"
complete -c agentbox -f -n "__fish_seen_subcommand_from stop" -a "(__agentbox_completion_roots stop)"
complete -c agentbox -f -n "__fish_seen_subcommand_from run" -l runtime -r -a "@RUNTIME_VALUES@"
complete -c agentbox -f -n "__fish_seen_subcommand_from runtime" -a "update"
complete -c agentbox -f -n "__fish_seen_subcommand_from runtime; and __fish_seen_subcommand_from update" -a "@RUNTIME_VALUES@"
complete -c agentbox -f -n "__fish_seen_subcommand_from completion" -a "@SHELL_VALUES@"
"#,
    )
}

fn completion_script(template: &str) -> String {
    let subcommands = completion_subcommands();
    let runtime_values = RuntimeKind::supported_values().join(" ");
    let output_values = OutputFormat::supported_values().join(" ");
    let shell_values = CompletionShell::supported_values().join(" ");
    let subcommand_names = completion_subcommand_names(&subcommands);
    let zsh_subcommand_specs = zsh_subcommand_specs(&subcommands);

    template
        .replace("@RUNTIME_VALUES@", &runtime_values)
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
        "run" => "Run a detached runtime server session".to_string(),
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

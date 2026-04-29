use crate::cli::CompletionShell;
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

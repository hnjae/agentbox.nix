// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::cli::StopArgs;
use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{SessionRecord, discover_agentbox_containers};
use crate::{Error, Result};

use super::session_targets::SessionTargetKind;

mod cleanup;

use super::target::SessionTargetInput;
use cleanup::{stop_all_running, stop_target};

pub fn run(args: StopArgs) -> Result<()> {
    if args.all {
        if !args.targets.is_empty() {
            return Err(Error::msg("stop --all does not accept a target"));
        }

        return stop_all_running();
    }

    let targets = if args.targets.is_empty() {
        select_stop_targets()?
    } else {
        args.targets
            .into_iter()
            .map(SessionTargetInput::Cli)
            .collect()
    };

    stop_targets(&targets, args.force)
}

fn select_stop_targets() -> Result<Vec<SessionTargetInput>> {
    let non_tty_error =
        "agentbox stop requires a target or --all when stdin or stderr is not a TTY";
    prompt::require_interactive_terminal(non_tty_error)?;
    let podman = Podman::new();
    let candidates = stop_prompt_candidates(&discover_agentbox_containers(&podman)?);

    if candidates.is_empty() {
        diagnostic::info("agentbox stop: no agentbox containers available to stop");
        return Ok(Vec::new());
    }

    let selected = prompt::select_many("Select sessions to stop", candidates, non_tty_error)?;
    if selected.is_empty() {
        diagnostic::warning("agentbox stop: no sessions selected");
        return Ok(Vec::new());
    }

    let mut targets = selected
        .into_iter()
        .map(prompt::Choice::into_value)
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();

    Ok(targets
        .into_iter()
        .map(SessionTargetInput::StableId)
        .collect())
}

fn stop_targets(targets: &[SessionTargetInput], force: bool) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }

    if targets.len() == 1 {
        return stop_target(&targets[0], force);
    }

    let mut failures = Vec::new();
    for target in targets {
        if let Err(error) = stop_target(target, force) {
            failures.push(TargetStopFailure::new(target, error));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_target_stop_failures(&failures)))
    }
}

struct TargetStopFailure {
    target: String,
    error: Error,
}

impl TargetStopFailure {
    fn new(target: &SessionTargetInput, error: Error) -> Self {
        Self {
            target: target.display(),
            error,
        }
    }
}

fn render_target_stop_failures(failures: &[TargetStopFailure]) -> String {
    let noun = if failures.len() == 1 {
        "target"
    } else {
        "targets"
    };
    let details = failures
        .iter()
        .map(|failure| format!("`{}`: {}", failure.target, failure.error))
        .collect::<Vec<_>>()
        .join("; ");

    format!("failed to stop {} {noun}: {details}", failures.len())
}

pub type StopPromptCandidate = prompt::Choice<String>;

pub fn stop_prompt_candidates(sessions: &[SessionRecord]) -> Vec<StopPromptCandidate> {
    SessionTargetKind::StableId.prompt_choices(
        sessions,
        |candidate| candidate.value().to_string(),
        |candidate| candidate.stop_prompt_label(),
    )
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::StopArgs;
use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{SessionRecord, discover_managed_sessions};
use crate::{Error, Result};

use super::session_targets::SessionTargetKind;

mod cleanup;
mod target;

use cleanup::{stop_all_running, stop_target};
use target::StopTargetInput;

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
        args.targets.into_iter().map(StopTargetInput::Cli).collect()
    };

    stop_targets(&targets, args.force)
}

fn select_stop_targets() -> Result<Vec<StopTargetInput>> {
    let non_tty_error =
        "agentbox stop requires a target or --all when stdin or stderr is not a TTY";
    prompt::require_interactive_terminal(non_tty_error)?;
    let podman = Podman::new();
    let candidates = stop_prompt_candidates(&discover_managed_sessions(&podman)?);

    if candidates.is_empty() {
        diagnostic::info("agentbox stop: no managed sessions available to stop");
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

    Ok(targets.into_iter().map(StopTargetInput::StableId).collect())
}

fn stop_targets(targets: &[StopTargetInput], force: bool) -> Result<()> {
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
    fn new(target: &StopTargetInput, error: Error) -> Self {
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

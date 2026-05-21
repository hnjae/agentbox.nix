// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use clap::Args;

use crate::diagnostic;
use crate::prompt;
use crate::session::{SessionRecord, SessionTargetInput};
use crate::{Error, Result};

use super::session_targets::{
    SessionTargetMultiSelection, SessionTargetSurface, select_many_session_targets,
    stop_prompt_label,
};

mod cleanup;

use cleanup::{stop_all_running, stop_target};

const STOP_NON_TTY_ERROR: &str =
    "agentbox stop requires a target or --all when stdin or stderr is not a TTY";

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
    let selected = select_many_session_targets(
        SessionTargetSurface::Stop,
        "Select sessions to stop",
        STOP_NON_TTY_ERROR,
        |candidate| candidate.value().to_string(),
        stop_prompt_label,
    )?;

    match selected {
        SessionTargetMultiSelection::NoCandidates => {
            diagnostic::info("agentbox stop: no agentbox containers available to stop");
            Ok(Vec::new())
        }
        SessionTargetMultiSelection::EmptySelection => {
            diagnostic::warning("agentbox stop: no sessions selected");
            Ok(Vec::new())
        }
        SessionTargetMultiSelection::Selected(mut targets) => {
            targets.sort();
            targets.dedup();

            Ok(targets
                .into_iter()
                .map(SessionTargetInput::StableId)
                .collect())
        }
    }
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
    SessionTargetSurface::Stop.prompt_choices(
        sessions,
        |candidate| candidate.value().to_string(),
        stop_prompt_label,
    )
}

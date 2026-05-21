// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::podman::Podman;
use crate::session::SessionRecord;
use crate::{Result, commands::session_targets::SessionTargetSurface};

use super::CompletionRootCommand;

pub(super) fn live_roots(command: CompletionRootCommand) -> Result<Vec<SessionRecord>> {
    let podman = Podman::new();
    let surface = completion_target_surface(command);
    let sessions = surface.discover(&podman)?;

    Ok(surface.target_sessions(sessions))
}

pub fn live_roots_output(command: CompletionRootCommand) -> Result<String> {
    let surface = completion_target_surface(command);
    let sessions = live_roots(command)?;
    let lines = surface.completion_lines(&sessions);

    Ok(lines.join("\n"))
}

fn completion_target_surface(command: CompletionRootCommand) -> SessionTargetSurface {
    match command {
        CompletionRootCommand::Connect => SessionTargetSurface::Connect,
        CompletionRootCommand::Health => SessionTargetSurface::Health,
        CompletionRootCommand::Restart => SessionTargetSurface::Restart,
        CompletionRootCommand::Stop => SessionTargetSurface::Stop,
    }
}

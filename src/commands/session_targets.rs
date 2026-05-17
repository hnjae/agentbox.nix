// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::prompt;
use crate::session::{SessionRecord, SessionTargetCandidate, SessionTargetKind};

pub(super) fn prompt_choices<T>(
    kind: SessionTargetKind,
    sessions: &[SessionRecord],
    value: impl Fn(SessionTargetCandidate<'_>) -> T,
    label: impl Fn(SessionTargetCandidate<'_>) -> String,
) -> Vec<prompt::Choice<T>> {
    let mut choices = kind
        .candidates(sessions)
        .map(|candidate| prompt::Choice::new(label(candidate), value(candidate)))
        .collect::<Vec<_>>();
    prompt::sort_choices_by_label(&mut choices);
    choices
}

pub(super) fn connect_prompt_label(candidate: SessionTargetCandidate<'_>) -> String {
    format!(
        "{} ({})",
        candidate.canonical_git_root_or_unknown(),
        candidate.runtime_or_unknown()
    )
}

pub(super) fn stop_prompt_label(candidate: SessionTargetCandidate<'_>) -> String {
    format!(
        "{} {} {} {}",
        candidate.value(),
        candidate.canonical_git_root_or_unknown(),
        candidate.runtime_or_unknown(),
        candidate.status_str(),
    )
}

pub(super) fn completion_line(candidate: SessionTargetCandidate<'_>) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        candidate.value(),
        candidate.canonical_git_root_or_unknown(),
        candidate.runtime_or_unknown(),
        candidate.status_str(),
    )
}

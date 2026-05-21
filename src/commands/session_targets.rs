// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::podman::Podman;
use crate::prompt;
use crate::session::{
    SessionDiscoveryQuery, SessionRecord, SessionTargetCandidate, SessionTargetKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionTargetSurface {
    Connect,
    Health,
    Restart,
    Stop,
}

impl SessionTargetSurface {
    pub(super) fn discover(self, podman: &Podman) -> Result<Vec<SessionRecord>> {
        self.discovery_query().discover(podman)
    }

    pub(super) fn target_sessions(self, sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
        sessions
            .into_iter()
            .filter(|session| self.target_kind().matches(session))
            .collect()
    }

    pub(super) fn completion_lines(self, sessions: &[SessionRecord]) -> Vec<String> {
        self.target_kind()
            .candidates(sessions)
            .map(completion_line)
            .collect()
    }

    pub(super) fn prompt_choices<T>(
        self,
        sessions: &[SessionRecord],
        value: impl Fn(SessionTargetCandidate<'_>) -> T,
        label: impl Fn(SessionTargetCandidate<'_>) -> String,
    ) -> Vec<prompt::Choice<T>> {
        prompt_choices(self.target_kind(), sessions, value, label)
    }

    fn discovery_query(self) -> SessionDiscoveryQuery<'static> {
        match self {
            Self::Connect | Self::Health | Self::Restart => {
                SessionDiscoveryQuery::managed_sessions()
            }
            Self::Stop => SessionDiscoveryQuery::agentbox_containers(),
        }
    }

    fn target_kind(self) -> SessionTargetKind {
        match self {
            Self::Connect => SessionTargetKind::ConnectRoot,
            Self::Restart => SessionTargetKind::RestartStableId,
            Self::Health | Self::Stop => SessionTargetKind::StableId,
        }
    }
}

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

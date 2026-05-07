use crate::prompt;
use crate::session::SessionRecord;

use super::session_output::SessionDisplay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionTargetKind {
    ConnectRoot,
    StableId,
}

impl SessionTargetKind {
    pub(super) fn candidate<'a>(
        self,
        session: &'a SessionRecord,
    ) -> Option<SessionTargetCandidate<'a>> {
        let value = match self {
            Self::ConnectRoot if session.is_connectable_candidate() => {
                session.canonical_git_root()?.as_str()
            }
            Self::StableId if session.has_stable_id() => session.stable_id()?,
            _ => return None,
        };
        let display = SessionDisplay::from_session(session);

        Some(SessionTargetCandidate {
            value,
            canonical_git_root: display.canonical_git_root_or_unknown(),
            runtime: display.runtime_or_unknown(),
            status: session.status.as_str(),
        })
    }

    pub(super) fn candidates<'a>(
        self,
        sessions: &'a [SessionRecord],
    ) -> impl Iterator<Item = SessionTargetCandidate<'a>> + 'a {
        sessions
            .iter()
            .filter_map(move |session| self.candidate(session))
    }

    pub(super) fn prompt_choices<T>(
        self,
        sessions: &[SessionRecord],
        value: impl Fn(&SessionTargetCandidate<'_>) -> T,
        label: impl Fn(&SessionTargetCandidate<'_>) -> String,
    ) -> Vec<prompt::Choice<T>> {
        let mut choices = self
            .candidates(sessions)
            .map(|candidate| prompt::Choice::new(label(&candidate), value(&candidate)))
            .collect::<Vec<_>>();
        prompt::sort_choices_by_label(&mut choices);
        choices
    }

    pub(super) fn matches(self, session: &SessionRecord) -> bool {
        self.candidate(session).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionTargetCandidate<'a> {
    value: &'a str,
    canonical_git_root: &'a str,
    runtime: &'a str,
    status: &'static str,
}

impl<'a> SessionTargetCandidate<'a> {
    pub(super) fn value(&self) -> &'a str {
        self.value
    }

    pub(super) fn connect_prompt_label(&self) -> String {
        format!("{} ({})", self.canonical_git_root, self.runtime)
    }

    pub(super) fn stop_prompt_label(&self) -> String {
        format!(
            "{} {} {} {}",
            self.value, self.canonical_git_root, self.runtime, self.status,
        )
    }

    pub(super) fn completion_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.value, self.canonical_git_root, self.runtime, self.status,
        )
    }
}

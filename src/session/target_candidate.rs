// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use super::record::SessionRecord;
use super::status::SessionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionTargetKind {
    ConnectRoot,
    RestartStableId,
    StableId,
}

impl SessionTargetKind {
    pub(crate) fn candidate<'a>(
        self,
        session: &'a SessionRecord,
    ) -> Option<SessionTargetCandidate<'a>> {
        let value = match self {
            Self::ConnectRoot if session.is_connectable_candidate() => {
                session.canonical_git_root()?.as_str()
            }
            Self::RestartStableId if session.is_restartable_candidate() => session.stable_id()?,
            Self::StableId if session.has_stable_id() => session.stable_id()?,
            _ => return None,
        };

        Some(SessionTargetCandidate {
            value,
            canonical_git_root: session.canonical_git_root(),
            runtime: session.runtime(),
            status: session.status(),
        })
    }

    pub(crate) fn candidates<'a>(
        self,
        sessions: &'a [SessionRecord],
    ) -> impl Iterator<Item = SessionTargetCandidate<'a>> + 'a {
        sessions
            .iter()
            .filter_map(move |session| self.candidate(session))
    }

    pub(crate) fn matches(self, session: &SessionRecord) -> bool {
        self.candidate(session).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionTargetCandidate<'a> {
    value: &'a str,
    canonical_git_root: Option<&'a Utf8Path>,
    runtime: Option<&'a str>,
    status: SessionStatus,
}

impl<'a> SessionTargetCandidate<'a> {
    pub(crate) fn value(self) -> &'a str {
        self.value
    }

    pub(crate) fn canonical_git_root_or_unknown(self) -> &'a str {
        self.canonical_git_root
            .map(Utf8Path::as_str)
            .unwrap_or("unknown")
    }

    pub(crate) fn runtime_or_unknown(self) -> &'a str {
        self.runtime.unwrap_or("unknown")
    }

    pub(crate) fn status_str(self) -> &'static str {
        self.status.as_str()
    }
}

#[cfg(test)]
mod tests {
    use crate::metadata::{LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, LABEL_RUNTIME};
    use crate::session::SessionStatus;
    use crate::session::test_support::SessionRecordFixture;

    use super::*;

    #[test]
    fn connect_root_candidates_require_connectable_managed_sessions() {
        let sessions = vec![
            managed_running_session("abcdef123456"),
            transient_session("fedcba654321"),
            failed_session("aaaaaa111111"),
            managed_without_endpoint("bbbbbb222222"),
        ];

        let candidates = SessionTargetKind::ConnectRoot
            .candidates(&sessions)
            .map(|candidate| candidate.value().to_string())
            .collect::<Vec<_>>();

        assert_eq!(candidates, vec!["/workspace/abcdef123456"]);
    }

    #[test]
    fn restart_stable_id_candidates_require_restartable_managed_sessions() {
        let sessions = vec![
            managed_running_session("abcdef123456"),
            transient_session("fedcba654321"),
            failed_session("aaaaaa111111"),
            managed_without_runtime("bbbbbb222222"),
            managed_without_launch_directory("cccccc333333"),
        ];

        let candidates = SessionTargetKind::RestartStableId
            .candidates(&sessions)
            .map(|candidate| candidate.value().to_string())
            .collect::<Vec<_>>();

        assert_eq!(candidates, vec!["abcdef123456"]);
    }

    #[test]
    fn stable_id_candidates_include_any_agentbox_record_with_a_stable_id() {
        let sessions = vec![
            managed_running_session("abcdef123456"),
            transient_session("fedcba654321"),
            failed_session("aaaaaa111111"),
            session_without_stable_id(),
        ];

        let candidates = SessionTargetKind::StableId
            .candidates(&sessions)
            .map(|candidate| candidate.value().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            candidates,
            vec!["abcdef123456", "fedcba654321", "aaaaaa111111"]
        );
    }

    fn managed_running_session(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::managed(stable_id).build()
    }

    fn transient_session(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::transient_run(stable_id).build()
    }

    fn failed_session(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::managed(stable_id)
            .status(SessionStatus::failed_unknown())
            .build()
    }

    fn managed_without_endpoint(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::managed(stable_id)
            .without_attach_endpoint()
            .build()
    }

    fn managed_without_runtime(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::managed(stable_id)
            .without_label(LABEL_RUNTIME)
            .build()
    }

    fn managed_without_launch_directory(stable_id: &str) -> SessionRecord {
        SessionRecordFixture::managed(stable_id)
            .without_label(LABEL_LAUNCH_DIRECTORY)
            .build()
    }

    fn session_without_stable_id() -> SessionRecord {
        SessionRecordFixture::managed("dddddd444444")
            .without_label(LABEL_GIT_ROOT_HASH)
            .build()
    }
}

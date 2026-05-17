// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::metadata::AgentboxContainerKind;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

use super::record::SessionRecord;
use super::status::{SessionStatus, resource_failure_requires_action_error};

#[derive(Debug)]
pub(crate) struct RestartableSession<'a> {
    session: &'a SessionRecord,
    runtime: RuntimeKind,
    launch_directory: &'a Utf8Path,
}

impl<'a> RestartableSession<'a> {
    pub(crate) fn session(&self) -> &'a SessionRecord {
        self.session
    }

    pub(crate) fn runtime(&self) -> RuntimeKind {
        self.runtime
    }

    pub(crate) fn launch_directory(&self) -> &'a Utf8Path {
        self.launch_directory
    }
}

pub(crate) fn prepare_restart_session<'a>(
    target_display: &str,
    session: &'a SessionRecord,
) -> Result<RestartableSession<'a>> {
    validate_restartable_status(target_display, session)?;

    let runtime = session_runtime(session)?;
    let launch_directory = session_launch_directory(session)?;

    Ok(RestartableSession {
        session,
        runtime,
        launch_directory,
    })
}

fn validate_restartable_status(target_display: &str, session: &SessionRecord) -> Result<()> {
    if session.is_transient_run() {
        return Err(Error::msg(format!(
            "transient run container `{}` cannot be restarted; stop it with `agentbox stop {}`",
            session.container_name,
            session.stable_id().unwrap_or(&session.container_name),
        )));
    }

    if !session.is_managed_session() {
        return Err(Error::msg(format!(
            "restart target `{target_display}` is not a managed session"
        )));
    }

    match session.status {
        SessionStatus::Running => Ok(()),
        SessionStatus::Orphaned => Err(Error::orphaned_managed_session(
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
        )),
        SessionStatus::Duplicate => Err(Error::duplicate_managed_sessions(
            restart_session_git_root(session)?.as_ref(),
        )),
        SessionStatus::Failed(Some(failure)) => Err(resource_failure_requires_action_error(
            AgentboxContainerKind::Managed,
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
            failure,
        )),
        SessionStatus::Failed(None) => Err(Error::failed_managed_session(
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
        )),
    }
}

fn restart_session_git_root(session: &SessionRecord) -> Result<Utf8PathBuf> {
    session.canonical_git_root().map(Utf8Path::to_path_buf).ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted safely because it has no recoverable git-root label",
            session.container_name
        ))
    })
}

fn session_runtime(session: &SessionRecord) -> Result<RuntimeKind> {
    session.runtime_kind().ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted because it has an unsupported or malformed `io.agentbox.runtime` label",
            session.container_name
        ))
    })
}

fn session_launch_directory(session: &SessionRecord) -> Result<&Utf8Path> {
    session.launch_directory().ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted because it has a missing or malformed `io.agentbox.launch_directory` label",
            session.container_name
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::metadata::{
        LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, LABEL_RUNTIME,
    };
    use crate::session::SessionMetadata;

    use super::*;

    #[test]
    fn running_managed_session_returns_restart_metadata() {
        let session = session(
            AgentboxContainerKind::Managed,
            SessionStatus::Running,
            &[
                (LABEL_GIT_ROOT, "/workspace/project"),
                (LABEL_GIT_ROOT_HASH, "abcdef123456"),
                (LABEL_RUNTIME, "opencode"),
                (LABEL_LAUNCH_DIRECTORY, "/workspace/project/nested"),
            ],
        );

        let restartable = prepare_restart_session("/workspace/project", &session).unwrap();

        assert_eq!(restartable.session().container_name, "agentbox-example");
        assert_eq!(restartable.runtime(), RuntimeKind::Opencode);
        assert_eq!(
            restartable.launch_directory(),
            Utf8Path::new("/workspace/project/nested")
        );
    }

    #[test]
    fn transient_run_container_is_not_restartable() {
        let session = session(
            AgentboxContainerKind::Run,
            SessionStatus::Running,
            &[(LABEL_GIT_ROOT_HASH, "abcdef123456")],
        );

        let error = prepare_restart_session("abcdef", &session).unwrap_err();

        assert!(error.to_string().contains("transient run container"));
        assert!(error.to_string().contains("cannot be restarted"));
    }

    #[test]
    fn running_session_requires_runtime_label() {
        let session = session(
            AgentboxContainerKind::Managed,
            SessionStatus::Running,
            &[
                (LABEL_GIT_ROOT, "/workspace/project"),
                (LABEL_GIT_ROOT_HASH, "abcdef123456"),
                (LABEL_LAUNCH_DIRECTORY, "/workspace/project"),
            ],
        );

        let error = prepare_restart_session("/workspace/project", &session).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("unsupported or malformed `io.agentbox.runtime` label")
        );
    }

    #[test]
    fn failed_session_without_git_root_reports_unrecoverable_root() {
        let session = session(
            AgentboxContainerKind::Managed,
            SessionStatus::failed_unknown(),
            &[(LABEL_GIT_ROOT_HASH, "abcdef123456")],
        );

        let error = prepare_restart_session("abcdef", &session).unwrap_err();

        assert!(error.to_string().contains("no recoverable git-root label"));
    }

    fn session(
        container_kind: AgentboxContainerKind,
        status: SessionStatus,
        labels: &[(&str, &str)],
    ) -> SessionRecord {
        let labels = labels
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<BTreeMap<_, _>>();

        SessionRecord {
            container_id: "container-id".to_string(),
            container_name: "agentbox-example".to_string(),
            container_kind,
            metadata: SessionMetadata::from_labels(&labels),
            attach_endpoint: None,
            container_running: status == SessionStatus::Running,
            status,
        }
    }
}

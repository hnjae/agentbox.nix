// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::conflict::duplicate_sessions_error;
use super::record::SessionRecord;
use super::selection::run_command_hint;
use super::status::{SessionFailure, SessionStatus, session_failure_requires_action_error};

pub(crate) struct ConnectableSession<'a> {
    session: &'a SessionRecord,
    runtime: RuntimeKind,
    endpoint: &'a AttachEndpoint,
    launch_directory: &'a Utf8Path,
}

impl<'a> ConnectableSession<'a> {
    pub(crate) fn session(&self) -> &'a SessionRecord {
        self.session
    }

    pub(crate) fn runtime(&self) -> RuntimeKind {
        self.runtime
    }

    pub(crate) fn endpoint(&self) -> &'a AttachEndpoint {
        self.endpoint
    }

    pub(crate) fn launch_directory(&self) -> &'a Utf8Path {
        self.launch_directory
    }
}

pub(crate) fn prepare_connect_session<'a>(
    workspace: &WorkspaceIdentity,
    session: &'a SessionRecord,
) -> Result<ConnectableSession<'a>> {
    validate_connectable_status(workspace, session)?;

    let runtime = session_runtime(workspace, session)?;
    let endpoint = session_endpoint(workspace, session)?;
    let launch_directory = session_launch_directory(workspace, session)?;

    Ok(ConnectableSession {
        session,
        runtime,
        endpoint,
        launch_directory,
    })
}

fn session_runtime(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Result<RuntimeKind> {
    session.runtime_kind().ok_or_else(|| {
        session_failure_error(workspace, session, SessionFailure::UnsupportedRuntimeLabel)
    })
}

fn session_endpoint<'a>(
    workspace: &WorkspaceIdentity,
    session: &'a SessionRecord,
) -> Result<&'a AttachEndpoint> {
    session.attach_endpoint().ok_or_else(|| {
        session_failure_error(workspace, session, SessionFailure::MalformedEndpointLabels)
    })
}

fn session_launch_directory<'a>(
    workspace: &WorkspaceIdentity,
    session: &'a SessionRecord,
) -> Result<&'a Utf8Path> {
    session.launch_directory().ok_or_else(|| {
        session_failure_error(workspace, session, SessionFailure::MalformedLaunchDirectory)
    })
}

fn validate_connectable_status(
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Result<()> {
    match session.status() {
        SessionStatus::Running => Ok(()),
        SessionStatus::Orphaned => Err(Error::orphaned_managed_session(
            workspace.canonical_git_root.as_ref(),
            session.container_name(),
        )),
        SessionStatus::Failed(Some(SessionFailure::NotRunning)) => {
            Err(not_running_session_error(workspace, session))
        }
        SessionStatus::Failed(Some(failure)) => Err(session_failure_requires_action_error(
            workspace.canonical_git_root.as_ref(),
            session.container_name(),
            failure,
        )),
        SessionStatus::Failed(None) => Err(Error::failed_managed_session(
            workspace.canonical_git_root.as_ref(),
            session.container_name(),
        )),
        SessionStatus::Duplicate => Err(duplicate_sessions_error(workspace)),
    }
}

fn not_running_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` for `{}` is not running; rerun `{}` to start a new session or `agentbox stop {}` to remove the leftover container",
        session.container_name(),
        workspace.canonical_git_root,
        run_command_hint(session.runtime(), workspace),
        workspace.requested_target,
    ))
}

fn session_failure_error(
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
    failure: SessionFailure,
) -> Error {
    session_failure_requires_action_error(
        workspace.canonical_git_root.as_ref(),
        session.container_name(),
        failure,
    )
}

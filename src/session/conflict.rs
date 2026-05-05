use crate::Error;
use crate::metadata::LABEL_LOGICAL_NAME;
use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::RuntimeCreateSpec;
use crate::workspace::WorkspaceIdentity;

use super::labels::{SessionIdentityLabels, SessionLabelReport};
use super::mounts::has_mount_destination;
use super::{
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionFailure, SessionMetadata, SessionRecord,
    SessionStatus, failed_session_requires_action_error, session_failure_requires_action_error,
};

pub(crate) fn existing_session_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Error {
    if session.status == SessionStatus::Duplicate {
        return duplicate_sessions_error(workspace);
    }

    match session.status {
        SessionStatus::Running => running_existing_session_error(workspace, session),
        SessionStatus::Orphaned => Error::orphaned_managed_session(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
        ),
        SessionStatus::Failed(_) => {
            failed_session_requires_action_error(workspace.canonical_git_root.as_ref(), session)
                .unwrap_or_else(|| {
                    podman
                        .inspect_one(&session.container_name)
                        .map(|inspect| {
                            classify_named_container_conflict(
                                workspace,
                                &session.container_name,
                                &inspect,
                            )
                        })
                        .unwrap_or_else(|_| {
                            generic_failed_session_error(workspace, &session.container_name)
                        })
                })
        }
        SessionStatus::Duplicate => duplicate_sessions_error(workspace),
    }
}

pub(crate) fn classify_create_error_or_else(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
    fallback: impl FnOnce(Error) -> Error,
) -> Error {
    match podman.inspect_one(&workspace.container_name) {
        Ok(inspect) => classify_named_container_conflict(
            workspace,
            &create_spec.labels[LABEL_LOGICAL_NAME],
            &inspect,
        ),
        Err(_) => fallback(original_error),
    }
}

fn classify_named_container_conflict(
    workspace: &WorkspaceIdentity,
    expected_name: &str,
    inspect: &PodmanContainerInspect,
) -> Error {
    let metadata = SessionMetadata::from_labels(&inspect.config.labels);
    let container_name = inspect_container_name(&metadata, expected_name);

    if !metadata.is_managed() {
        return unmanaged_container_conflict_error(workspace);
    }

    let label_report = SessionLabelReport::from_metadata(&metadata);
    let identity = match label_report.identity_labels() {
        Ok(identity) => identity,
        Err(failure) => return failure_conflict_error(workspace, &container_name, failure),
    };
    let same_root_failure = label_report
        .required_failure()
        .or_else(|| label_report.attach_failure());

    ManagedContainerConflict {
        workspace,
        container_name: &container_name,
        identity,
        same_root_failure,
        inspect,
    }
    .classify()
}

struct ManagedContainerConflict<'a> {
    workspace: &'a WorkspaceIdentity,
    container_name: &'a str,
    identity: &'a SessionIdentityLabels,
    same_root_failure: Option<SessionFailure>,
    inspect: &'a PodmanContainerInspect,
}

impl ManagedContainerConflict<'_> {
    fn classify(&self) -> Error {
        if self.hash_matches_different_root() {
            return self.hash_collision_error();
        }

        if self.root_matches_workspace() {
            return self.same_root_error();
        }

        self.different_root_error()
    }

    fn hash_matches_different_root(&self) -> bool {
        self.identity.git_root_hash() == self.workspace.hash12.as_str()
            && !self.root_matches_workspace()
    }

    fn root_matches_workspace(&self) -> bool {
        self.identity.canonical_git_root().as_str() == self.workspace.canonical_git_root.as_str()
    }

    fn hash_matches_workspace(&self) -> bool {
        self.identity.git_root_hash() == self.workspace.hash12.as_str()
    }

    fn same_root_error(&self) -> Error {
        if let Some(failure) = self.same_root_failure {
            return failure_conflict_error(self.workspace, self.container_name, failure);
        }

        if !self.hash_matches_workspace() {
            return failure_conflict_error(
                self.workspace,
                self.container_name,
                SessionFailure::DriftedGitRootHash,
            );
        }

        if !has_mount_destination(&self.inspect.mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
            return failure_conflict_error(
                self.workspace,
                self.container_name,
                SessionFailure::MissingCacheMount,
            );
        }

        generic_failed_session_error(self.workspace, self.container_name)
    }

    fn hash_collision_error(&self) -> Error {
        Error::msg(format!(
            "managed container `{}` collides on git-root hash `{}`: stored root `{}` does not match `{}`; remove or recreate the conflicting container before retrying",
            self.container_name,
            self.workspace.hash12,
            self.identity.canonical_git_root(),
            self.workspace.canonical_git_root,
        ))
    }

    fn different_root_error(&self) -> Error {
        Error::msg(format!(
            "container name `{}` is already used by managed session `{}` for `{}`; remove or rename the conflicting container before retrying `{}`",
            self.workspace.container_name,
            self.container_name,
            self.identity.canonical_git_root(),
            self.workspace.canonical_git_root,
        ))
    }
}

fn unmanaged_container_conflict_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "container name `{}` is already in use by a different container; remove or rename that container before retrying `{}`",
        workspace.container_name, workspace.canonical_git_root,
    ))
}

pub(crate) fn duplicate_sessions_error(workspace: &WorkspaceIdentity) -> Error {
    Error::duplicate_managed_sessions(workspace.canonical_git_root.as_ref())
}

fn running_existing_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` is already running for `{}`; use `agentbox attach {}` to join it or `agentbox stop {}` to stop it first",
        session.container_name,
        workspace.canonical_git_root,
        workspace.requested_target,
        workspace.requested_target,
    ))
}

fn generic_failed_session_error(workspace: &WorkspaceIdentity, container_name: &str) -> Error {
    Error::failed_managed_session(workspace.canonical_git_root.as_ref(), container_name)
}

fn failure_conflict_error(
    workspace: &WorkspaceIdentity,
    container_name: &str,
    failure: SessionFailure,
) -> Error {
    session_failure_requires_action_error(
        workspace.canonical_git_root.as_ref(),
        container_name,
        failure,
    )
}

fn inspect_container_name(metadata: &SessionMetadata, fallback: &str) -> String {
    metadata.logical_name_or(fallback).to_string()
}

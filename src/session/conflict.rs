use crate::Error;
use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::RuntimeCreateSpec;
use crate::workspace::WorkspaceIdentity;

use super::mounts::has_mount_destination;
use super::{
    REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionFailure, SessionMetadata, SessionRecord,
    SessionStatus, failed_session_requires_action_error, session_failure_requires_action_error,
};
use crate::metadata::LABEL_LOGICAL_NAME;

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
        SessionStatus::Failed => {
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

pub(crate) fn classify_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    match podman.inspect_one(&workspace.container_name) {
        Ok(inspect) => classify_named_container_conflict(
            workspace,
            &create_spec.labels[LABEL_LOGICAL_NAME],
            &inspect,
        ),
        Err(_) => original_error,
    }
}

fn classify_named_container_conflict(
    workspace: &WorkspaceIdentity,
    expected_name: &str,
    inspect: &PodmanContainerInspect,
) -> Error {
    let metadata = SessionMetadata::from_labels(&inspect.config.labels);
    let container_name = inspect_container_name(&metadata, expected_name);
    let canonical_git_root = metadata.canonical_git_root();
    let git_root_hash = metadata.git_root_hash();

    if metadata.is_managed() {
        if !metadata.has_all_required_label_values() {
            return failure_conflict_error(
                workspace,
                &container_name,
                SessionFailure::MissingRequiredLabels,
            );
        }

        if git_root_hash == Some(workspace.hash12.as_str())
            && canonical_git_root
                .is_some_and(|root| root.as_str() != workspace.canonical_git_root.as_str())
        {
            return Error::msg(format!(
                "managed container `{}` collides on git-root hash `{}`: stored root `{}` does not match `{}`; remove or recreate the conflicting container before retrying",
                container_name,
                workspace.hash12,
                canonical_git_root.map_or("<missing>", camino::Utf8Path::as_str),
                workspace.canonical_git_root,
            ));
        }

        if canonical_git_root
            .is_some_and(|root| root.as_str() == workspace.canonical_git_root.as_str())
        {
            if git_root_hash != Some(workspace.hash12.as_str()) {
                return failure_conflict_error(
                    workspace,
                    &container_name,
                    SessionFailure::DriftedGitRootHash,
                );
            }

            if !has_mount_destination(&inspect.mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
                return failure_conflict_error(
                    workspace,
                    &container_name,
                    SessionFailure::MissingCacheMount,
                );
            }

            return generic_failed_session_error(workspace, &container_name);
        }

        if let Some(root) = canonical_git_root {
            return Error::msg(format!(
                "container name `{}` is already used by managed session `{}` for `{}`; remove or rename the conflicting container before retrying `{}`",
                workspace.container_name, container_name, root, workspace.canonical_git_root,
            ));
        }
    }

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

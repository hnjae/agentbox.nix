// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::diagnostic;
use crate::podman::Podman;
use crate::session::{
    SessionDiscoveryQuery, SessionRecord, duplicate_agentbox_containers_error,
    existing_session_error, select_single_session,
};
use crate::workspace::WorkspaceIdentity;

use super::policy::{ExistingResourceCheck, ExistingResourceScope};

pub(super) fn ensure_required_resources_absent(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    check: ExistingResourceCheck,
) -> Result<()> {
    let ExistingResourceCheck::RequireAbsent(scope) = check else {
        return Ok(());
    };

    diagnostic::info(scope.diagnostic_message());
    let sessions = discover_existing_resources(podman, workspace, scope)?;
    if let Some(session) = select_existing_resource(&sessions, workspace, scope)? {
        return Err(existing_session_error(podman, workspace, session));
    }

    Ok(())
}

fn discover_existing_resources(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    scope: ExistingResourceScope,
) -> Result<Vec<SessionRecord>> {
    match scope {
        ExistingResourceScope::ManagedSessions => SessionDiscoveryQuery::managed_sessions()
            .for_git_root(workspace.canonical_git_root.as_ref())
            .discover(podman),
        ExistingResourceScope::AgentboxContainers => SessionDiscoveryQuery::agentbox_containers()
            .for_git_root(workspace.canonical_git_root.as_ref())
            .discover(podman),
    }
}

fn select_existing_resource<'a>(
    sessions: &'a [SessionRecord],
    workspace: &WorkspaceIdentity,
    scope: ExistingResourceScope,
) -> Result<Option<&'a SessionRecord>> {
    match sessions {
        [] => Ok(None),
        [session] => Ok(Some(session)),
        _ if scope == ExistingResourceScope::AgentboxContainers => {
            Err(duplicate_agentbox_containers_error(workspace))
        }
        _ => select_single_session(sessions, workspace),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use camino::Utf8PathBuf;

    use crate::Error;
    use crate::metadata::{AgentboxContainerKind, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH};
    use crate::session::{SessionMetadata, SessionStatus};

    use super::*;

    #[test]
    fn selecting_existing_resource_accepts_empty_scope() {
        let selected =
            select_existing_resource(&[], &workspace(), ExistingResourceScope::AgentboxContainers)
                .unwrap();

        assert!(selected.is_none());
    }

    #[test]
    fn selecting_existing_resource_accepts_single_match() {
        let workspace = workspace();
        let sessions = vec![session("/workspace/demo", "0123456789ab", "agentbox-demo")];

        let selected = select_existing_resource(
            &sessions,
            &workspace,
            ExistingResourceScope::AgentboxContainers,
        )
        .unwrap();

        assert_eq!(selected.unwrap().container_name(), "agentbox-demo");
    }

    #[test]
    fn selecting_existing_resource_rejects_duplicate_agentbox_containers() {
        let workspace = workspace();
        let sessions = vec![
            session("/workspace/demo", "0123456789ab", "agentbox-demo-1"),
            session("/workspace/demo", "0123456789ab", "agentbox-demo-2"),
        ];

        let error = select_existing_resource(
            &sessions,
            &workspace,
            ExistingResourceScope::AgentboxContainers,
        )
        .unwrap_err();

        assert!(error.to_string().contains("duplicate agentbox containers"));
    }

    #[test]
    fn selecting_existing_managed_sessions_uses_workspace_ambiguity_error() {
        let workspace = workspace();
        let sessions = vec![
            session("/workspace/demo", "0123456789ab", "agentbox-demo-1"),
            session("/workspace/demo", "0123456789ab", "agentbox-demo-2"),
        ];

        let error = select_existing_resource(
            &sessions,
            &workspace,
            ExistingResourceScope::ManagedSessions,
        )
        .unwrap_err();

        assert_existing_session_selection_error(error);
    }

    fn assert_existing_session_selection_error(error: Error) {
        assert!(error.to_string().contains("duplicate managed sessions"));
    }

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentity {
            requested_target: Utf8PathBuf::from("/workspace/demo"),
            absolute_target: Utf8PathBuf::from("/workspace/demo"),
            canonical_target: Utf8PathBuf::from("/workspace/demo"),
            canonical_git_root: Utf8PathBuf::from("/workspace/demo"),
            digest64: "0123456789abcdef".to_string(),
            hash12: "0123456789ab".to_string(),
            container_name: "agentbox-demo".to_string(),
        }
    }

    fn session(canonical_git_root: &str, stable_id: &str, name: &str) -> SessionRecord {
        let labels = BTreeMap::from([
            (LABEL_GIT_ROOT.to_string(), canonical_git_root.to_string()),
            (LABEL_GIT_ROOT_HASH.to_string(), stable_id.to_string()),
        ]);

        SessionRecord::new(
            format!("{name}-id"),
            name,
            AgentboxContainerKind::Managed,
            SessionMetadata::from_labels(&labels),
            None,
            true,
            SessionStatus::Running,
        )
    }
}

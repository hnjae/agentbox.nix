// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::metadata::{
    AgentboxContainerKind, LABEL_CONTAINER_KIND, LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE,
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_SESSION_METADATA_LABELS,
    REQUIRED_SESSION_WORKSPACE_IDENTITY_LABELS,
};
use crate::paths::path_is_or_descendant;
use crate::workspace::git_root_hash12;

use super::{
    SessionLabelResult, require_session_label, require_session_label_value, require_session_labels,
};
use crate::session::record::SessionMetadata;
use crate::session::status::SessionFailure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) struct SessionIdentityLabels {
    canonical_git_root: Utf8PathBuf,
    git_root_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::session) struct RequiredSessionLabels {
    identity: SessionIdentityLabels,
    launch_directory: Utf8PathBuf,
}

impl SessionIdentityLabels {
    pub(super) fn validated(labels: &SessionMetadata) -> SessionLabelResult<Self> {
        let identity = Self::from_session_labels(labels)?;
        if !identity.hash_matches_root() {
            return Err(SessionFailure::DriftedGitRootHash);
        }

        Ok(identity)
    }

    pub(in crate::session) fn canonical_git_root(&self) -> &Utf8Path {
        &self.canonical_git_root
    }

    pub(in crate::session) fn git_root_hash(&self) -> &str {
        &self.git_root_hash
    }

    fn from_session_labels(labels: &SessionMetadata) -> SessionLabelResult<Self> {
        require_agentbox_container_marker(labels)?;
        require_session_labels(labels, REQUIRED_SESSION_WORKSPACE_IDENTITY_LABELS)?;

        let canonical_git_root = Utf8PathBuf::from(require_session_label(labels, LABEL_GIT_ROOT)?);
        let git_root_hash = require_session_label(labels, LABEL_GIT_ROOT_HASH)?.to_string();

        Ok(Self {
            canonical_git_root,
            git_root_hash,
        })
    }

    fn hash_matches_root(&self) -> bool {
        self.git_root_hash == git_root_hash12(&self.canonical_git_root)
    }
}

impl RequiredSessionLabels {
    pub(super) fn from_validated_identity(
        labels: &SessionMetadata,
        identity: SessionLabelResult<SessionIdentityLabels>,
    ) -> SessionLabelResult<Self> {
        let identity = identity?;
        require_session_labels(labels, REQUIRED_SESSION_METADATA_LABELS)?;

        let required = Self {
            identity,
            launch_directory: Utf8PathBuf::from(require_session_label(
                labels,
                LABEL_LAUNCH_DIRECTORY,
            )?),
        };

        if !required.launch_directory_is_valid() {
            return Err(SessionFailure::MalformedLaunchDirectory);
        }

        Ok(required)
    }

    pub(in crate::session) fn canonical_git_root(&self) -> &Utf8Path {
        self.identity.canonical_git_root()
    }

    fn launch_directory_is_valid(&self) -> bool {
        self.launch_directory.is_absolute()
            && path_is_or_descendant(&self.launch_directory, self.identity.canonical_git_root())
    }
}

fn require_agentbox_container_marker(labels: &SessionMetadata) -> SessionLabelResult<()> {
    match labels.container_kind() {
        Some(AgentboxContainerKind::Managed) => {
            require_session_label_value(labels, LABEL_MANAGED, LABEL_MANAGED_VALUE)?;
            require_session_label_value(labels, LABEL_SCHEMA, LABEL_SCHEMA_VALUE)
        }
        Some(AgentboxContainerKind::Run) => require_session_label_value(
            labels,
            LABEL_CONTAINER_KIND,
            LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE,
        ),
        None => Err(SessionFailure::MissingRequiredLabels),
    }
}

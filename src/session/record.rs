// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::metadata::{
    AgentboxContainerKind, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY,
    LABEL_LOGICAL_NAME, LABEL_RUNTIME, agentbox_container_kind_from_labels, required_label_value,
};
use crate::runtime::{AttachEndpoint, RuntimeKind};

use super::status::SessionStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub container_id: String,
    pub container_name: String,
    pub container_kind: AgentboxContainerKind,
    pub metadata: SessionMetadata,
    pub attach_endpoint: Option<AttachEndpoint>,
    pub container_running: bool,
    pub status: SessionStatus,
}

impl SessionRecord {
    pub fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.metadata.canonical_git_root()
    }

    pub fn git_root_hash(&self) -> Option<&str> {
        self.metadata.git_root_hash()
    }

    pub fn stable_id(&self) -> Option<&str> {
        self.git_root_hash()
    }

    pub fn runtime(&self) -> Option<&str> {
        self.metadata.runtime()
    }

    pub fn launch_directory(&self) -> Option<&Utf8Path> {
        self.metadata.launch_directory()
    }

    pub fn runtime_kind(&self) -> Option<RuntimeKind> {
        self.metadata.runtime_kind()
    }

    pub fn container_kind(&self) -> AgentboxContainerKind {
        self.container_kind
    }

    pub fn is_managed_session(&self) -> bool {
        self.container_kind == AgentboxContainerKind::Managed
    }

    pub fn is_transient_run(&self) -> bool {
        self.container_kind == AgentboxContainerKind::Run
    }

    pub fn container_running(&self) -> bool {
        self.container_running
    }

    pub(crate) fn is_running(&self) -> bool {
        self.status.is_running()
    }

    pub(crate) fn has_stable_id(&self) -> bool {
        self.stable_id().is_some()
    }

    pub(crate) fn is_connectable_candidate(&self) -> bool {
        self.is_managed_session()
            && self.is_running()
            && self.attach_endpoint.is_some()
            && self.canonical_git_root().is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub(crate) labels: BTreeMap<String, String>,
}

impl SessionMetadata {
    pub fn from_labels(labels: &BTreeMap<String, String>) -> Self {
        Self {
            labels: labels.clone(),
        }
    }

    pub(crate) fn container_kind(&self) -> Option<AgentboxContainerKind> {
        agentbox_container_kind_from_labels(&self.labels)
    }

    pub(crate) fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.label(LABEL_GIT_ROOT).map(Utf8Path::new)
    }

    pub(crate) fn git_root_hash(&self) -> Option<&str> {
        self.label(LABEL_GIT_ROOT_HASH)
    }

    pub(crate) fn runtime(&self) -> Option<&str> {
        self.label(LABEL_RUNTIME)
    }

    pub(crate) fn runtime_kind(&self) -> Option<RuntimeKind> {
        self.runtime()?.parse().ok()
    }

    pub(crate) fn launch_directory(&self) -> Option<&Utf8Path> {
        self.label(LABEL_LAUNCH_DIRECTORY).map(Utf8Path::new)
    }

    pub(crate) fn logical_name_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.label(LABEL_LOGICAL_NAME).unwrap_or(fallback)
    }

    pub(super) fn label(&self, name: &str) -> Option<&str> {
        required_label_value(&self.labels, name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT};

    #[test]
    fn runtime_kind_only_depends_on_the_runtime_label() {
        let metadata = SessionMetadata::from_labels(&BTreeMap::from([
            (LABEL_RUNTIME.to_string(), RuntimeKind::Opencode.to_string()),
            (LABEL_ATTACH_SCHEME.to_string(), "ftp".to_string()),
            (LABEL_CONTAINER_PORT.to_string(), "not-a-port".to_string()),
            (
                LABEL_CONTAINER_LISTEN_IP.to_string(),
                "127.0.0.1".to_string(),
            ),
        ]));
        let session = SessionRecord {
            container_id: "container-id".to_string(),
            container_name: "agentbox-example".to_string(),
            container_kind: AgentboxContainerKind::Managed,
            metadata,
            attach_endpoint: None,
            container_running: false,
            status: SessionStatus::failed_unknown(),
        };

        assert_eq!(session.runtime_kind(), Some(RuntimeKind::Opencode));
        assert!(session.metadata.attach_labels().is_err());
    }
}

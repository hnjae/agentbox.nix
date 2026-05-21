// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::metadata::{
    AgentboxContainerKind, LABEL_ATTACH_SCHEME, LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH,
    LABEL_LAUNCH_DIRECTORY, LABEL_RUNTIME,
};
use crate::runtime::{AttachEndpoint, RuntimeKind};

use super::{SessionMetadata, SessionRecord, SessionRecordInput, SessionStatus};

#[derive(Debug, Clone)]
pub(crate) struct SessionRecordFixture {
    container_id: String,
    container_name: String,
    container_kind: AgentboxContainerKind,
    labels: BTreeMap<String, String>,
    attach_endpoint: Option<AttachEndpoint>,
    container_running: bool,
    status: SessionStatus,
}

impl SessionRecordFixture {
    pub(crate) fn managed(stable_id: impl Into<String>) -> Self {
        Self::new(AgentboxContainerKind::Managed, stable_id)
    }

    pub(crate) fn transient_run(stable_id: impl Into<String>) -> Self {
        Self::new(AgentboxContainerKind::Run, stable_id)
    }

    fn new(container_kind: AgentboxContainerKind, stable_id: impl Into<String>) -> Self {
        let stable_id = stable_id.into();
        let root = format!("/workspace/{stable_id}");
        Self {
            container_id: format!("{stable_id}-id"),
            container_name: format!("agentbox-{stable_id}"),
            container_kind,
            labels: BTreeMap::from([
                (LABEL_GIT_ROOT.to_string(), root.clone()),
                (LABEL_GIT_ROOT_HASH.to_string(), stable_id),
                (
                    LABEL_RUNTIME.to_string(),
                    RuntimeKind::Opencode.as_str().to_string(),
                ),
                (LABEL_LAUNCH_DIRECTORY.to_string(), root),
                (LABEL_ATTACH_SCHEME.to_string(), "http".to_string()),
            ]),
            attach_endpoint: Some(AttachEndpoint {
                scheme: "http".to_string(),
                host_ip: "127.0.0.1".to_string(),
                host_port: 4096,
            }),
            container_running: true,
            status: SessionStatus::Running,
        }
    }

    pub(crate) fn named(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        self.container_id = format!("{name}-id");
        self.container_name = name;
        self
    }

    pub(crate) fn root(mut self, root: impl Into<String>) -> Self {
        let root = root.into();
        self.labels.insert(LABEL_GIT_ROOT.to_string(), root.clone());
        self.labels.insert(LABEL_LAUNCH_DIRECTORY.to_string(), root);
        self
    }

    pub(crate) fn without_git_root(mut self) -> Self {
        self.labels.remove(LABEL_GIT_ROOT);
        self
    }

    pub(crate) fn without_label(mut self, label: &str) -> Self {
        self.labels.remove(label);
        self
    }

    pub(crate) fn label(mut self, label: &str, value: impl Into<String>) -> Self {
        self.labels.insert(label.to_string(), value.into());
        self
    }

    pub(crate) fn without_attach_endpoint(mut self) -> Self {
        self.attach_endpoint = None;
        self
    }

    pub(crate) fn status(mut self, status: SessionStatus) -> Self {
        self.container_running = status == SessionStatus::Running;
        self.status = status;
        self
    }

    pub(crate) fn build(self) -> SessionRecord {
        SessionRecord::new(SessionRecordInput {
            container_id: self.container_id,
            container_name: self.container_name,
            container_kind: self.container_kind,
            metadata: SessionMetadata::from_labels(&self.labels),
            attach_endpoint: self.attach_endpoint,
            container_running: self.container_running,
            status: self.status,
        })
    }
}

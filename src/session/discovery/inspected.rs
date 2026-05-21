// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::metadata::AgentboxContainerKind;
use crate::podman::{PodmanContainerInspect, PodmanContainerMount, PodmanPsContainer};

use super::super::endpoint::AttachEndpointReport;
use super::super::labels::SessionLabelReport;
use super::super::record::{SessionMetadata, SessionRecord, SessionRecordInput};
use super::super::status::{GitRootProbe, SessionStatusInput, derive_status};

pub(super) fn build_session_record(
    container: PodmanPsContainer,
    inspect: PodmanContainerInspect,
    container_kind: AgentboxContainerKind,
    git_root_probe: &dyn GitRootProbe,
) -> SessionRecord {
    InspectedAgentboxContainer::from_podman(container, inspect, container_kind)
        .into_session_record(git_root_probe)
}

struct InspectedAgentboxContainer {
    container_id: String,
    container_name: String,
    container_kind: AgentboxContainerKind,
    metadata: SessionMetadata,
    label_report: SessionLabelReport,
    attach_endpoint: AttachEndpointReport,
    running: bool,
    mounts: Vec<PodmanContainerMount>,
}

impl InspectedAgentboxContainer {
    fn from_podman(
        container: PodmanPsContainer,
        inspect: PodmanContainerInspect,
        container_kind: AgentboxContainerKind,
    ) -> Self {
        let labels = &inspect.config.labels;
        let container_name = container
            .names
            .as_ref()
            .and_then(|names| names.first())
            .cloned()
            .unwrap_or_else(|| container.id.clone());
        let metadata = SessionMetadata::from_labels(labels);
        let label_report = SessionLabelReport::from_metadata(&metadata);
        let attach_endpoint =
            AttachEndpointReport::from_label_report_and_inspect(&label_report, &inspect);
        let running = inspect.state.running;
        let mounts = inspect.mounts;

        Self {
            container_id: container.id,
            container_name,
            container_kind,
            metadata,
            label_report,
            attach_endpoint,
            running,
            mounts,
        }
    }

    fn into_session_record(self, git_root_probe: &dyn GitRootProbe) -> SessionRecord {
        let status = derive_status(SessionStatusInput {
            label_report: &self.label_report,
            attach_endpoint: &self.attach_endpoint,
            running: self.running,
            mounts: &self.mounts,
            git_root_probe,
        });
        let attach_endpoint = self.attach_endpoint.into_endpoint();

        SessionRecord::new(SessionRecordInput {
            container_id: self.container_id,
            container_name: self.container_name,
            container_kind: self.container_kind,
            metadata: self.metadata,
            attach_endpoint,
            container_running: self.running,
            status,
        })
    }
}

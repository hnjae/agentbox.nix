// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::podman::PodmanContainerInspect;
use crate::runtime::AttachEndpoint;
use crate::{Error, Result};

use super::labels::AttachLabels;
use super::labels::SessionLabelReport;
use super::record::SessionMetadata;
use super::status::SessionFailure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AttachEndpointReport {
    endpoint: std::result::Result<AttachEndpoint, SessionFailure>,
}

impl AttachEndpointReport {
    pub(super) fn from_label_report_and_inspect(
        label_report: &SessionLabelReport,
        inspect: &PodmanContainerInspect,
    ) -> Self {
        let endpoint = label_report.attach_labels().and_then(|attach_labels| {
            derive_attach_endpoint(attach_labels, inspect)
                .map_err(|_| SessionFailure::MissingPublishedAttachPort)
        });

        Self { endpoint }
    }

    pub(super) fn failure(&self) -> Option<SessionFailure> {
        self.endpoint.as_ref().err().copied()
    }

    pub(super) fn into_endpoint(self) -> Option<AttachEndpoint> {
        self.endpoint.ok()
    }
}

pub fn discover_attach_endpoint_from_inspect(
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let metadata = SessionMetadata::from_labels(&inspect.config.labels);
    let attach_labels = metadata
        .attach_labels()
        .map_err(|error| error.into_error())?;
    derive_attach_endpoint(attach_labels, inspect)
}

pub(super) fn derive_attach_endpoint(
    attach_labels: AttachLabels,
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let container_port = attach_labels.container_port();
    let port_key = format!("{container_port}/tcp");
    let published_port = inspect
        .network_settings
        .published_tcp_host_port(container_port)?
        .ok_or_else(|| {
            Error::msg(format!(
                "managed session has no published attach port for `{port_key}`"
            ))
        })?;

    Ok(AttachEndpoint {
        scheme: attach_labels.scheme().to_string(),
        host_ip: published_port.host_ip,
        host_port: published_port.host_port,
    })
}

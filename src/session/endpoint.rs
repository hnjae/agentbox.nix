// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::podman::PodmanContainerInspect;
use crate::runtime::{AttachEndpoint, DEFAULT_HOST_ATTACH_IP};
use crate::{Error, Result};

use super::labels::SessionLabels;

pub fn discover_attach_endpoint_from_inspect(
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let labels = SessionLabels::from_map(&inspect.config.labels);
    derive_attach_endpoint(&labels, inspect)
}

pub(super) fn derive_attach_endpoint(
    labels: &SessionLabels,
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let attach_labels = labels.attach_labels().map_err(|error| error.into_error())?;
    let container_port = attach_labels.container_port();

    let port_key = format!("{container_port}/tcp");
    let binding = inspect
        .network_settings
        .ports
        .get(&port_key)
        .and_then(|bindings| bindings.as_ref())
        .and_then(|bindings| bindings.iter().find(|binding| binding.host_port.is_some()))
        .ok_or_else(|| {
            Error::msg(format!(
                "managed session has no published attach port for `{port_key}`"
            ))
        })?;

    let host_port = binding
        .host_port
        .as_deref()
        .ok_or_else(|| Error::msg(format!("missing host port for `{port_key}`")))?
        .parse::<u16>()
        .map_err(|error| Error::msg(format!("malformed published host port: {error}")))?;
    let host_ip = binding
        .host_ip
        .as_deref()
        .filter(|host_ip| !host_ip.trim().is_empty())
        .unwrap_or(DEFAULT_HOST_ATTACH_IP)
        .to_string();

    Ok(AttachEndpoint {
        scheme: attach_labels.scheme().to_string(),
        host_ip,
        host_port,
    })
}

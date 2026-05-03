// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::spec::{AttachEndpoint, RuntimeAttachSpec, RuntimeCommand};

#[derive(Debug, Clone, Copy)]
pub(super) struct ServerCommandTemplate {
    args: &'static [ServerCommandArg],
}

impl ServerCommandTemplate {
    pub(super) const fn new(args: &'static [ServerCommandArg]) -> Self {
        Self { args }
    }

    pub(super) fn render(self, attach: RuntimeAttachSpec) -> RuntimeCommand {
        RuntimeCommand {
            argv: self.args.iter().map(|arg| arg.render(attach)).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ServerCommandArg {
    Literal(&'static str),
    ContainerListenIp,
    ContainerPort,
    ContainerListenEndpoint,
}

impl ServerCommandArg {
    fn render(self, attach: RuntimeAttachSpec) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
            Self::ContainerListenIp => attach.container_listen_ip.to_string(),
            Self::ContainerPort => attach.container_port.to_string(),
            Self::ContainerListenEndpoint => attach.container_listen_endpoint(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct HostClientCommandTemplate {
    args: &'static [HostClientCommandArg],
}

impl HostClientCommandTemplate {
    pub(super) const fn new(args: &'static [HostClientCommandArg]) -> Self {
        Self { args }
    }

    pub(super) fn render(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        RuntimeCommand {
            argv: self.args.iter().map(|arg| arg.render(endpoint)).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum HostClientCommandArg {
    Literal(&'static str),
    AttachEndpoint,
}

impl HostClientCommandArg {
    fn render(self, endpoint: &AttachEndpoint) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
            Self::AttachEndpoint => endpoint.to_string(),
        }
    }
}

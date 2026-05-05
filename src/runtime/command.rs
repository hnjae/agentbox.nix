// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::spec::{AttachEndpoint, RuntimeAttachSpec, RuntimeCommand};

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeCommandTemplate<A: 'static> {
    args: &'static [A],
}

impl<A> RuntimeCommandTemplate<A> {
    pub(super) const fn new(args: &'static [A]) -> Self {
        Self { args }
    }

    fn render_with<C>(self, context: &C) -> RuntimeCommand
    where
        A: RuntimeCommandTemplateArg<C>,
    {
        RuntimeCommand {
            argv: self.args.iter().map(|arg| arg.render(context)).collect(),
        }
    }
}

trait RuntimeCommandTemplateArg<C>: Copy {
    fn render(self, context: &C) -> String;
}

pub(super) type ServerCommandTemplate = RuntimeCommandTemplate<ServerCommandArg>;

impl RuntimeCommandTemplate<ServerCommandArg> {
    pub(super) fn render(self, attach: RuntimeAttachSpec) -> RuntimeCommand {
        self.render_with(&attach)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ServerCommandArg {
    Literal(&'static str),
    ContainerListenIp,
    ContainerPort,
    ContainerListenEndpoint,
}

impl RuntimeCommandTemplateArg<RuntimeAttachSpec> for ServerCommandArg {
    fn render(self, attach: &RuntimeAttachSpec) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
            Self::ContainerListenIp => attach.container_listen_ip.to_string(),
            Self::ContainerPort => attach.container_port.to_string(),
            Self::ContainerListenEndpoint => attach.container_listen_endpoint(),
        }
    }
}

pub(super) type HostClientCommandTemplate = RuntimeCommandTemplate<HostClientCommandArg>;

impl RuntimeCommandTemplate<HostClientCommandArg> {
    pub(super) fn render(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        self.render_with(endpoint)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum HostClientCommandArg {
    Literal(&'static str),
    AttachEndpoint,
}

impl RuntimeCommandTemplateArg<AttachEndpoint> for HostClientCommandArg {
    fn render(self, endpoint: &AttachEndpoint) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
            Self::AttachEndpoint => endpoint.to_string(),
        }
    }
}

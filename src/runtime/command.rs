// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

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

pub(super) type DirectCommandTemplate = RuntimeCommandTemplate<DirectCommandArg>;

impl RuntimeCommandTemplate<DirectCommandArg> {
    pub(super) fn render(self) -> RuntimeCommand {
        self.render_with(&())
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum DirectCommandArg {
    Literal(&'static str),
}

impl RuntimeCommandTemplateArg<()> for DirectCommandArg {
    fn render(self, (): &()) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
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

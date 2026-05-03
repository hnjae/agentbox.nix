// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;

use super::command::{
    HostClientCommandArg, HostClientCommandTemplate, ServerCommandArg, ServerCommandTemplate,
};
use super::default_image::{self, DefaultImageBuildContext};
use super::kind::RuntimeKind;
use super::spec::RuntimeAttachSpec;

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";

const OPENCODE_SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("opencode"),
    ServerCommandArg::Literal("serve"),
    ServerCommandArg::Literal("--port"),
    ServerCommandArg::ContainerPort,
]);
const OPENCODE_HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("opencode"),
    HostClientCommandArg::Literal("attach"),
    HostClientCommandArg::AttachEndpoint,
]);

const CODEX_SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("codex"),
    ServerCommandArg::Literal("--dangerously-bypass-approvals-and-sandbox"),
    ServerCommandArg::Literal("app-server"),
    ServerCommandArg::Literal("--listen"),
    ServerCommandArg::ContainerListenEndpoint,
]);
const CODEX_HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("codex"),
    HostClientCommandArg::Literal("--remote"),
    HostClientCommandArg::AttachEndpoint,
]);

const RUNTIME_PROFILES: &[RuntimeProfile] = &[
    RuntimeProfile {
        kind: RuntimeKind::Opencode,
        name: "opencode",
        default_image: default_image::OPENCODE_DEFAULT_IMAGE,
        materialize_default_image_context: default_image::materialize_default_image_context,
        attach: RuntimeAttachSpec {
            scheme: "http",
            container_listen_ip: CONTAINER_LISTEN_IP,
            container_port: 4096,
        },
        server_command: OPENCODE_SERVER_COMMAND,
        host_client_command: OPENCODE_HOST_CLIENT_COMMAND,
    },
    RuntimeProfile {
        kind: RuntimeKind::Codex,
        name: "codex",
        default_image: default_image::CODEX_DEFAULT_IMAGE,
        materialize_default_image_context: default_image::materialize_default_image_context,
        attach: RuntimeAttachSpec {
            scheme: "ws",
            container_listen_ip: CONTAINER_LISTEN_IP,
            container_port: 1455,
        },
        server_command: CODEX_SERVER_COMMAND,
        host_client_command: CODEX_HOST_CLIENT_COMMAND,
    },
];

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeProfile {
    pub(super) kind: RuntimeKind,
    pub(super) name: &'static str,
    pub(super) default_image: &'static str,
    pub(super) materialize_default_image_context: fn() -> Result<DefaultImageBuildContext>,
    pub(super) attach: RuntimeAttachSpec,
    pub(super) server_command: ServerCommandTemplate,
    pub(super) host_client_command: HostClientCommandTemplate,
}

pub(super) fn runtime_profile(kind: RuntimeKind) -> &'static RuntimeProfile {
    RUNTIME_PROFILES
        .iter()
        .find(|profile| profile.kind == kind)
        .expect("every RuntimeKind must have a RuntimeProfile")
}

pub(super) fn runtime_kind_from_name(value: &str) -> Option<RuntimeKind> {
    RUNTIME_PROFILES
        .iter()
        .find(|profile| profile.name == value)
        .map(|profile| profile.kind)
}

pub(super) fn supported_runtime_names() -> String {
    backticked_runtime_names().join(" and ")
}

pub(super) fn supported_runtime_placeholder() -> String {
    format!("<{}>", runtime_names().join("|"))
}

pub(super) fn supported_runtime_values() -> Vec<&'static str> {
    runtime_names()
}

fn runtime_names() -> Vec<&'static str> {
    RUNTIME_PROFILES
        .iter()
        .map(|profile| profile.name)
        .collect()
}

fn backticked_runtime_names() -> Vec<String> {
    runtime_names()
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
}

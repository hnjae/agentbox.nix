// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;

use super::{
    AttachEndpoint, RuntimeCommand, RuntimeKind,
    default_image::{self, DefaultImageBuildContext},
};

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";

const RUNTIME_PROFILES: &[RuntimeProfile] = &[
    RuntimeProfile {
        kind: RuntimeKind::Opencode,
        name: "opencode",
        default_image: default_image::OPENCODE_DEFAULT_IMAGE,
        materialize_default_image_context: default_image::materialize_default_image_context,
        attach_scheme: "http",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 4096,
        server_command: opencode_server_command,
        host_client_command: opencode_host_client_command,
    },
    RuntimeProfile {
        kind: RuntimeKind::Codex,
        name: "codex",
        default_image: default_image::CODEX_DEFAULT_IMAGE,
        materialize_default_image_context: default_image::materialize_default_image_context,
        attach_scheme: "ws",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 1455,
        server_command: codex_server_command,
        host_client_command: codex_host_client_command,
    },
];

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeProfile {
    pub(super) kind: RuntimeKind,
    pub(super) name: &'static str,
    pub(super) default_image: &'static str,
    pub(super) materialize_default_image_context: fn() -> Result<DefaultImageBuildContext>,
    pub(super) attach_scheme: &'static str,
    pub(super) container_listen_ip: &'static str,
    pub(super) container_port: u16,
    pub(super) server_command: fn(&RuntimeProfile) -> RuntimeCommand,
    pub(super) host_client_command: fn(&RuntimeProfile, &AttachEndpoint) -> RuntimeCommand,
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

fn opencode_server_command(profile: &RuntimeProfile) -> RuntimeCommand {
    RuntimeCommand {
        argv: vec![
            "opencode".to_string(),
            "serve".to_string(),
            "--port".to_string(),
            profile.container_port.to_string(),
        ],
    }
}

fn codex_server_command(profile: &RuntimeProfile) -> RuntimeCommand {
    RuntimeCommand {
        argv: vec![
            "codex".to_string(),
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "app-server".to_string(),
            "--listen".to_string(),
            format!(
                "{}://{}:{}",
                profile.attach_scheme, profile.container_listen_ip, profile.container_port
            ),
        ],
    }
}

fn opencode_host_client_command(
    _profile: &RuntimeProfile,
    endpoint: &AttachEndpoint,
) -> RuntimeCommand {
    RuntimeCommand {
        argv: vec![
            "opencode".to_string(),
            "attach".to_string(),
            endpoint.to_string(),
        ],
    }
}

fn codex_host_client_command(
    _profile: &RuntimeProfile,
    endpoint: &AttachEndpoint,
) -> RuntimeCommand {
    RuntimeCommand {
        argv: vec![
            "codex".to_string(),
            "--remote".to_string(),
            endpoint.to_string(),
        ],
    }
}

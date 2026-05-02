// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;

use crate::preflight::NIX_CACHE_DESTINATION;
use crate::session::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

pub mod opencode;

pub const DEFAULT_HOST_ATTACH_IP: &str = "127.0.0.1";

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";
const CODEX_DEFAULT_IMAGE: &str = "localhost/agentbox-codex:local";

const RUNTIME_PROFILES: &[RuntimeProfile] = &[
    RuntimeProfile {
        kind: RuntimeKind::Opencode,
        name: "opencode",
        default_image: opencode::DEFAULT_IMAGE,
        attach_scheme: "http",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 4096,
        server_command: opencode_server_command,
        host_client_command: opencode_host_client_command,
    },
    RuntimeProfile {
        kind: RuntimeKind::Codex,
        name: "codex",
        default_image: CODEX_DEFAULT_IMAGE,
        attach_scheme: "ws",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 1455,
        server_command: codex_server_command,
        host_client_command: codex_host_client_command,
    },
];

#[derive(Debug, Clone, Copy)]
struct RuntimeProfile {
    kind: RuntimeKind,
    name: &'static str,
    default_image: &'static str,
    attach_scheme: &'static str,
    container_listen_ip: &'static str,
    container_port: u16,
    server_command: fn(&RuntimeProfile) -> RuntimeCommand,
    host_client_command: fn(&RuntimeProfile, &AttachEndpoint) -> RuntimeCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMountKind {
    Bind,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMount {
    pub kind: RuntimeMountKind,
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecSpec {
    pub argv: Vec<String>,
    pub detached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCommand {
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCreateSpec {
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub mounts: Vec<RuntimeMount>,
    pub command: Vec<String>,
    pub default_env: BTreeMap<String, String>,
    pub network_enabled: bool,
    pub published_ports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachEndpoint {
    pub scheme: String,
    pub host_ip: String,
    pub host_port: u16,
}

impl fmt::Display for AttachEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}://{}:{}",
            self.scheme, self.host_ip, self.host_port
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum RuntimeKind {
    Opencode,
    Codex,
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        runtime_profile(self).name
    }

    pub fn adapter(self) -> RuntimeAdapter {
        RuntimeAdapter { kind: self }
    }
}

impl fmt::Display for RuntimeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RuntimeKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.trim().is_empty() {
            return Err(Error::msg(
                "malformed runtime label: `io.agentbox.runtime` is empty",
            ));
        }

        RUNTIME_PROFILES
            .iter()
            .find(|profile| profile.name == value)
            .map(|profile| profile.kind)
            .ok_or_else(|| {
                Error::msg(format!(
                    "unsupported runtime `{value}`; supported runtimes are {}",
                    supported_runtime_names()
                ))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAdapter {
    kind: RuntimeKind,
}

impl RuntimeAdapter {
    pub fn new(kind: RuntimeKind) -> Self {
        Self { kind }
    }

    pub fn kind(self) -> RuntimeKind {
        self.kind
    }

    pub fn name(self) -> &'static str {
        self.kind.as_str()
    }

    pub fn default_image(self) -> &'static str {
        self.profile().default_image
    }

    pub fn attach_scheme(self) -> &'static str {
        self.profile().attach_scheme
    }

    pub fn container_listen_ip(self) -> &'static str {
        self.profile().container_listen_ip
    }

    pub fn container_port(self) -> u16 {
        self.profile().container_port
    }

    pub fn server_command(self) -> RuntimeCommand {
        let profile = self.profile();
        (profile.server_command)(profile)
    }

    pub fn host_client_command(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        let profile = self.profile();
        (profile.host_client_command)(profile, endpoint)
    }

    pub fn create_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
    ) -> RuntimeCreateSpec {
        let image = self.default_image().to_string();
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string());
        labels.insert(LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string());
        labels.insert(
            LABEL_GIT_ROOT.to_string(),
            workspace.canonical_git_root.to_string(),
        );
        labels.insert(LABEL_GIT_ROOT_HASH.to_string(), workspace.hash12.clone());
        labels.insert(LABEL_RUNTIME.to_string(), self.name().to_string());
        labels.insert(LABEL_IMAGE.to_string(), image.clone());
        labels.insert(
            LABEL_LOGICAL_NAME.to_string(),
            workspace.container_name.clone(),
        );
        labels.insert(
            LABEL_ATTACH_SCHEME.to_string(),
            self.attach_scheme().to_string(),
        );
        labels.insert(
            LABEL_CONTAINER_PORT.to_string(),
            self.container_port().to_string(),
        );
        labels.insert(
            LABEL_CONTAINER_LISTEN_IP.to_string(),
            self.container_listen_ip().to_string(),
        );

        let mut mounts = vec![RuntimeMount {
            kind: RuntimeMountKind::Bind,
            source: workspace.canonical_git_root.to_string(),
            destination: workspace.canonical_git_root.to_string(),
            read_only: false,
        }];
        mounts.push(RuntimeMount {
            kind: RuntimeMountKind::Volume,
            source: workspace.container_name.clone(),
            destination: NIX_CACHE_DESTINATION.to_string(),
            read_only: false,
        });
        mounts.extend(host_nix_mounts.iter().cloned());

        RuntimeCreateSpec {
            image,
            labels,
            mounts,
            command: self.server_command().argv,
            default_env: BTreeMap::new(),
            network_enabled: true,
            published_ports: vec![format!(
                "{}::{}",
                DEFAULT_HOST_ATTACH_IP,
                self.container_port()
            )],
        }
    }

    fn profile(self) -> &'static RuntimeProfile {
        runtime_profile(self.kind)
    }
}

fn runtime_profile(kind: RuntimeKind) -> &'static RuntimeProfile {
    RUNTIME_PROFILES
        .iter()
        .find(|profile| profile.kind == kind)
        .expect("every RuntimeKind must have a RuntimeProfile")
}

fn supported_runtime_names() -> String {
    RUNTIME_PROFILES
        .iter()
        .map(|profile| format!("`{}`", profile.name))
        .collect::<Vec<_>>()
        .join(" and ")
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

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

const OPENCODE_CONTAINER_PORT: u16 = 4096;
const CODEX_CONTAINER_PORT: u16 = 1455;
const CONTAINER_LISTEN_IP: &str = "0.0.0.0";
const CODEX_DEFAULT_IMAGE: &str = "localhost/agentbox-codex:local";

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
        match self {
            Self::Opencode => "opencode",
            Self::Codex => "codex",
        }
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
        match value {
            "opencode" => Ok(Self::Opencode),
            "codex" => Ok(Self::Codex),
            other if other.trim().is_empty() => Err(Error::msg(
                "malformed runtime label: `io.agentbox.runtime` is empty",
            )),
            other => Err(Error::msg(format!(
                "unsupported runtime `{other}`; supported runtimes are `opencode` and `codex`"
            ))),
        }
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
        match self.kind {
            RuntimeKind::Opencode => opencode::DEFAULT_IMAGE,
            RuntimeKind::Codex => CODEX_DEFAULT_IMAGE,
        }
    }

    pub fn attach_scheme(self) -> &'static str {
        match self.kind {
            RuntimeKind::Opencode => "http",
            RuntimeKind::Codex => "ws",
        }
    }

    pub fn container_listen_ip(self) -> &'static str {
        CONTAINER_LISTEN_IP
    }

    pub fn container_port(self) -> u16 {
        match self.kind {
            RuntimeKind::Opencode => OPENCODE_CONTAINER_PORT,
            RuntimeKind::Codex => CODEX_CONTAINER_PORT,
        }
    }

    pub fn server_command(self) -> RuntimeCommand {
        let port = self.container_port().to_string();
        let argv = match self.kind {
            RuntimeKind::Opencode => vec![
                "opencode".to_string(),
                "serve".to_string(),
                "--port".to_string(),
                port,
            ],
            RuntimeKind::Codex => vec![
                "codex".to_string(),
                "--dangerously-bypass-approvals-and-sandbox".to_string(),
                "app-server".to_string(),
                "--listen".to_string(),
                format!("ws://{}:{port}", self.container_listen_ip()),
            ],
        };

        RuntimeCommand { argv }
    }

    pub fn host_client_command(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        let endpoint = endpoint.to_string();
        let argv = match self.kind {
            RuntimeKind::Opencode => vec!["opencode".to_string(), "attach".to_string(), endpoint],
            RuntimeKind::Codex => vec!["codex".to_string(), "--remote".to_string(), endpoint],
        };

        RuntimeCommand { argv }
    }

    pub fn create_spec(
        self,
        workspace: &WorkspaceIdentity,
        image: Option<&str>,
        host_nix_mounts: &[RuntimeMount],
    ) -> RuntimeCreateSpec {
        let image = image.unwrap_or(self.default_image()).to_string();
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
}

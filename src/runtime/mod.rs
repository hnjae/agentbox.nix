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

use default_image::DefaultImageBuildContext;

use crate::metadata::managed_session_labels;
use crate::preflight::NIX_CACHE_DESTINATION;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

pub mod default_image;
mod profile;

pub const DEFAULT_HOST_ATTACH_IP: &str = "127.0.0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMountKind {
    Bind,
    Volume,
}

impl RuntimeMountKind {
    pub(crate) fn podman_type(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Volume => "volume",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMount {
    pub kind: RuntimeMountKind,
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}

impl RuntimeMount {
    pub fn bind(source: impl Into<String>, destination: impl Into<String>) -> Self {
        Self::new(RuntimeMountKind::Bind, source, destination, false)
    }

    pub fn read_only_bind(source: impl Into<String>, destination: impl Into<String>) -> Self {
        Self::new(RuntimeMountKind::Bind, source, destination, true)
    }

    pub fn volume(source: impl Into<String>, destination: impl Into<String>) -> Self {
        Self::new(RuntimeMountKind::Volume, source, destination, false)
    }

    fn new(
        kind: RuntimeMountKind,
        source: impl Into<String>,
        destination: impl Into<String>,
        read_only: bool,
    ) -> Self {
        Self {
            kind,
            source: source.into(),
            destination: destination.into(),
            read_only,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAttachSpec {
    pub scheme: &'static str,
    pub container_listen_ip: &'static str,
    pub container_port: u16,
}

impl RuntimeAttachSpec {
    pub fn container_listen_endpoint(self) -> String {
        format!(
            "{}://{}:{}",
            self.scheme, self.container_listen_ip, self.container_port
        )
    }

    pub fn published_port(self, host_ip: &str) -> String {
        format!("{host_ip}::{}", self.container_port)
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
        profile::runtime_profile(self).name
    }

    pub fn supported_values_placeholder() -> String {
        profile::supported_runtime_placeholder()
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

        profile::runtime_kind_from_name(value).ok_or_else(|| {
            Error::msg(format!(
                "unsupported runtime `{value}`; supported runtimes are {}",
                profile::supported_runtime_names()
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

    pub fn materialize_default_image_context(self) -> Result<DefaultImageBuildContext> {
        let profile = self.profile();
        (profile.materialize_default_image_context)()
    }

    pub fn attach_spec(self) -> RuntimeAttachSpec {
        self.profile().attach
    }

    pub fn attach_scheme(self) -> &'static str {
        self.attach_spec().scheme
    }

    pub fn container_listen_ip(self) -> &'static str {
        self.attach_spec().container_listen_ip
    }

    pub fn container_port(self) -> u16 {
        self.attach_spec().container_port
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
        let attach = self.attach_spec();
        let labels = managed_session_labels(
            workspace,
            &image,
            self.name(),
            attach.scheme,
            attach.container_port,
            attach.container_listen_ip,
        );

        let mut mounts = vec![RuntimeMount::bind(
            workspace.canonical_git_root.to_string(),
            workspace.canonical_git_root.to_string(),
        )];
        mounts.push(RuntimeMount::volume(
            workspace.container_name.clone(),
            NIX_CACHE_DESTINATION,
        ));
        mounts.extend(host_nix_mounts.iter().cloned());

        RuntimeCreateSpec {
            image,
            labels,
            mounts,
            command: self.server_command().argv,
            default_env: BTreeMap::new(),
            network_enabled: true,
            published_ports: vec![attach.published_port(DEFAULT_HOST_ATTACH_IP)],
        }
    }

    fn profile(self) -> &'static profile::RuntimeProfile {
        profile::runtime_profile(self.kind)
    }
}

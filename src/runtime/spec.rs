// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::metadata::{ManagedSessionLabelInput, managed_session_labels};
use crate::preflight::NIX_CACHE_DESTINATION;
use crate::workspace::WorkspaceIdentity;

use super::kind::RuntimeKind;

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
pub struct RuntimeInvocation {
    argv: Vec<String>,
    workdir: Utf8PathBuf,
}

impl RuntimeInvocation {
    pub fn new(argv: impl Into<Vec<String>>, workdir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            argv: argv.into(),
            workdir: workdir.into(),
        }
    }

    pub(crate) fn argv(&self) -> &[String] {
        &self.argv
    }

    pub(crate) fn into_parts(self) -> (Vec<String>, Utf8PathBuf) {
        (self.argv, self.workdir)
    }

    pub(crate) fn workdir(&self) -> &Utf8Path {
        &self.workdir
    }
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
pub struct RuntimeRunSpec {
    create: RuntimeCreateSpec,
    workdir: Utf8PathBuf,
}

impl RuntimeRunSpec {
    pub(crate) fn new(
        create: RuntimeCreateSpec,
        workdir: impl Into<Utf8PathBuf>,
    ) -> RuntimeRunSpec {
        Self {
            create,
            workdir: workdir.into(),
        }
    }

    pub fn create(&self) -> &RuntimeCreateSpec {
        &self.create
    }

    pub(crate) fn create_mut(&mut self) -> &mut RuntimeCreateSpec {
        &mut self.create
    }

    pub(crate) fn workdir(&self) -> &Utf8Path {
        &self.workdir
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeHealthCheck {
    pub(crate) path: &'static str,
    pub(crate) response_policy: RuntimeHealthResponsePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeHealthResponsePolicy {
    HttpOk,
    JsonHealthyFlag,
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

impl RuntimeKind {
    pub fn run_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        invocation: RuntimeInvocation,
    ) -> RuntimeRunSpec {
        let (command, workdir) = invocation.into_parts();
        RuntimeRunSpec::new(
            self.create_spec(workspace, host_nix_mounts, runtime_mounts, command),
            workdir,
        )
    }

    pub fn foreground_run_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        invocation: RuntimeInvocation,
    ) -> RuntimeRunSpec {
        let (command, workdir) = invocation.into_parts();
        RuntimeRunSpec::new(
            self.foreground_create_spec(workspace, host_nix_mounts, runtime_mounts, command),
            workdir,
        )
    }

    pub fn transient_server_run_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        invocation: RuntimeInvocation,
    ) -> RuntimeRunSpec {
        let (command, workdir) = invocation.into_parts();
        RuntimeRunSpec::new(
            self.transient_server_create_spec(workspace, host_nix_mounts, runtime_mounts, command),
            workdir,
        )
    }

    fn create_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        command: impl Into<Vec<String>>,
    ) -> RuntimeCreateSpec {
        let image = self.default_image();
        let attach = self.attach_spec();
        let labels = managed_session_labels(ManagedSessionLabelInput::from_workspace(
            workspace, &image, self,
        ));

        RuntimeCreateSpec {
            image,
            labels,
            mounts: runtime_mounts_for_workspace(workspace, host_nix_mounts, runtime_mounts),
            command: command.into(),
            default_env: self.default_env(),
            network_enabled: true,
            published_ports: vec![attach.published_port(DEFAULT_HOST_ATTACH_IP)],
        }
    }

    fn foreground_create_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        command: impl Into<Vec<String>>,
    ) -> RuntimeCreateSpec {
        RuntimeCreateSpec {
            image: self.default_image(),
            labels: BTreeMap::new(),
            mounts: runtime_mounts_for_workspace(workspace, host_nix_mounts, runtime_mounts),
            command: command.into(),
            default_env: self.default_env(),
            network_enabled: true,
            published_ports: Vec::new(),
        }
    }

    fn transient_server_create_spec(
        self,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        command: impl Into<Vec<String>>,
    ) -> RuntimeCreateSpec {
        let attach = self.attach_spec();

        RuntimeCreateSpec {
            image: self.default_image(),
            labels: BTreeMap::new(),
            mounts: runtime_mounts_for_workspace(workspace, host_nix_mounts, runtime_mounts),
            command: command.into(),
            default_env: self.default_env(),
            network_enabled: true,
            published_ports: vec![attach.published_port(DEFAULT_HOST_ATTACH_IP)],
        }
    }

    fn default_env(self) -> BTreeMap<String, String> {
        self.profile()
            .default_env
            .iter()
            .map(|entry| (entry.name.to_string(), entry.value.to_string()))
            .collect()
    }
}

fn runtime_mounts_for_workspace(
    workspace: &WorkspaceIdentity,
    host_nix_mounts: &[RuntimeMount],
    runtime_mounts: &[RuntimeMount],
) -> Vec<RuntimeMount> {
    let mut mounts = vec![RuntimeMount::bind(
        workspace.canonical_git_root.to_string(),
        workspace.canonical_git_root.to_string(),
    )];
    mounts.push(RuntimeMount::volume(
        workspace.container_name.clone(),
        NIX_CACHE_DESTINATION,
    ));
    mounts.extend(host_nix_mounts.iter().cloned());
    mounts.extend(runtime_mounts.iter().cloned());
    mounts
}

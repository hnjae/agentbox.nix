// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::config::ResourceLimits;
use crate::metadata::{ManagedSessionLabelInput, managed_session_labels, transient_run_labels};
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
    pub fn is_volume(self) -> bool {
        matches!(self, Self::Volume)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMount {
    kind: RuntimeMountKind,
    source: String,
    destination: String,
    read_only: bool,
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

    pub fn kind(&self) -> RuntimeMountKind {
        self.kind
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn is_volume(&self) -> bool {
        self.kind.is_volume()
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
    image: String,
    labels: BTreeMap<String, String>,
    mounts: Vec<RuntimeMount>,
    command: Vec<String>,
    default_env: BTreeMap<String, String>,
    network_enabled: bool,
    published_ports: Vec<String>,
    resource_limits: ResourceLimits,
}

impl RuntimeCreateSpec {
    pub fn builder(image: impl Into<String>) -> RuntimeCreateSpecBuilder {
        RuntimeCreateSpecBuilder::new(image)
    }

    pub fn image(&self) -> &str {
        &self.image
    }

    pub fn labels(&self) -> &BTreeMap<String, String> {
        &self.labels
    }

    pub fn mounts(&self) -> &[RuntimeMount] {
        &self.mounts
    }

    pub fn command(&self) -> &[String] {
        &self.command
    }

    pub fn default_env(&self) -> &BTreeMap<String, String> {
        &self.default_env
    }

    pub fn network_enabled(&self) -> bool {
        self.network_enabled
    }

    pub fn published_ports(&self) -> &[String] {
        &self.published_ports
    }

    pub fn resource_limits(&self) -> &ResourceLimits {
        &self.resource_limits
    }

    pub(crate) fn logical_name(&self) -> Option<&str> {
        crate::metadata::required_label_value(&self.labels, crate::metadata::LABEL_LOGICAL_NAME)
    }

    fn insert_label(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.labels.insert(name.into(), value.into());
    }

    fn add_mount(&mut self, mount: RuntimeMount) {
        self.mounts.push(mount);
    }

    fn extend_default_env(&mut self, env: BTreeMap<String, String>) {
        self.default_env.extend(env);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCreateSpecBuilder {
    image: String,
    labels: BTreeMap<String, String>,
    mounts: Vec<RuntimeMount>,
    command: Vec<String>,
    default_env: BTreeMap<String, String>,
    network_enabled: bool,
    published_ports: Vec<String>,
    resource_limits: ResourceLimits,
}

impl RuntimeCreateSpecBuilder {
    fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            labels: BTreeMap::new(),
            mounts: Vec::new(),
            command: Vec::new(),
            default_env: BTreeMap::new(),
            network_enabled: true,
            published_ports: Vec::new(),
            resource_limits: ResourceLimits::default(),
        }
    }

    pub fn labels(mut self, labels: BTreeMap<String, String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn mounts(mut self, mounts: Vec<RuntimeMount>) -> Self {
        self.mounts = mounts;
        self
    }

    pub fn command(mut self, command: impl Into<Vec<String>>) -> Self {
        self.command = command.into();
        self
    }

    pub fn default_env(mut self, default_env: BTreeMap<String, String>) -> Self {
        self.default_env = default_env;
        self
    }

    pub fn network_enabled(mut self, network_enabled: bool) -> Self {
        self.network_enabled = network_enabled;
        self
    }

    pub fn published_ports(mut self, published_ports: Vec<String>) -> Self {
        self.published_ports = published_ports;
        self
    }

    pub fn resource_limits(mut self, resource_limits: ResourceLimits) -> Self {
        self.resource_limits = resource_limits;
        self
    }

    pub fn build(self) -> RuntimeCreateSpec {
        RuntimeCreateSpec {
            image: self.image,
            labels: self.labels,
            mounts: self.mounts,
            command: self.command,
            default_env: self.default_env,
            network_enabled: self.network_enabled,
            published_ports: self.published_ports,
            resource_limits: self.resource_limits,
        }
    }
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

    pub(crate) fn workdir(&self) -> &Utf8Path {
        &self.workdir
    }

    pub(crate) fn insert_create_label(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) {
        self.create.insert_label(name, value);
    }

    pub(crate) fn add_create_mount(&mut self, mount: RuntimeMount) {
        self.create.add_mount(mount);
    }

    pub(crate) fn extend_create_default_env(&mut self, env: BTreeMap<String, String>) {
        self.create.extend_default_env(env);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRunMode {
    ManagedSession,
    TransientServer,
    Foreground,
}

impl RuntimeRunMode {
    fn publishes_attach_endpoint(self) -> bool {
        matches!(self, Self::ManagedSession | Self::TransientServer)
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

    pub fn tcp_port_key(self) -> String {
        format!("{}/tcp", self.container_port)
    }

    pub fn published_port(self, host_ip: &str) -> String {
        format!("{host_ip}::{}", self.container_port)
    }
}

impl RuntimeKind {
    #[allow(clippy::too_many_arguments)]
    pub fn run_spec(
        self,
        mode: RuntimeRunMode,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        invocation: RuntimeInvocation,
        server_args: &[String],
        resource_limits: ResourceLimits,
    ) -> RuntimeRunSpec {
        let (command, workdir) = invocation.into_parts();
        RuntimeRunSpec::new(
            self.create_spec(
                mode,
                workspace,
                host_nix_mounts,
                runtime_mounts,
                command,
                server_args,
                resource_limits,
            ),
            workdir,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_spec(
        self,
        mode: RuntimeRunMode,
        workspace: &WorkspaceIdentity,
        host_nix_mounts: &[RuntimeMount],
        runtime_mounts: &[RuntimeMount],
        command: impl Into<Vec<String>>,
        server_args: &[String],
        resource_limits: ResourceLimits,
    ) -> RuntimeCreateSpec {
        let image = self.default_image();
        let label_input =
            ManagedSessionLabelInput::from_workspace(workspace, &image, self, server_args);
        let labels = match mode {
            RuntimeRunMode::ManagedSession => managed_session_labels(label_input),
            RuntimeRunMode::TransientServer => transient_run_labels(label_input),
            RuntimeRunMode::Foreground => BTreeMap::new(),
        };
        let published_ports = if mode.publishes_attach_endpoint() {
            vec![self.attach_spec().published_port(DEFAULT_HOST_ATTACH_IP)]
        } else {
            Vec::new()
        };

        RuntimeCreateSpec::builder(image)
            .labels(labels)
            .mounts(runtime_mounts_for_workspace(
                workspace,
                host_nix_mounts,
                runtime_mounts,
            ))
            .command(command)
            .default_env(self.default_env())
            .published_ports(published_ports)
            .resource_limits(resource_limits)
            .build()
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

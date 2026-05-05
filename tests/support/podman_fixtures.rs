#![allow(dead_code)]

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use agentbox::metadata::{
    LABEL_CONTAINER_PORT, LABEL_GIT_ROOT_HASH, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    ManagedSessionLabelInput, managed_session_labels,
};
use agentbox::podman::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
    PodmanHostConfig, PodmanNetworkSettings, PodmanPortBinding, PodmanPsContainer,
};
use agentbox::runtime::{RuntimeKind, default_image::OPENCODE_DEFAULT_IMAGE};
use agentbox::session::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use agentbox::workspace::{WorkspaceIdentity, git_root_hash12};
use camino::Utf8Path;
use serde_json::Value;

const FIXTURE_CREATED_AT: &str = "2026-04-21 00:00:00 +0000 UTC";
const FIXTURE_CREATED_RFC3339: &str = "2026-04-21T00:00:00.000000000Z";
const FIXTURE_STARTED_RFC3339: &str = "2026-04-21T00:00:01.000000000Z";
const FIXTURE_HOST_IP: &str = "127.0.0.1";
const FIXTURE_HOST_PORT: u16 = 49152;
const OPENCODE_BINARY: &str = "opencode";

pub fn podman_ps_fixture() -> &'static str {
    // Keep both `Created` and `CreatedAt`: Podman emits the unix timestamp and
    // a derived display string, and callers should not need to reconstruct it.
    r#"[
  {
    "Id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "Image": "ghcr.io/example/agentbox:latest",
    "Command": null,
    "Created": 1713681300,
    "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
    "Names": null,
    "Ports": null,
    "Status": "Up 2 minutes",
    "State": "running",
    "Labels": {
      "io.containers.autoupdate": "registry"
    },
    "Mounts": null,
    "Networks": null,
    "Namespaces": null
  }
]"#
}

pub fn podman_inspect_fixture() -> &'static str {
    // Prefer `State.Health` from JSON over legacy template aliases.
    r#"[
  {
    "Id": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "Created": "2026-04-21T10:15:00.000000000Z",
    "Path": "/usr/bin/opencode",
    "Args": [],
    "State": {
      "Status": "running",
      "Running": true,
      "ExitCode": 0,
      "Pid": 4321,
      "StartedAt": "2026-04-21T10:15:01.000000000Z",
      "FinishedAt": "0001-01-01T00:00:00Z",
      "Health": {
        "Status": "healthy",
        "FailingStreak": 0
      }
    },
    "ImageName": "ghcr.io/example/agentbox:latest",
    "Config": {
      "User": "agent",
      "Env": ["A=1", "B=2"],
      "Cmd": ["opencode"],
      "WorkingDir": "/workspace",
      "Labels": {
        "com.example.role": "agentbox"
      },
      "Entrypoint": ["/entrypoint"],
      "StopSignal": "SIGTERM"
    },
    "HostConfig": {
      "AutoRemove": false,
      "NetworkMode": "bridge",
      "Privileged": false
    },
    "Mounts": [
      {
        "Type": "bind",
        "Source": "/tmp/workspace",
        "Destination": "/workspace",
        "RW": true
      }
    ],
    "NetworkSettings": {
      "Networks": {
        "podman": {
          "IPAddress": "10.88.0.10",
          "Gateway": "10.88.0.1",
          "Aliases": ["agentbox-demo"]
        }
      }
    }
  }
]"#
}

pub fn ps_fixture(entries: Vec<Value>) -> String {
    serde_json::to_string(&entries).unwrap()
}

pub fn managed_ps_entry(id: &str, name: &str, git_root_hash: &str) -> Value {
    serde_json::to_value(managed_ps_model(id, name, git_root_hash, true)).unwrap()
}

pub fn workspace_ps_entry(id: &str, workspace: &WorkspaceIdentity) -> Value {
    managed_ps_entry(id, &workspace.container_name, &workspace.hash12)
}

pub fn managed_container_models(
    name: &str,
    root: &Utf8Path,
    running: bool,
    include_cache_mount: bool,
) -> (PodmanPsContainer, PodmanContainerInspect) {
    managed_container_models_with_hash(
        name,
        root,
        git_root_hash12(root).as_str(),
        running,
        include_cache_mount,
    )
}

pub fn managed_container_models_with_hash(
    name: &str,
    root: &Utf8Path,
    git_root_hash: &str,
    running: bool,
    include_cache_mount: bool,
) -> (PodmanPsContainer, PodmanContainerInspect) {
    let inspect_labels = opencode_managed_labels(root.as_str(), git_root_hash, name);
    let id = format!("{name}-id");

    (
        managed_ps_model(&id, name, git_root_hash, running),
        ManagedInspectFixture::new(name, root.as_str(), inspect_labels)
            .id(&id)
            .running(running)
            .include_cache_mount(include_cache_mount)
            .build(),
    )
}

fn managed_ps_model(id: &str, name: &str, git_root_hash: &str, running: bool) -> PodmanPsContainer {
    PodmanPsContainer {
        id: id.to_string(),
        image: OPENCODE_DEFAULT_IMAGE.to_string(),
        command: Some(vec![OPENCODE_BINARY.to_string()]),
        created: 1713681300,
        created_at: FIXTURE_CREATED_AT.to_string(),
        names: Some(vec![name.to_string()]),
        ports: Some(Vec::new()),
        status: if running {
            "Up 2 minutes".to_string()
        } else {
            "Exited (137) 1 minute ago".to_string()
        },
        state: if running {
            "running".to_string()
        } else {
            "exited".to_string()
        },
        labels: BTreeMap::from([
            (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
            (LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string()),
        ]),
        mounts: Some(Vec::new()),
        networks: Some(vec!["podman".to_string()]),
        namespaces: None,
    }
}

#[derive(Debug, Clone)]
struct ManagedInspectFixture {
    id: String,
    container_name: String,
    git_root: String,
    image: String,
    running: bool,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
    command: Vec<String>,
    working_dir: String,
    auto_remove: bool,
    path: String,
    host_port: u16,
}

impl ManagedInspectFixture {
    fn new(container_name: &str, git_root: &str, labels: BTreeMap<String, String>) -> Self {
        Self {
            id: container_name.to_string(),
            container_name: container_name.to_string(),
            git_root: git_root.to_string(),
            image: OPENCODE_DEFAULT_IMAGE.to_string(),
            running: true,
            include_cache_mount: true,
            labels,
            command: vec![OPENCODE_BINARY.to_string()],
            working_dir: git_root.to_string(),
            auto_remove: false,
            path: format!("/usr/bin/{OPENCODE_BINARY}"),
            host_port: FIXTURE_HOST_PORT,
        }
    }

    fn id(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    fn image(mut self, image: &str) -> Self {
        self.image = image.to_string();
        self
    }

    fn running(mut self, running: bool) -> Self {
        self.running = running;
        self
    }

    fn include_cache_mount(mut self, include_cache_mount: bool) -> Self {
        self.include_cache_mount = include_cache_mount;
        self
    }

    fn command(mut self, command: Vec<String>) -> Self {
        self.command = command;
        self
    }

    fn working_dir(mut self, working_dir: &str) -> Self {
        self.working_dir = working_dir.to_string();
        self
    }

    fn auto_remove(mut self, auto_remove: bool) -> Self {
        self.auto_remove = auto_remove;
        self
    }

    fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    fn host_port(mut self, host_port: u16) -> Self {
        self.host_port = host_port;
        self
    }

    fn build(self) -> PodmanContainerInspect {
        PodmanContainerInspect {
            id: self.id,
            created: FIXTURE_CREATED_RFC3339.to_string(),
            path: self.path,
            args: Vec::new(),
            state: container_state(self.running),
            image_name: self.image,
            config: PodmanContainerConfig {
                user: Some("user".to_string()),
                env: Vec::new(),
                cmd: self.command,
                working_dir: Some(self.working_dir),
                labels: self.labels.clone(),
                entrypoint: Some(vec!["/entrypoint".to_string()]),
                stop_signal: Some("SIGTERM".to_string()),
            },
            host_config: PodmanHostConfig {
                auto_remove: self.auto_remove,
                network_mode: Some("bridge".to_string()),
                privileged: false,
            },
            mounts: managed_container_mounts(
                &self.git_root,
                &self.container_name,
                self.include_cache_mount,
            ),
            network_settings: PodmanNetworkSettings {
                networks: BTreeMap::new(),
                ports: published_ports_for_labels(&self.labels, self.host_port),
            },
        }
    }
}

fn container_state(running: bool) -> PodmanContainerState {
    PodmanContainerState {
        status: if running {
            "running".to_string()
        } else {
            "exited".to_string()
        },
        running,
        exit_code: if running { 0 } else { 137 },
        pid: if running { 4321 } else { 0 },
        started_at: Some(FIXTURE_STARTED_RFC3339.to_string()),
        finished_at: None,
        health: None,
    }
}

fn managed_container_mounts(
    git_root: &str,
    container_name: &str,
    include_cache_mount: bool,
) -> Vec<PodmanContainerMount> {
    let mut mounts = vec![PodmanContainerMount {
        kind: "bind".to_string(),
        source: git_root.to_string(),
        destination: git_root.to_string(),
        rw: true,
    }];

    if include_cache_mount {
        mounts.push(PodmanContainerMount {
            kind: "volume".to_string(),
            source: container_name.to_string(),
            destination: REQUIRED_NIX_CACHE_MOUNT_DESTINATION.to_string(),
            rw: true,
        });
    }

    mounts
}

fn published_ports_for_labels(
    labels: &BTreeMap<String, String>,
    host_port: u16,
) -> BTreeMap<String, Option<Vec<PodmanPortBinding>>> {
    BTreeMap::from([(
        port_key_for_labels(labels),
        Some(vec![PodmanPortBinding {
            host_ip: Some(FIXTURE_HOST_IP.to_string()),
            host_port: Some(host_port.to_string()),
        }]),
    )])
}

fn port_key_for_labels(labels: &BTreeMap<String, String>) -> String {
    labels
        .get(LABEL_CONTAINER_PORT)
        .map(|port| format!("{port}/tcp"))
        .unwrap_or_else(|| "4096/tcp".to_string())
}

fn inspect_fixture_json(inspect: PodmanContainerInspect) -> String {
    serde_json::to_string(&vec![inspect]).unwrap()
}

pub fn inspect_models_by_id(
    inspects: Vec<PodmanContainerInspect>,
) -> impl FnMut(&str) -> agentbox::Result<PodmanContainerInspect> {
    let mut inspects = inspects
        .into_iter()
        .map(|inspect| (inspect.id.clone(), inspect))
        .collect::<BTreeMap<_, _>>();

    move |container_id| {
        inspects.remove(container_id).ok_or_else(|| {
            agentbox::Error::msg(format!("missing inspect fixture for `{container_id}`"))
        })
    }
}

pub fn managed_labels(
    git_root: &str,
    git_root_hash: &str,
    runtime: RuntimeKind,
    logical_name: &str,
) -> BTreeMap<String, String> {
    managed_labels_for_image(
        git_root,
        git_root_hash,
        runtime,
        OPENCODE_DEFAULT_IMAGE,
        logical_name,
    )
}

pub fn opencode_managed_labels(
    git_root: &str,
    git_root_hash: &str,
    logical_name: &str,
) -> BTreeMap<String, String> {
    managed_labels(git_root, git_root_hash, RuntimeKind::Opencode, logical_name)
}

pub fn opencode_workspace_labels(workspace: &WorkspaceIdentity) -> BTreeMap<String, String> {
    opencode_managed_labels(
        workspace.canonical_git_root.as_str(),
        &workspace.hash12,
        &workspace.container_name,
    )
}

pub fn managed_labels_for_image(
    git_root: &str,
    git_root_hash: &str,
    runtime: RuntimeKind,
    image: &str,
    logical_name: &str,
) -> BTreeMap<String, String> {
    managed_labels_with_launch_directory(
        git_root,
        git_root,
        git_root_hash,
        runtime,
        image,
        logical_name,
    )
}

fn workspace_managed_labels(
    workspace: &WorkspaceIdentity,
    image: &str,
    runtime: RuntimeKind,
) -> BTreeMap<String, String> {
    managed_session_labels(ManagedSessionLabelInput::from_workspace(
        workspace, image, runtime,
    ))
}

fn managed_labels_with_launch_directory(
    git_root: &str,
    launch_directory: &str,
    git_root_hash: &str,
    runtime: RuntimeKind,
    image: &str,
    logical_name: &str,
) -> BTreeMap<String, String> {
    managed_session_labels(ManagedSessionLabelInput {
        canonical_git_root: git_root,
        git_root_hash,
        runtime,
        image,
        launch_directory,
        logical_name,
    })
}

pub fn managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
) -> String {
    inspect_fixture_json(
        ManagedInspectFixture::new(container_name, git_root, labels)
            .running(running)
            .include_cache_mount(include_cache_mount)
            .build(),
    )
}

pub fn running_managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
) -> String {
    managed_inspect_fixture(container_name, git_root, true, include_cache_mount, labels)
}

pub fn opencode_workspace_inspect_fixture(
    workspace: &WorkspaceIdentity,
    running: bool,
    include_cache_mount: bool,
) -> String {
    managed_inspect_fixture(
        &workspace.container_name,
        workspace.canonical_git_root.as_str(),
        running,
        include_cache_mount,
        opencode_workspace_labels(workspace),
    )
}

pub fn running_workspace_inspect_fixture(
    workspace: &WorkspaceIdentity,
    image: &str,
    runtime: RuntimeKind,
) -> String {
    running_workspace_inspect_fixture_with_host_port(workspace, image, runtime, FIXTURE_HOST_PORT)
}

pub fn running_workspace_inspect_fixture_with_host_port(
    workspace: &WorkspaceIdentity,
    image: &str,
    runtime: RuntimeKind,
    host_port: u16,
) -> String {
    let labels = workspace_managed_labels(workspace, image, runtime);

    inspect_fixture_json(
        ManagedInspectFixture::new(
            &workspace.container_name,
            workspace.canonical_git_root.as_str(),
            labels,
        )
        .image(image)
        .command(runtime.server_command().argv)
        .working_dir(workspace.canonical_target.as_str())
        .auto_remove(true)
        .path(format!("/usr/bin/{}", runtime.as_str()))
        .host_port(host_port)
        .build(),
    )
}

pub fn cached_managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    labels: BTreeMap<String, String>,
) -> String {
    managed_inspect_fixture(container_name, git_root, running, true, labels)
}

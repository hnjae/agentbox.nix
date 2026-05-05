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
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use agentbox::podman::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
    PodmanHostConfig, PodmanNetworkSettings, PodmanPortBinding, PodmanPsContainer,
};
use agentbox::runtime::{RuntimeKind, default_image::OPENCODE_DEFAULT_IMAGE};
use agentbox::session::REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
use agentbox::workspace::{WorkspaceIdentity, hash12};
use camino::Utf8Path;
use serde_json::{Value, json};

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
    json!({
        "Id": id,
        "Image": OPENCODE_DEFAULT_IMAGE,
        "Command": ["opencode"],
        "Created": 1713681300,
        "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
        "Names": [name],
        "Ports": [],
        "Status": "Up 2 minutes",
        "State": "running",
        "Labels": {
            LABEL_MANAGED: LABEL_MANAGED_VALUE,
            LABEL_GIT_ROOT_HASH: git_root_hash,
        },
        "Mounts": [],
        "Networks": ["podman"],
        "Namespaces": null,
    })
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
        hash12(root.as_str().as_bytes()).as_str(),
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
    let ps_labels = BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string()),
    ]);
    let inspect_labels = managed_labels(root.as_str(), git_root_hash, "opencode", name);
    let mounts = if include_cache_mount {
        vec![PodmanContainerMount {
            kind: "volume".to_string(),
            source: "agentbox-cache".to_string(),
            destination: REQUIRED_NIX_CACHE_MOUNT_DESTINATION.to_string(),
            rw: true,
        }]
    } else {
        Vec::new()
    };

    (
        PodmanPsContainer {
            id: format!("{name}-id"),
            image: OPENCODE_DEFAULT_IMAGE.to_string(),
            command: Some(vec!["opencode".to_string()]),
            created: 0,
            created_at: "2026-04-21 00:00:00 +0000 UTC".to_string(),
            names: Some(vec![name.to_string()]),
            ports: Some(Vec::new()),
            status: if running {
                "Up 1 minute".to_string()
            } else {
                "Exited (0) 1 minute ago".to_string()
            },
            state: if running {
                "running".to_string()
            } else {
                "exited".to_string()
            },
            labels: ps_labels,
            mounts: Some(Vec::new()),
            networks: Some(vec!["podman".to_string()]),
            namespaces: None,
        },
        PodmanContainerInspect {
            id: format!("{name}-id"),
            created: "2026-04-21T00:00:00.000000000Z".to_string(),
            path: "/usr/bin/opencode".to_string(),
            args: Vec::new(),
            state: PodmanContainerState {
                status: if running {
                    "running".to_string()
                } else {
                    "exited".to_string()
                },
                running,
                exit_code: 0,
                pid: if running { 4321 } else { 0 },
                started_at: Some("2026-04-21T00:00:01.000000000Z".to_string()),
                finished_at: None,
                health: None,
            },
            image_name: OPENCODE_DEFAULT_IMAGE.to_string(),
            config: PodmanContainerConfig {
                user: Some("user".to_string()),
                env: Vec::new(),
                cmd: vec!["opencode".to_string()],
                working_dir: Some("/workspace".to_string()),
                labels: inspect_labels,
                entrypoint: Some(vec!["/entrypoint".to_string()]),
                stop_signal: Some("SIGTERM".to_string()),
            },
            host_config: PodmanHostConfig {
                auto_remove: false,
                network_mode: Some("bridge".to_string()),
                privileged: false,
            },
            mounts,
            network_settings: PodmanNetworkSettings {
                networks: BTreeMap::new(),
                ports: BTreeMap::from([(
                    "4096/tcp".to_string(),
                    Some(vec![PodmanPortBinding {
                        host_ip: Some("127.0.0.1".to_string()),
                        host_port: Some("49152".to_string()),
                    }]),
                )]),
            },
        },
    )
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
    runtime: &str,
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
    managed_labels(git_root, git_root_hash, "opencode", logical_name)
}

pub fn managed_labels_for_image(
    git_root: &str,
    git_root_hash: &str,
    runtime: &str,
    image: &str,
    logical_name: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (LABEL_GIT_ROOT.to_string(), git_root.to_string()),
        (LABEL_GIT_ROOT_HASH.to_string(), git_root_hash.to_string()),
        (LABEL_RUNTIME.to_string(), runtime.to_string()),
        (LABEL_IMAGE.to_string(), image.to_string()),
        (LABEL_LAUNCH_DIRECTORY.to_string(), git_root.to_string()),
        (LABEL_LOGICAL_NAME.to_string(), logical_name.to_string()),
        (LABEL_ATTACH_SCHEME.to_string(), "http".to_string()),
        (LABEL_CONTAINER_PORT.to_string(), "4096".to_string()),
        (LABEL_CONTAINER_LISTEN_IP.to_string(), "0.0.0.0".to_string()),
    ])
}

pub fn managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
) -> String {
    let mut mounts = vec![json!({
        "Type": "bind",
        "Source": git_root,
        "Destination": git_root,
        "RW": true,
    })];
    if include_cache_mount {
        mounts.push(json!({
            "Type": "volume",
            "Source": container_name,
            "Destination": REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
            "RW": true,
        }));
    }

    serde_json::to_string(&vec![json!({
        "Id": container_name,
        "Created": "2026-04-21T10:15:00.000000000Z",
        "Path": "/usr/bin/opencode",
        "Args": [],
        "State": {
            "Status": if running { "running" } else { "exited" },
            "Running": running,
            "ExitCode": if running { 0 } else { 137 },
            "Pid": if running { 4321 } else { 0 },
            "StartedAt": "2026-04-21T10:15:01.000000000Z",
            "FinishedAt": null,
            "Health": null,
        },
        "ImageName": OPENCODE_DEFAULT_IMAGE,
        "Config": {
            "User": "user",
            "Env": [],
            "Cmd": ["opencode"],
            "WorkingDir": git_root,
            "Labels": labels,
            "Entrypoint": ["/entrypoint"],
            "StopSignal": "SIGTERM",
        },
        "HostConfig": {
            "AutoRemove": false,
            "NetworkMode": "bridge",
            "Privileged": false,
        },
        "Mounts": mounts,
        "NetworkSettings": {
            "Networks": {},
            "Ports": {
                "4096/tcp": [
                    {
                        "HostIp": "127.0.0.1",
                        "HostPort": "49152"
                    }
                ]
            },
        },
    })])
    .unwrap()
}

pub fn running_managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    include_cache_mount: bool,
    labels: BTreeMap<String, String>,
) -> String {
    managed_inspect_fixture(container_name, git_root, true, include_cache_mount, labels)
}

pub fn running_workspace_inspect_fixture(
    workspace: &WorkspaceIdentity,
    image: &str,
    runtime: RuntimeKind,
) -> String {
    let attach = runtime.attach_spec();
    let labels = BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (
            LABEL_GIT_ROOT.to_string(),
            workspace.canonical_git_root.to_string(),
        ),
        (LABEL_GIT_ROOT_HASH.to_string(), workspace.hash12.clone()),
        (LABEL_RUNTIME.to_string(), runtime.as_str().to_string()),
        (LABEL_IMAGE.to_string(), image.to_string()),
        (
            LABEL_LAUNCH_DIRECTORY.to_string(),
            workspace.canonical_target.to_string(),
        ),
        (
            LABEL_LOGICAL_NAME.to_string(),
            workspace.container_name.clone(),
        ),
        (LABEL_ATTACH_SCHEME.to_string(), attach.scheme.to_string()),
        (
            LABEL_CONTAINER_PORT.to_string(),
            attach.container_port.to_string(),
        ),
        (
            LABEL_CONTAINER_LISTEN_IP.to_string(),
            attach.container_listen_ip.to_string(),
        ),
    ]);
    let command = runtime.server_command().argv;
    let ports = BTreeMap::from([(
        format!("{}/tcp", attach.container_port),
        json!([
            {
                "HostIp": "127.0.0.1",
                "HostPort": "49152"
            }
        ]),
    )]);

    serde_json::to_string(&vec![json!({
        "Id": workspace.container_name,
        "Created": "2026-04-21T10:15:00.000000000Z",
        "Path": format!("/usr/bin/{}", runtime.as_str()),
        "Args": [],
        "State": {
            "Status": "running",
            "Running": true,
            "ExitCode": 0,
            "Pid": 4321,
            "StartedAt": "2026-04-21T10:15:01.000000000Z",
            "FinishedAt": null,
            "Health": null,
        },
        "ImageName": image,
        "Config": {
            "User": "user",
            "Env": [],
            "Cmd": command,
            "WorkingDir": workspace.canonical_target.as_str(),
            "Labels": labels,
            "Entrypoint": ["/entrypoint"],
            "StopSignal": "SIGTERM",
        },
        "HostConfig": {
            "AutoRemove": true,
            "NetworkMode": "bridge",
            "Privileged": false,
        },
        "Mounts": [
            {
                "Type": "bind",
                "Source": workspace.canonical_git_root.as_str(),
                "Destination": workspace.canonical_git_root.as_str(),
                "RW": true,
            },
            {
                "Type": "volume",
                "Source": workspace.container_name,
                "Destination": REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
                "RW": true,
            }
        ],
        "NetworkSettings": {
            "Networks": {},
            "Ports": ports,
        },
    })])
    .unwrap()
}

pub fn cached_managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    labels: BTreeMap<String, String>,
) -> String {
    managed_inspect_fixture(container_name, git_root, running, true, labels)
}

#![allow(dead_code)]

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use agentbox::runtime::default_image::OPENCODE_DEFAULT_IMAGE;
use agentbox::session::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
};
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

pub fn cached_managed_inspect_fixture(
    container_name: &str,
    git_root: &str,
    running: bool,
    labels: BTreeMap<String, String>,
) -> String {
    managed_inspect_fixture(container_name, git_root, running, true, labels)
}

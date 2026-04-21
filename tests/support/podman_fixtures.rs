// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
    "Path": "/usr/bin/sleep",
    "Args": ["infinity"],
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
      "Cmd": ["infinity"],
      "WorkingDir": "/workspace",
      "Labels": {
        "com.example.role": "agentbox"
      },
      "Entrypoint": ["/usr/bin/sleep"],
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

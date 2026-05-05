// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::preflight::{
    DirenvPreflightSnapshot, HostDirectoryPreflightSnapshot, HostPreflightSnapshot,
    NixConfigPreflightSnapshot, NixCustomConfPreflightSnapshot, NixPreflightSnapshot,
    OpenCodePreflightSnapshot, PreflightSnapshot,
};

pub fn snapshot_with(configure: impl FnOnce(&mut PreflightSnapshot)) -> PreflightSnapshot {
    let mut snapshot = passing_preflight_snapshot();
    configure(&mut snapshot);
    snapshot
}

pub fn passing_preflight_snapshot_with_static_nix_mount() -> PreflightSnapshot {
    snapshot_with(|snapshot| {
        snapshot.nix.config.custom_conf = NixCustomConfPreflightSnapshot {
            present: true,
            has_readable_target: true,
            needs_static_mount: true,
        };
    })
}

fn host_directory(path: &str) -> HostDirectoryPreflightSnapshot {
    HostDirectoryPreflightSnapshot {
        source: Some(path.into()),
        exists: true,
        is_directory: true,
        readable: true,
        writable: true,
    }
}

fn passing_preflight_snapshot() -> PreflightSnapshot {
    PreflightSnapshot {
        host: HostPreflightSnapshot {
            has_git: true,
            has_podman: true,
            direnv: DirenvPreflightSnapshot {
                required: false,
                available: true,
            },
            codex: host_directory("/home/example/.codex"),
            opencode: OpenCodePreflightSnapshot {
                config: host_directory("/home/example/.config/opencode"),
                data: host_directory("/home/example/.local/share/opencode"),
            },
        },
        nix: NixPreflightSnapshot {
            has_daemon_socket: true,
            client_source: Some("/run/current-system/sw/bin/nix".into()),
            config: NixConfigPreflightSnapshot {
                has_etc_nix_mount: true,
                has_readable_nix_conf: true,
                custom_conf: NixCustomConfPreflightSnapshot {
                    present: false,
                    has_readable_target: true,
                    needs_static_mount: false,
                },
            },
        },
    }
}

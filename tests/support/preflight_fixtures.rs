// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use agentbox::preflight::{
    CODEX_CONFIG_DESTINATION, HostDirectoryPreflightSnapshot, HostPreflightSnapshot,
    NixConfigPreflightSnapshot, NixCustomConfPreflightSnapshot, NixPreflightSnapshot,
    OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION, PreflightSnapshot,
};
use agentbox::runtime::RuntimeKind;

pub fn snapshot_with(
    runtime: RuntimeKind,
    configure: impl FnOnce(&mut PreflightSnapshot),
) -> PreflightSnapshot {
    let mut snapshot = passing_preflight_snapshot(runtime);
    configure(&mut snapshot);
    snapshot
}

pub fn passing_preflight_snapshot_with_static_nix_mount(runtime: RuntimeKind) -> PreflightSnapshot {
    snapshot_with(runtime, |snapshot| {
        snapshot.nix.config.custom_conf = NixCustomConfPreflightSnapshot {
            present: true,
            has_readable_target: true,
            needs_static_mount: true,
        };
    })
}

pub fn host_state_mut<'a>(
    snapshot: &'a mut PreflightSnapshot,
    destination: &str,
) -> &'a mut HostDirectoryPreflightSnapshot {
    snapshot
        .host
        .runtime_state
        .get_mut(destination)
        .unwrap_or_else(|| panic!("missing host-state fixture for `{destination}`"))
}

fn host_directory(path: &str) -> HostDirectoryPreflightSnapshot {
    HostDirectoryPreflightSnapshot {
        source: Some(path.into()),
        exists: true,
        is_directory: true,
        readable: true,
        writable: true,
        searchable: true,
    }
}

fn passing_preflight_snapshot(runtime: RuntimeKind) -> PreflightSnapshot {
    PreflightSnapshot {
        host: HostPreflightSnapshot {
            has_git: true,
            has_podman: true,
            runtime_state: runtime_state(runtime),
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

fn runtime_state(runtime: RuntimeKind) -> BTreeMap<String, HostDirectoryPreflightSnapshot> {
    match runtime {
        RuntimeKind::Opencode => BTreeMap::from([
            (
                OPENCODE_CONFIG_DESTINATION.to_string(),
                host_directory("/home/example/.config/opencode"),
            ),
            (
                OPENCODE_DATA_DESTINATION.to_string(),
                host_directory("/home/example/.local/share/opencode"),
            ),
        ]),
        RuntimeKind::Codex => BTreeMap::from([(
            CODEX_CONFIG_DESTINATION.to_string(),
            host_directory("/home/example/.codex"),
        )]),
    }
}

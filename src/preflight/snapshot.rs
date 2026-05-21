// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::runtime::RuntimeKind;

mod host;
mod host_state;
mod nix;
#[cfg(test)]
mod test_support;

use host::{PreflightHost, SystemPreflightHost};
pub use host_state::{HostDirectoryPreflightSnapshot, HostPreflightSnapshot};
pub use nix::{NixConfigPreflightSnapshot, NixCustomConfPreflightSnapshot, NixPreflightSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightSnapshot {
    pub host: HostPreflightSnapshot,
    pub nix: NixPreflightSnapshot,
}

impl PreflightSnapshot {
    pub fn detect(runtime: RuntimeKind) -> Self {
        Self::detect_with(runtime, &SystemPreflightHost)
    }

    fn detect_with(runtime: RuntimeKind, host: &impl PreflightHost) -> Self {
        Self {
            host: HostPreflightSnapshot::detect(runtime, host),
            nix: NixPreflightSnapshot::detect(host),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        CODEX_CONFIG_DESTINATION, ETC_NIX_DESTINATION, OPENCODE_CONFIG_DESTINATION,
        OPENCODE_DATA_DESTINATION,
    };
    use super::*;
    use crate::preflight::NIX_DAEMON_SOCKET_PATH;
    use crate::preflight::snapshot::host::HostPathStatus;
    use crate::preflight::snapshot::test_support::FakePreflightHost;
    use camino::Utf8Path;

    #[test]
    fn preflight_snapshot_detects_runtime_and_nix_state_from_host_probe() {
        let host = FakePreflightHost::default()
            .with_command("git")
            .with_command("podman")
            .with_env_path("HOME", "/home/example")
            .with_env_path("XDG_CONFIG_HOME", "/xdg/config")
            .with_env_path("XDG_DATA_HOME", "/xdg/data")
            .with_which("nix", "/run/current-system/sw/bin/nix")
            .with_socket(NIX_DAEMON_SOCKET_PATH)
            .with_existing_path(ETC_NIX_DESTINATION)
            .with_readable_file("/etc/nix/nix.conf")
            .with_path_status(
                "/xdg/config/opencode",
                HostPathStatus {
                    exists: true,
                    is_directory: true,
                    readable: true,
                    writable: true,
                    searchable: true,
                },
            )
            .with_path_status(
                "/xdg/data/opencode",
                HostPathStatus {
                    exists: true,
                    is_directory: true,
                    readable: true,
                    writable: true,
                    searchable: true,
                },
            );

        let snapshot = PreflightSnapshot::detect_with(RuntimeKind::Opencode, &host);

        assert!(snapshot.host.has_git);
        assert!(snapshot.host.has_podman);
        assert!(snapshot.nix.has_daemon_socket);
        assert_eq!(
            snapshot.nix.client_source.as_deref(),
            Some(Utf8Path::new("/run/current-system/sw/bin/nix"))
        );
        assert!(snapshot.nix.config.has_etc_nix_mount);
        assert!(snapshot.nix.config.has_readable_nix_conf);
        assert_eq!(
            snapshot
                .host
                .runtime_state
                .get(OPENCODE_CONFIG_DESTINATION)
                .and_then(|state| state.source.as_deref()),
            Some(Utf8Path::new("/xdg/config/opencode"))
        );
        assert_eq!(
            snapshot
                .host
                .runtime_state
                .get(OPENCODE_DATA_DESTINATION)
                .and_then(|state| state.source.as_deref()),
            Some(Utf8Path::new("/xdg/data/opencode"))
        );
        assert!(!snapshot.nix.config.custom_conf.present);
    }

    #[test]
    fn preflight_snapshot_fixture_mode_short_circuits_host_nix_requirements() {
        let host = FakePreflightHost::default().with_test_fixtures_enabled();

        let snapshot = PreflightSnapshot::detect_with(RuntimeKind::Codex, &host);

        assert!(snapshot.host.has_git);
        assert!(!snapshot.host.has_podman);
        assert!(snapshot.nix.has_daemon_socket);
        assert_eq!(
            snapshot.nix.client_source.as_deref(),
            Some(Utf8Path::new("/usr/bin/nix"))
        );
        assert!(snapshot.nix.config.has_etc_nix_mount);
        assert!(snapshot.nix.config.has_readable_nix_conf);
        assert_eq!(
            snapshot
                .host
                .runtime_state
                .get(CODEX_CONFIG_DESTINATION)
                .and_then(|state| state.source.as_ref()),
            None
        );
    }
}

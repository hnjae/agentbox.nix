// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use super::super::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, NIX_CUSTOM_CONF_PATH, NIX_DAEMON_SOCKET_PATH,
};
use super::host::PreflightHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixPreflightSnapshot {
    pub has_daemon_socket: bool,
    pub client_source: Option<Utf8PathBuf>,
    pub config: NixConfigPreflightSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixConfigPreflightSnapshot {
    pub has_etc_nix_mount: bool,
    pub has_readable_nix_conf: bool,
    pub custom_conf: NixCustomConfPreflightSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixCustomConfPreflightSnapshot {
    pub present: bool,
    pub has_readable_target: bool,
    pub needs_static_mount: bool,
}

impl NixPreflightSnapshot {
    pub(super) fn detect(host: &impl PreflightHost) -> Self {
        Self {
            has_daemon_socket: host.test_fixtures_enabled()
                || host.unix_socket_exists(Utf8Path::new(NIX_DAEMON_SOCKET_PATH)),
            client_source: resolve_nix_client_source(host),
            config: NixConfigPreflightSnapshot::detect(host),
        }
    }
}

impl NixConfigPreflightSnapshot {
    fn detect(host: &impl PreflightHost) -> Self {
        Self {
            has_etc_nix_mount: host.test_fixtures_enabled()
                || host.symlink_or_path_exists(Utf8Path::new(ETC_NIX_DESTINATION)),
            has_readable_nix_conf: host.test_fixtures_enabled()
                || host.file_is_readable(Utf8Path::new("/etc/nix/nix.conf")),
            custom_conf: NixCustomConfPreflightSnapshot::detect(host),
        }
    }
}

impl NixCustomConfPreflightSnapshot {
    fn detect(host: &impl PreflightHost) -> Self {
        let path = Utf8Path::new(NIX_CUSTOM_CONF_PATH);
        let resolved_path = host.resolve_path(path);
        Self {
            present: host.symlink_or_path_exists(path),
            has_readable_target: host.file_is_readable(&resolved_path),
            needs_static_mount: host
                .path_reaches_mount_root(path, Utf8Path::new(ETC_STATIC_NIX_DESTINATION)),
        }
    }
}

fn resolve_nix_client_source(host: &impl PreflightHost) -> Option<Utf8PathBuf> {
    if host.test_fixtures_enabled() {
        return Some(Utf8PathBuf::from("/usr/bin/nix"));
    }

    host.which("nix")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::snapshot::test_support::FakePreflightHost;

    #[test]
    fn custom_nix_conf_detection_uses_resolved_target_and_static_mount_probe() {
        let host = FakePreflightHost::default()
            .with_existing_path(NIX_CUSTOM_CONF_PATH)
            .with_resolved_path(NIX_CUSTOM_CONF_PATH, "/etc/static/nix/nix.custom.conf")
            .with_readable_file("/etc/static/nix/nix.custom.conf")
            .with_path_reaches_mount_root(NIX_CUSTOM_CONF_PATH, ETC_STATIC_NIX_DESTINATION);

        let custom_conf = NixCustomConfPreflightSnapshot::detect(&host);

        assert!(custom_conf.present);
        assert!(custom_conf.has_readable_target);
        assert!(custom_conf.needs_static_mount);
    }
}

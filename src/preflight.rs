// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod check;
mod mounts;
mod path;
mod snapshot;

pub use check::{
    PreflightReport, check_host_prerequisites_for_runtime, check_host_prerequisites_with_snapshot,
};
pub use mounts::required_host_mount_destinations;
pub use snapshot::{
    HostDirectoryPreflightSnapshot, HostPreflightSnapshot, NixConfigPreflightSnapshot,
    NixCustomConfPreflightSnapshot, NixPreflightSnapshot, PreflightSnapshot,
};

pub const NIX_DAEMON_SOCKET_PATH: &str = "/nix/var/nix/daemon-socket/socket";
pub const NIX_STORE_DESTINATION: &str = "/nix";
pub const NIX_CLIENT_DESTINATION: &str = "/usr/local/bin/nix";
pub const ETC_NIX_DESTINATION: &str = "/etc/nix";
pub const ETC_STATIC_NIX_DESTINATION: &str = "/etc/static/nix";
pub const NIX_CACHE_DESTINATION: &str = "/home/user";
pub const CODEX_CONFIG_DESTINATION: &str = "/home/user/.codex";
pub const OPENCODE_CONFIG_DESTINATION: &str = "/home/user/.config/opencode";
pub const OPENCODE_DATA_DESTINATION: &str = "/home/user/.local/share/opencode";

const NIX_CUSTOM_CONF_PATH: &str = "/etc/nix/nix.custom.conf";

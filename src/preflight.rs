// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

mod check;
mod path;
mod snapshot;

pub use check::{
    PreflightReport, check_host_prerequisites_for_runtime, check_host_prerequisites_with_snapshot,
    required_host_mount_destinations,
};
pub use snapshot::{
    DirenvPreflightSnapshot, HostDirectoryPreflightSnapshot, HostPreflightSnapshot,
    NixConfigPreflightSnapshot, NixCustomConfPreflightSnapshot, NixPreflightSnapshot,
    PreflightSnapshot,
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

pub fn direnv_applies_to_target(target_directory: &Utf8Path, git_root: &Utf8Path) -> bool {
    path::envrc_applies_within_git_root(target_directory, git_root)
}

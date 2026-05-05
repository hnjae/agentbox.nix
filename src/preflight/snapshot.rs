// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::runtime::{RuntimeHostStateSource, all_host_state_mounts};

use super::path::{
    envrc_applies_within_git_root, path_reaches_mount_root, resolve_path, symlink_or_path_exists,
};
use super::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, NIX_CUSTOM_CONF_PATH, NIX_DAEMON_SOCKET_PATH,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightSnapshot {
    pub host: HostPreflightSnapshot,
    pub nix: NixPreflightSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPreflightSnapshot {
    pub has_git: bool,
    pub has_podman: bool,
    pub direnv: DirenvPreflightSnapshot,
    pub runtime_state: BTreeMap<String, HostDirectoryPreflightSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirenvPreflightSnapshot {
    pub required: bool,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDirectoryPreflightSnapshot {
    pub source: Option<Utf8PathBuf>,
    pub exists: bool,
    pub is_directory: bool,
    pub readable: bool,
    pub writable: bool,
}

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

impl PreflightSnapshot {
    pub fn detect(target_directory: Option<&Utf8Path>, git_root: Option<&Utf8Path>) -> Self {
        Self {
            host: HostPreflightSnapshot::detect(target_directory, git_root),
            nix: NixPreflightSnapshot::detect(),
        }
    }
}

impl HostPreflightSnapshot {
    fn detect(target_directory: Option<&Utf8Path>, git_root: Option<&Utf8Path>) -> Self {
        Self {
            has_git: test_fixtures_enabled() || command_exists("git"),
            has_podman: command_exists("podman"),
            direnv: DirenvPreflightSnapshot::detect(target_directory, git_root),
            runtime_state: detect_runtime_state(),
        }
    }
}

impl DirenvPreflightSnapshot {
    fn detect(target_directory: Option<&Utf8Path>, git_root: Option<&Utf8Path>) -> Self {
        Self {
            required: target_directory
                .zip(git_root)
                .is_some_and(|(target_directory, git_root)| {
                    envrc_applies_within_git_root(target_directory, git_root)
                }),
            available: command_exists("direnv"),
        }
    }
}

impl HostDirectoryPreflightSnapshot {
    fn detect(source: Option<Utf8PathBuf>) -> Self {
        let metadata = source
            .as_ref()
            .and_then(|path| fs::metadata(path.as_std_path()).ok());
        let exists = source
            .as_ref()
            .is_some_and(|path| symlink_or_path_exists(path));
        let is_directory = metadata.as_ref().is_some_and(fs::Metadata::is_dir);
        let readable = source
            .as_ref()
            .is_some_and(|path| fs::read_dir(path.as_std_path()).is_ok());
        let writable = metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.permissions().readonly());

        Self {
            source,
            exists,
            is_directory,
            readable,
            writable,
        }
    }
}

impl NixPreflightSnapshot {
    fn detect() -> Self {
        Self {
            has_daemon_socket: test_fixtures_enabled()
                || unix_socket_exists(Utf8Path::new(NIX_DAEMON_SOCKET_PATH)),
            client_source: resolve_nix_client_source(),
            config: NixConfigPreflightSnapshot::detect(),
        }
    }
}

impl NixConfigPreflightSnapshot {
    fn detect() -> Self {
        Self {
            has_etc_nix_mount: test_fixtures_enabled()
                || symlink_or_path_exists(Utf8Path::new(ETC_NIX_DESTINATION)),
            has_readable_nix_conf: test_fixtures_enabled()
                || fs::File::open("/etc/nix/nix.conf").is_ok(),
            custom_conf: NixCustomConfPreflightSnapshot::detect(),
        }
    }
}

impl NixCustomConfPreflightSnapshot {
    fn detect() -> Self {
        let path = Utf8Path::new(NIX_CUSTOM_CONF_PATH);
        let resolved_path = resolve_path(path);
        Self {
            present: symlink_or_path_exists(path),
            has_readable_target: fs::File::open(&resolved_path).is_ok(),
            needs_static_mount: path_reaches_mount_root(
                path,
                Utf8Path::new(ETC_STATIC_NIX_DESTINATION),
            ),
        }
    }
}

fn detect_runtime_state() -> BTreeMap<String, HostDirectoryPreflightSnapshot> {
    all_host_state_mounts()
        .map(|mount| {
            (
                mount.destination.to_string(),
                HostDirectoryPreflightSnapshot::detect(host_state_source(mount.source)),
            )
        })
        .collect()
}

fn host_state_source(source: RuntimeHostStateSource) -> Option<Utf8PathBuf> {
    source.resolve(path_from_environment)
}

fn path_from_environment(variable: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(variable)
        .filter(|value| !value.as_os_str().is_empty())
        .map(std::path::PathBuf::from)
}

fn resolve_nix_client_source() -> Option<Utf8PathBuf> {
    if test_fixtures_enabled() {
        return Some(Utf8PathBuf::from("/usr/bin/nix"));
    }

    which::which("nix")
        .ok()
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
}

fn test_fixtures_enabled() -> bool {
    std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some()
}

fn command_exists(program: &str) -> bool {
    which::which(program).is_ok()
}

#[cfg(unix)]
fn unix_socket_exists(path: &Utf8Path) -> bool {
    use std::os::unix::fs::FileTypeExt;

    fs::metadata(path.as_std_path())
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn unix_socket_exists(_path: &Utf8Path) -> bool {
    false
}

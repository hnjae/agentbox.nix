// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
#[cfg(unix)]
use std::ffi::CString;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::runtime::{RuntimeHostStateSource, RuntimeKind};

use super::path::{path_reaches_mount_root, resolve_path, symlink_or_path_exists};
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
    pub runtime_state: BTreeMap<String, HostDirectoryPreflightSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDirectoryPreflightSnapshot {
    pub source: Option<Utf8PathBuf>,
    pub exists: bool,
    pub is_directory: bool,
    pub readable: bool,
    pub writable: bool,
    pub searchable: bool,
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
    pub fn detect(runtime: RuntimeKind) -> Self {
        Self {
            host: HostPreflightSnapshot::detect(runtime),
            nix: NixPreflightSnapshot::detect(),
        }
    }
}

impl HostPreflightSnapshot {
    fn detect(runtime: RuntimeKind) -> Self {
        Self {
            has_git: test_fixtures_enabled() || command_exists("git"),
            has_podman: command_exists("podman"),
            runtime_state: detect_runtime_state(runtime),
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
            .is_some_and(|path| current_user_has_access(path, AccessMode::Read));
        let writable = source
            .as_ref()
            .is_some_and(|path| current_user_has_access(path, AccessMode::Write));
        let searchable = source
            .as_ref()
            .is_some_and(|path| current_user_has_access(path, AccessMode::Search));

        Self {
            source,
            exists,
            is_directory,
            readable,
            writable,
            searchable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
    Search,
}

#[cfg(unix)]
fn current_user_has_access(path: &Utf8Path, mode: AccessMode) -> bool {
    let Ok(path) = CString::new(path.as_std_path().as_os_str().as_bytes()) else {
        return false;
    };

    // access(2) checks the real uid/gid, which matches the host user whose
    // state directory will be mounted into the runtime.
    unsafe { libc::access(path.as_ptr(), access_mode_flag(mode)) == 0 }
}

#[cfg(unix)]
fn access_mode_flag(mode: AccessMode) -> libc::c_int {
    match mode {
        AccessMode::Read => libc::R_OK,
        AccessMode::Write => libc::W_OK,
        AccessMode::Search => libc::X_OK,
    }
}

#[cfg(not(unix))]
fn current_user_has_access(path: &Utf8Path, mode: AccessMode) -> bool {
    match mode {
        AccessMode::Read => fs::read_dir(path.as_std_path()).is_ok(),
        AccessMode::Write => fs::metadata(path.as_std_path())
            .is_ok_and(|metadata| !metadata.permissions().readonly()),
        AccessMode::Search => path.as_std_path().is_dir(),
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

fn detect_runtime_state(runtime: RuntimeKind) -> BTreeMap<String, HostDirectoryPreflightSnapshot> {
    runtime
        .host_state_mounts()
        .iter()
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

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn host_directory_detect_uses_current_user_access_not_any_writable_mode_bit() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let state_directory = root.join("state");
        fs::create_dir(&state_directory).unwrap();
        fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o020)).unwrap();

        let snapshot = HostDirectoryPreflightSnapshot::detect(Some(state_directory.clone()));

        fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(snapshot.exists);
        assert!(snapshot.is_directory);
        assert!(!snapshot.readable);
        assert!(!snapshot.writable);
        assert!(!snapshot.searchable);
    }

    #[test]
    fn host_directory_detect_accepts_read_write_search_access_without_probe_file() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let state_directory = root.join("state");
        fs::create_dir(&state_directory).unwrap();
        fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(
            fs::read_dir(&state_directory).unwrap().count(),
            0,
            "test setup should start with an empty directory"
        );

        let snapshot = HostDirectoryPreflightSnapshot::detect(Some(state_directory.clone()));

        assert!(snapshot.exists);
        assert!(snapshot.is_directory);
        assert!(snapshot.readable);
        assert!(snapshot.writable);
        assert!(snapshot.searchable);
        assert_eq!(
            fs::read_dir(&state_directory).unwrap().count(),
            0,
            "access detection must not create probe files"
        );
    }
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use super::super::path::{path_reaches_mount_root, resolve_path, symlink_or_path_exists};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct HostPathStatus {
    pub(super) exists: bool,
    pub(super) is_directory: bool,
    pub(super) readable: bool,
    pub(super) writable: bool,
    pub(super) searchable: bool,
}

pub(super) trait PreflightHost {
    fn test_fixtures_enabled(&self) -> bool;
    fn command_exists(&self, program: &str) -> bool;
    fn path_from_environment(&self, variable: &str) -> Option<PathBuf>;
    fn which(&self, program: &str) -> Option<Utf8PathBuf>;
    fn unix_socket_exists(&self, path: &Utf8Path) -> bool;
    fn symlink_or_path_exists(&self, path: &Utf8Path) -> bool;
    fn file_is_readable(&self, path: &Utf8Path) -> bool;
    fn path_status(&self, path: &Utf8Path) -> HostPathStatus;
    fn resolve_path(&self, path: &Utf8Path) -> Utf8PathBuf;
    fn path_reaches_mount_root(&self, path: &Utf8Path, mount_root: &Utf8Path) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SystemPreflightHost;

impl PreflightHost for SystemPreflightHost {
    fn test_fixtures_enabled(&self) -> bool {
        std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some()
    }

    fn command_exists(&self, program: &str) -> bool {
        which::which(program).is_ok()
    }

    fn path_from_environment(&self, variable: &str) -> Option<PathBuf> {
        std::env::var_os(variable)
            .filter(|value| !value.as_os_str().is_empty())
            .map(PathBuf::from)
    }

    fn which(&self, program: &str) -> Option<Utf8PathBuf> {
        which::which(program)
            .ok()
            .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
    }

    fn unix_socket_exists(&self, path: &Utf8Path) -> bool {
        unix_socket_exists(path)
    }

    fn symlink_or_path_exists(&self, path: &Utf8Path) -> bool {
        symlink_or_path_exists(path)
    }

    fn file_is_readable(&self, path: &Utf8Path) -> bool {
        fs::File::open(path.as_std_path()).is_ok()
    }

    fn path_status(&self, path: &Utf8Path) -> HostPathStatus {
        let metadata = fs::metadata(path.as_std_path()).ok();
        HostPathStatus {
            exists: self.symlink_or_path_exists(path),
            is_directory: metadata.as_ref().is_some_and(fs::Metadata::is_dir),
            readable: current_user_has_access(path, AccessMode::Read),
            writable: current_user_has_access(path, AccessMode::Write),
            searchable: current_user_has_access(path, AccessMode::Search),
        }
    }

    fn resolve_path(&self, path: &Utf8Path) -> Utf8PathBuf {
        resolve_path(path)
    }

    fn path_reaches_mount_root(&self, path: &Utf8Path, mount_root: &Utf8Path) -> bool {
        path_reaches_mount_root(path, mount_root)
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
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

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

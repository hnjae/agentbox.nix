// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
#[cfg(unix)]
use std::ffi::CString;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

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
        Self::detect_with(runtime, &SystemPreflightHost)
    }

    fn detect_with(runtime: RuntimeKind, host: &impl PreflightHost) -> Self {
        Self {
            host: HostPreflightSnapshot::detect(runtime, host),
            nix: NixPreflightSnapshot::detect(host),
        }
    }
}

impl HostPreflightSnapshot {
    fn detect(runtime: RuntimeKind, host: &impl PreflightHost) -> Self {
        Self {
            has_git: host.test_fixtures_enabled() || host.command_exists("git"),
            has_podman: host.command_exists("podman"),
            runtime_state: detect_runtime_state(runtime, host),
        }
    }
}

impl HostDirectoryPreflightSnapshot {
    fn detect_with(source: Option<Utf8PathBuf>, host: &impl PreflightHost) -> Self {
        let status = source
            .as_deref()
            .map(|path| host.path_status(path))
            .unwrap_or_default();

        Self {
            source,
            exists: status.exists,
            is_directory: status.is_directory,
            readable: status.readable,
            writable: status.writable,
            searchable: status.searchable,
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
    fn detect(host: &impl PreflightHost) -> Self {
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

fn detect_runtime_state(
    runtime: RuntimeKind,
    host: &impl PreflightHost,
) -> BTreeMap<String, HostDirectoryPreflightSnapshot> {
    runtime
        .host_state_mounts()
        .iter()
        .map(|mount| {
            (
                mount.destination.to_string(),
                HostDirectoryPreflightSnapshot::detect_with(
                    host_state_source(mount.source, host),
                    host,
                ),
            )
        })
        .collect()
}

fn host_state_source(
    source: RuntimeHostStateSource,
    host: &impl PreflightHost,
) -> Option<Utf8PathBuf> {
    source.resolve(|variable| host.path_from_environment(variable))
}

fn resolve_nix_client_source(host: &impl PreflightHost) -> Option<Utf8PathBuf> {
    if host.test_fixtures_enabled() {
        return Some(Utf8PathBuf::from("/usr/bin/nix"));
    }

    host.which("nix")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HostPathStatus {
    exists: bool,
    is_directory: bool,
    readable: bool,
    writable: bool,
    searchable: bool,
}

trait PreflightHost {
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
struct SystemPreflightHost;

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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::super::{
        CODEX_CONFIG_DESTINATION, OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION,
    };
    use super::*;

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

    #[cfg(unix)]
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

        let snapshot = HostDirectoryPreflightSnapshot::detect_with(
            Some(state_directory.clone()),
            &SystemPreflightHost,
        );

        fs::set_permissions(&state_directory, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(snapshot.exists);
        assert!(snapshot.is_directory);
        assert!(!snapshot.readable);
        assert!(!snapshot.writable);
        assert!(!snapshot.searchable);
    }

    #[cfg(unix)]
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

        let snapshot = HostDirectoryPreflightSnapshot::detect_with(
            Some(state_directory.clone()),
            &SystemPreflightHost,
        );

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

    #[derive(Debug, Default)]
    struct FakePreflightHost {
        test_fixtures_enabled: bool,
        commands: BTreeSet<String>,
        environment: BTreeMap<String, PathBuf>,
        binaries: BTreeMap<String, Utf8PathBuf>,
        sockets: BTreeSet<Utf8PathBuf>,
        path_statuses: BTreeMap<Utf8PathBuf, HostPathStatus>,
        readable_files: BTreeSet<Utf8PathBuf>,
        resolved_paths: BTreeMap<Utf8PathBuf, Utf8PathBuf>,
        paths_reaching_mount_root: BTreeSet<(Utf8PathBuf, Utf8PathBuf)>,
    }

    impl FakePreflightHost {
        fn with_test_fixtures_enabled(mut self) -> Self {
            self.test_fixtures_enabled = true;
            self
        }

        fn with_command(mut self, program: &str) -> Self {
            self.commands.insert(program.to_string());
            self
        }

        fn with_env_path(mut self, variable: &str, path: &str) -> Self {
            self.environment
                .insert(variable.to_string(), PathBuf::from(path));
            self
        }

        fn with_which(mut self, program: &str, path: &str) -> Self {
            self.binaries
                .insert(program.to_string(), Utf8PathBuf::from(path));
            self
        }

        fn with_socket(mut self, path: &str) -> Self {
            self.sockets.insert(Utf8PathBuf::from(path));
            self
        }

        fn with_existing_path(mut self, path: &str) -> Self {
            self.path_statuses.insert(
                Utf8PathBuf::from(path),
                HostPathStatus {
                    exists: true,
                    ..HostPathStatus::default()
                },
            );
            self
        }

        fn with_path_status(mut self, path: &str, status: HostPathStatus) -> Self {
            self.path_statuses.insert(Utf8PathBuf::from(path), status);
            self
        }

        fn with_readable_file(mut self, path: &str) -> Self {
            let path = Utf8PathBuf::from(path);
            self.path_statuses.insert(
                path.clone(),
                HostPathStatus {
                    exists: true,
                    ..HostPathStatus::default()
                },
            );
            self.readable_files.insert(path);
            self
        }

        fn with_resolved_path(mut self, path: &str, resolved: &str) -> Self {
            self.resolved_paths
                .insert(Utf8PathBuf::from(path), Utf8PathBuf::from(resolved));
            self
        }

        fn with_path_reaches_mount_root(mut self, path: &str, mount_root: &str) -> Self {
            self.paths_reaching_mount_root
                .insert((Utf8PathBuf::from(path), Utf8PathBuf::from(mount_root)));
            self
        }
    }

    impl PreflightHost for FakePreflightHost {
        fn test_fixtures_enabled(&self) -> bool {
            self.test_fixtures_enabled
        }

        fn command_exists(&self, program: &str) -> bool {
            self.commands.contains(program)
        }

        fn path_from_environment(&self, variable: &str) -> Option<PathBuf> {
            self.environment.get(variable).cloned()
        }

        fn which(&self, program: &str) -> Option<Utf8PathBuf> {
            self.binaries.get(program).cloned()
        }

        fn unix_socket_exists(&self, path: &Utf8Path) -> bool {
            self.sockets.contains(path)
        }

        fn symlink_or_path_exists(&self, path: &Utf8Path) -> bool {
            self.path_status(path).exists
        }

        fn file_is_readable(&self, path: &Utf8Path) -> bool {
            self.readable_files.contains(path)
        }

        fn path_status(&self, path: &Utf8Path) -> HostPathStatus {
            self.path_statuses.get(path).copied().unwrap_or_default()
        }

        fn resolve_path(&self, path: &Utf8Path) -> Utf8PathBuf {
            self.resolved_paths
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.to_path_buf())
        }

        fn path_reaches_mount_root(&self, path: &Utf8Path, mount_root: &Utf8Path) -> bool {
            self.paths_reaching_mount_root
                .contains(&(path.to_path_buf(), mount_root.to_path_buf()))
        }
    }
}

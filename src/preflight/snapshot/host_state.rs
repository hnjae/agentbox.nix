// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8PathBuf;

use crate::runtime::{
    RuntimeHostStateSource, RuntimeHostStateSourceResolution, RuntimeKind, RuntimeRunMode,
};

use super::host::PreflightHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPreflightSnapshot {
    pub has_git: bool,
    pub has_podman: bool,
    pub runtime_state: BTreeMap<String, HostDirectoryPreflightSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDirectoryPreflightSnapshot {
    pub source: Option<Utf8PathBuf>,
    pub source_environment_variable: Option<String>,
    pub source_error: Option<String>,
    pub exists: bool,
    pub is_directory: bool,
    pub readable: bool,
    pub writable: bool,
    pub searchable: bool,
}

impl HostPreflightSnapshot {
    pub(super) fn detect(
        runtime: RuntimeKind,
        run_mode: RuntimeRunMode,
        host: &impl PreflightHost,
    ) -> Self {
        Self {
            has_git: host.test_fixtures_enabled() || host.command_exists("git"),
            has_podman: host.command_exists("podman"),
            runtime_state: detect_runtime_state(runtime, run_mode, host),
        }
    }
}

impl HostDirectoryPreflightSnapshot {
    pub(super) fn detect_with(
        source: Option<Utf8PathBuf>,
        source_environment_variable: Option<String>,
        host: &impl PreflightHost,
    ) -> Self {
        let status = source
            .as_deref()
            .map(|path| host.path_status(path))
            .unwrap_or_default();

        Self {
            source,
            source_environment_variable,
            source_error: None,
            exists: status.exists,
            is_directory: status.is_directory,
            readable: status.readable,
            writable: status.writable,
            searchable: status.searchable,
        }
    }

    fn detect_error(error: crate::Error) -> Self {
        Self {
            source: None,
            source_environment_variable: None,
            source_error: Some(error.to_string()),
            exists: false,
            is_directory: false,
            readable: false,
            writable: false,
            searchable: false,
        }
    }
}

fn detect_runtime_state(
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
    host: &impl PreflightHost,
) -> BTreeMap<String, HostDirectoryPreflightSnapshot> {
    runtime
        .host_state_mounts(run_mode)
        .iter()
        .map(|mount| {
            (
                mount.snapshot_key().to_string(),
                match host_state_source(mount.source, host) {
                    Ok(resolution) => HostDirectoryPreflightSnapshot::detect_with(
                        resolution.source,
                        resolution.source_environment_variable.map(str::to_string),
                        host,
                    ),
                    Err(error) => HostDirectoryPreflightSnapshot::detect_error(error),
                },
            )
        })
        .collect()
}

fn host_state_source(
    source: RuntimeHostStateSource,
    host: &impl PreflightHost,
) -> crate::Result<RuntimeHostStateSourceResolution> {
    source.resolve(|variable| host.path_from_environment(variable))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::preflight::snapshot::host::SystemPreflightHost;

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
            None,
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
            None,
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
}

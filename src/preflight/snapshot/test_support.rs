// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use super::host::{HostPathStatus, PreflightHost};

#[derive(Debug, Default)]
pub(super) struct FakePreflightHost {
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
    pub(super) fn with_test_fixtures_enabled(mut self) -> Self {
        self.test_fixtures_enabled = true;
        self
    }

    pub(super) fn with_command(mut self, program: &str) -> Self {
        self.commands.insert(program.to_string());
        self
    }

    pub(super) fn with_env_path(mut self, variable: &str, path: &str) -> Self {
        self.environment
            .insert(variable.to_string(), PathBuf::from(path));
        self
    }

    pub(super) fn with_which(mut self, program: &str, path: &str) -> Self {
        self.binaries
            .insert(program.to_string(), Utf8PathBuf::from(path));
        self
    }

    pub(super) fn with_socket(mut self, path: &str) -> Self {
        self.sockets.insert(Utf8PathBuf::from(path));
        self
    }

    pub(super) fn with_existing_path(mut self, path: &str) -> Self {
        self.path_statuses.insert(
            Utf8PathBuf::from(path),
            HostPathStatus {
                exists: true,
                ..HostPathStatus::default()
            },
        );
        self
    }

    pub(super) fn with_path_status(mut self, path: &str, status: HostPathStatus) -> Self {
        self.path_statuses.insert(Utf8PathBuf::from(path), status);
        self
    }

    pub(super) fn with_readable_file(mut self, path: &str) -> Self {
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

    pub(super) fn with_resolved_path(mut self, path: &str, resolved: &str) -> Self {
        self.resolved_paths
            .insert(Utf8PathBuf::from(path), Utf8PathBuf::from(resolved));
        self
    }

    pub(super) fn with_path_reaches_mount_root(mut self, path: &str, mount_root: &str) -> Self {
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

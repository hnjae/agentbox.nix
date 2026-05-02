// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::runtime::{RuntimeMount, RuntimeMountKind};
use crate::{Error, Result};

pub const NIX_DAEMON_SOCKET_PATH: &str = "/nix/var/nix/daemon-socket/socket";
pub const NIX_STORE_DESTINATION: &str = "/nix";
pub const NIX_CLIENT_DESTINATION: &str = "/usr/local/bin/nix";
pub const ETC_NIX_DESTINATION: &str = "/etc/nix";
pub const ETC_STATIC_NIX_DESTINATION: &str = "/etc/static/nix";
pub const NIX_CACHE_DESTINATION: &str = "/home/user/.cache/nix";

const NIX_CLIENT_CANDIDATES: [&str; 2] = [
    "/run/current-system/sw/bin/nix",
    "/nix/var/nix/profiles/default/bin/nix",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub host_nix_mounts: Vec<RuntimeMount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightSnapshot {
    pub has_git: bool,
    pub has_podman: bool,
    pub direnv_required: bool,
    pub has_direnv: bool,
    pub has_nix_daemon_socket: bool,
    pub nix_client_source: Option<Utf8PathBuf>,
    pub has_etc_nix_mount: bool,
    pub has_readable_nix_conf: bool,
    pub nix_custom_conf_present: bool,
    pub has_readable_nix_custom_conf_target: bool,
    pub needs_static_nix_mount: bool,
}

impl PreflightSnapshot {
    pub fn detect(target_directory: Option<&Utf8Path>, git_root: Option<&Utf8Path>) -> Self {
        let direnv_required =
            target_directory
                .zip(git_root)
                .is_some_and(|(target_directory, git_root)| {
                    envrc_applies_within_git_root(target_directory, git_root)
                });

        let nix_custom_conf = Utf8Path::new("/etc/nix/nix.custom.conf");
        let nix_custom_conf_present = symlink_or_path_exists(nix_custom_conf);
        let resolved_nix_custom_conf = resolve_path(nix_custom_conf);

        Self {
            has_git: std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some() || command_exists("git"),
            has_podman: command_exists("podman"),
            direnv_required,
            has_direnv: command_exists("direnv"),
            has_nix_daemon_socket: std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some()
                || unix_socket_exists(Utf8Path::new(NIX_DAEMON_SOCKET_PATH)),
            nix_client_source: resolve_nix_client_source(),
            has_etc_nix_mount: std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some()
                || symlink_or_path_exists(Utf8Path::new(ETC_NIX_DESTINATION)),
            has_readable_nix_conf: std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some()
                || fs::File::open("/etc/nix/nix.conf").is_ok(),
            nix_custom_conf_present,
            has_readable_nix_custom_conf_target: fs::File::open(&resolved_nix_custom_conf).is_ok(),
            needs_static_nix_mount: resolved_nix_custom_conf
                == Utf8Path::new(ETC_STATIC_NIX_DESTINATION)
                || resolved_nix_custom_conf.starts_with(Utf8Path::new(ETC_STATIC_NIX_DESTINATION)),
        }
    }
}

pub fn check_host_prerequisites(
    target_directory: Option<&Utf8Path>,
    git_root: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot(
        &PreflightSnapshot::detect(target_directory, git_root),
        target_directory,
    )
}

pub fn check_host_prerequisites_with_snapshot(
    snapshot: &PreflightSnapshot,
    target_directory: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    if !snapshot.has_git {
        return Err(Error::msg(
            "`git` was not found on PATH; install `git` or add it to PATH",
        ));
    }

    if !snapshot.has_podman {
        return Err(Error::msg(
            "`podman` was not found on PATH; install `podman` or add it to PATH",
        ));
    }

    if snapshot.direnv_required && !snapshot.has_direnv {
        let target = target_directory
            .map(ToString::to_string)
            .unwrap_or_else(|| ".".to_string());
        return Err(Error::msg(format!(
            "`.envrc` applies to `{target}`, but `direnv` was not found on PATH; install `direnv` or add it to PATH"
        )));
    }

    if !snapshot.has_nix_daemon_socket {
        return Err(Error::msg(format!(
            "Missing host nix-daemon socket at: {NIX_DAEMON_SOCKET_PATH}. Mount /nix:/nix:ro."
        )));
    }

    let nix_client_source = snapshot.nix_client_source.as_ref().ok_or_else(|| {
        Error::msg(
            "Expected host-mounted nix not found in PATH. Mount /run/current-system/sw/bin/nix:/usr/local/bin/nix:ro or /nix/var/nix/profiles/default/bin/nix:/usr/local/bin/nix:ro.",
        )
    })?;

    if !snapshot.has_etc_nix_mount {
        return Err(Error::msg(
            "Missing /etc/nix host mount. Mount /etc/nix:/etc/nix:ro so the wrapper inherits the host config and registry.",
        ));
    }

    if !snapshot.has_readable_nix_conf {
        return Err(Error::msg(
            "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro.",
        ));
    }

    if snapshot.nix_custom_conf_present && !snapshot.has_readable_nix_custom_conf_target {
        return Err(Error::msg(
            "Missing readable target for /etc/nix/nix.custom.conf. Mount /etc/static/nix:/etc/static/nix:ro when /etc/nix points there.",
        ));
    }

    let mut host_nix_mounts = vec![RuntimeMount {
        kind: RuntimeMountKind::Bind,
        source: NIX_STORE_DESTINATION.to_string(),
        destination: NIX_STORE_DESTINATION.to_string(),
        read_only: true,
    }];
    host_nix_mounts.push(RuntimeMount {
        kind: RuntimeMountKind::Bind,
        source: nix_client_source.to_string(),
        destination: NIX_CLIENT_DESTINATION.to_string(),
        read_only: true,
    });
    host_nix_mounts.push(RuntimeMount {
        kind: RuntimeMountKind::Bind,
        source: ETC_NIX_DESTINATION.to_string(),
        destination: ETC_NIX_DESTINATION.to_string(),
        read_only: true,
    });

    if snapshot.needs_static_nix_mount {
        host_nix_mounts.push(RuntimeMount {
            kind: RuntimeMountKind::Bind,
            source: ETC_STATIC_NIX_DESTINATION.to_string(),
            destination: ETC_STATIC_NIX_DESTINATION.to_string(),
            read_only: true,
        });
    }

    Ok(PreflightReport { host_nix_mounts })
}

pub fn direnv_applies_to_target(target_directory: &Utf8Path, git_root: &Utf8Path) -> bool {
    envrc_applies_within_git_root(target_directory, git_root)
}

pub fn required_host_mount_destinations() -> [&'static str; 4] {
    [
        NIX_STORE_DESTINATION,
        NIX_CLIENT_DESTINATION,
        ETC_NIX_DESTINATION,
        ETC_STATIC_NIX_DESTINATION,
    ]
}

fn resolve_nix_client_source() -> Option<Utf8PathBuf> {
    if std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some() {
        return Some(Utf8PathBuf::from("/usr/bin/nix"));
    }

    NIX_CLIENT_CANDIDATES.iter().find_map(|candidate| {
        let path = Utf8PathBuf::from(candidate.to_string());
        fs::File::open(path.as_std_path()).ok().map(|_| path)
    })
}

fn command_exists(program: &str) -> bool {
    which::which(program).is_ok()
}

fn envrc_applies_within_git_root(target_directory: &Utf8Path, git_root: &Utf8Path) -> bool {
    if target_directory != git_root && !target_directory.starts_with(git_root) {
        return false;
    }

    target_directory
        .ancestors()
        .take_while(|candidate| *candidate != git_root)
        .chain(std::iter::once(git_root))
        .any(|candidate| candidate.join(".envrc").is_file())
}

fn symlink_or_path_exists(path: &Utf8Path) -> bool {
    fs::symlink_metadata(path.as_std_path()).is_ok()
}

fn resolve_path(path: &Utf8Path) -> Utf8PathBuf {
    fs::canonicalize(path.as_std_path())
        .ok()
        .and_then(|value| Utf8PathBuf::from_path_buf(value).ok())
        .unwrap_or_else(|| path.to_owned())
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

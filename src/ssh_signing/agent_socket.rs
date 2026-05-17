// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

pub(super) const HOST_SSH_AUTH_SOCK_ENV: &str = "SSH_AUTH_SOCK";

pub(super) fn detect_host_agent_socket(
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    warning: &mut impl FnMut(String),
) -> Option<Utf8PathBuf> {
    let host_socket = environment(HOST_SSH_AUTH_SOCK_ENV)?;
    if host_socket.is_empty() {
        return None;
    }

    let Some(host_socket) = utf8_path(host_socket) else {
        warning(format!(
            "{HOST_SSH_AUTH_SOCK_ENV} is not UTF-8; SSH commit signing passthrough disabled"
        ));
        return None;
    };

    if let Err(reason) = validate_ssh_agent_socket(&host_socket) {
        warning(format!(
            "{HOST_SSH_AUTH_SOCK_ENV} does not reference a usable Unix socket ({reason}); SSH commit signing passthrough disabled"
        ));
        return None;
    }

    Some(host_socket)
}

pub(super) fn utf8_path(value: OsString) -> Option<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(PathBuf::from(value)).ok()
}

#[cfg(unix)]
fn validate_ssh_agent_socket(path: &Utf8Path) -> std::result::Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixStream;

    let metadata =
        fs::metadata(path.as_std_path()).map_err(|error| format!("{}: {error}", path.as_str()))?;
    if !metadata.file_type().is_socket() {
        return Err(format!("{} is not a Unix socket", path.as_str()));
    }

    UnixStream::connect(path.as_std_path())
        .map(|_| ())
        .map_err(|error| format!("cannot connect to {}: {error}", path.as_str()))
}

#[cfg(not(unix))]
fn validate_ssh_agent_socket(path: &Utf8Path) -> std::result::Result<(), String> {
    Err(format!(
        "{} cannot be validated on this platform",
        path.as_str()
    ))
}

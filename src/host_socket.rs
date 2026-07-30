// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

pub(crate) fn utf8_path(value: OsString) -> Option<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(PathBuf::from(value)).ok()
}

#[cfg(unix)]
pub(crate) fn validate_connectable_unix_socket(path: &Utf8Path) -> std::result::Result<(), String> {
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
pub(crate) fn validate_connectable_unix_socket(path: &Utf8Path) -> std::result::Result<(), String> {
    Err(format!(
        "{} cannot be validated on this platform",
        path.as_str()
    ))
}

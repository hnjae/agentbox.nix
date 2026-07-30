// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;

use camino::Utf8PathBuf;

pub(super) use crate::host_socket::utf8_path;
use crate::host_socket::validate_connectable_unix_socket;

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

    if let Err(reason) = validate_connectable_unix_socket(&host_socket) {
        warning(format!(
            "{HOST_SSH_AUTH_SOCK_ENV} does not reference a usable Unix socket ({reason}); SSH commit signing passthrough disabled"
        ));
        return None;
    }

    Some(host_socket)
}

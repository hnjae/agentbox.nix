// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::ffi::OsString;

use camino::Utf8PathBuf;

use crate::diagnostic;
use crate::host_socket::{utf8_path, validate_connectable_unix_socket};
use crate::runtime::{RuntimeMount, RuntimeRunSpec};

const HOST_WAYLAND_DISPLAY_ENV: &str = "WAYLAND_DISPLAY";
const HOST_XDG_RUNTIME_DIR_ENV: &str = "XDG_RUNTIME_DIR";
const CONTAINER_WAYLAND_DISPLAY: &str = "/run/agentbox/wayland.sock";

#[derive(Debug, Clone, PartialEq, Eq)]
struct WaylandPassthrough {
    host_socket: Utf8PathBuf,
}

impl WaylandPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) {
        run_spec.add_create_mount(RuntimeMount::bind(
            self.host_socket.to_string(),
            CONTAINER_WAYLAND_DISPLAY,
        ));
        run_spec.extend_create_default_env(BTreeMap::from([(
            HOST_WAYLAND_DISPLAY_ENV.to_string(),
            CONTAINER_WAYLAND_DISPLAY.to_string(),
        )]));
    }
}

pub(crate) fn apply_wayland_passthrough(run_spec: &mut RuntimeRunSpec) {
    if let Some(passthrough) =
        detect_with(&mut |name| std::env::var_os(name), &mut diagnostic::warning)
    {
        passthrough.apply_to(run_spec);
    }
}

fn detect_with(
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    warning: &mut impl FnMut(String),
) -> Option<WaylandPassthrough> {
    let display = environment(HOST_WAYLAND_DISPLAY_ENV)?;
    if display.is_empty() {
        return None;
    }

    let Some(display) = utf8_path(display) else {
        warning(format!(
            "{HOST_WAYLAND_DISPLAY_ENV} is not UTF-8; Wayland passthrough disabled"
        ));
        return None;
    };

    let host_socket = if display.is_absolute() {
        display
    } else {
        let Some(runtime_dir) = environment(HOST_XDG_RUNTIME_DIR_ENV) else {
            warn_unresolvable_relative_display(warning);
            return None;
        };
        if runtime_dir.is_empty() {
            warn_unresolvable_relative_display(warning);
            return None;
        }
        let Some(runtime_dir) = utf8_path(runtime_dir) else {
            warning(format!(
                "{HOST_XDG_RUNTIME_DIR_ENV} is not UTF-8; Wayland passthrough disabled"
            ));
            return None;
        };
        if !runtime_dir.is_absolute() {
            warning(format!(
                "{HOST_XDG_RUNTIME_DIR_ENV} is not absolute; Wayland passthrough disabled"
            ));
            return None;
        }
        runtime_dir.join(display)
    };

    if let Err(reason) = validate_connectable_unix_socket(&host_socket) {
        warning(format!(
            "{HOST_WAYLAND_DISPLAY_ENV} does not reference a usable Unix socket ({reason}); Wayland passthrough disabled"
        ));
        return None;
    }

    Some(WaylandPassthrough { host_socket })
}

fn warn_unresolvable_relative_display(warning: &mut impl FnMut(String)) {
    warning(format!(
        "{HOST_WAYLAND_DISPLAY_ENV} is relative but {HOST_XDG_RUNTIME_DIR_ENV} is unset or empty; Wayland passthrough disabled"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{RuntimeCreateSpec, RuntimeRunSpec};

    #[test]
    fn unset_or_empty_display_does_not_enable_passthrough() {
        let mut warnings = Vec::new();

        let unset = detect_with(&mut |_| None, &mut |warning| warnings.push(warning));
        let empty = detect_with(
            &mut |name| match name {
                HOST_WAYLAND_DISPLAY_ENV => Some(OsString::new()),
                _ => None,
            },
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(unset, None);
        assert_eq!(empty, None);
        assert!(warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn relative_display_resolves_to_runtime_socket_and_applies_minimal_contract() {
        let (socket_dir, socket_path, _listener) = bind_test_socket();
        let passthrough = detect_with(
            &mut |name| match name {
                HOST_WAYLAND_DISPLAY_ENV => Some(OsString::from("wayland-7")),
                HOST_XDG_RUNTIME_DIR_ENV => Some(socket_dir.path().as_os_str().to_os_string()),
                _ => None,
            },
            &mut panic_warning,
        )
        .unwrap();
        let mut spec = empty_run_spec();

        passthrough.apply_to(&mut spec);

        assert_eq!(
            spec.create().mounts(),
            [RuntimeMount::bind(
                socket_path.to_string(),
                CONTAINER_WAYLAND_DISPLAY
            )]
        );
        assert_eq!(
            spec.create()
                .default_env()
                .get(HOST_WAYLAND_DISPLAY_ENV)
                .map(String::as_str),
            Some(CONTAINER_WAYLAND_DISPLAY)
        );
        assert!(
            !spec
                .create()
                .default_env()
                .contains_key(HOST_XDG_RUNTIME_DIR_ENV)
        );
    }

    #[cfg(unix)]
    #[test]
    fn absolute_display_does_not_require_runtime_directory() {
        let (_socket_dir, socket_path, _listener) = bind_test_socket();
        let mut requested_environment = Vec::new();

        let passthrough = detect_with(
            &mut |name| {
                requested_environment.push(name.to_string());
                match name {
                    HOST_WAYLAND_DISPLAY_ENV => Some(socket_path.as_os_str().to_os_string()),
                    _ => None,
                }
            },
            &mut panic_warning,
        )
        .unwrap();

        assert_eq!(passthrough.host_socket, socket_path);
        assert_eq!(requested_environment, [HOST_WAYLAND_DISPLAY_ENV]);
    }

    #[test]
    fn unresolved_relative_display_warns_and_disables_passthrough() {
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            &mut |name| match name {
                HOST_WAYLAND_DISPLAY_ENV => Some(OsString::from("wayland-0")),
                _ => None,
            },
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, None);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn unusable_socket_warns_and_disables_passthrough() {
        let sandbox = tempfile::tempdir().unwrap();
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            &mut |name| match name {
                HOST_WAYLAND_DISPLAY_ENV => Some(OsString::from("missing.sock")),
                HOST_XDG_RUNTIME_DIR_ENV => Some(sandbox.path().as_os_str().to_os_string()),
                _ => None,
            },
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, None);
        assert_eq!(warnings.len(), 1);
    }

    fn empty_run_spec() -> RuntimeRunSpec {
        RuntimeRunSpec::new(RuntimeCreateSpec::builder("image").build(), "/repo")
    }

    fn panic_warning(warning: String) {
        panic!("unexpected warning: {warning}");
    }

    #[cfg(unix)]
    fn bind_test_socket() -> (
        tempfile::TempDir,
        Utf8PathBuf,
        std::os::unix::net::UnixListener,
    ) {
        let sandbox = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(sandbox.path().join("wayland-7")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket_path.as_std_path()).unwrap();
        (sandbox, socket_path, listener)
    }
}

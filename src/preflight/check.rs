// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use std::collections::BTreeMap;

use crate::runtime::{RuntimeKind, RuntimeMount, RuntimeRunMode};
use crate::{Error, Result};

use super::{
    NIX_DAEMON_SOCKET_PATH, PreflightSnapshot,
    mounts::{host_nix_mounts, runtime_mounts},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub host_nix_mounts: Vec<RuntimeMount>,
    pub runtime_mounts: Vec<RuntimeMount>,
    pub runtime_environment: BTreeMap<String, String>,
}

pub fn check_host_prerequisites_for_runtime(runtime: RuntimeKind) -> Result<PreflightReport> {
    check_host_prerequisites_for_runtime_mode(runtime, RuntimeRunMode::ManagedSession)
}

pub fn check_host_prerequisites_for_runtime_mode(
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot_for_mode(
        &PreflightSnapshot::detect_for_run_mode(runtime, run_mode),
        runtime,
        run_mode,
    )
}

pub fn check_host_prerequisites_with_snapshot(
    snapshot: &PreflightSnapshot,
    runtime: RuntimeKind,
) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot_for_mode(
        snapshot,
        runtime,
        RuntimeRunMode::ManagedSession,
    )
}

pub fn check_host_prerequisites_with_snapshot_for_mode(
    snapshot: &PreflightSnapshot,
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
) -> Result<PreflightReport> {
    PreflightCheck {
        snapshot,
        runtime,
        run_mode,
    }
    .run()
}

struct PreflightCheck<'a> {
    snapshot: &'a PreflightSnapshot,
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
}

impl PreflightCheck<'_> {
    fn run(&self) -> Result<PreflightReport> {
        self.validate_host_tools()?;
        self.validate_nix_daemon()?;
        let nix_client_source = self.nix_client_source()?;
        self.validate_nix_config()?;
        let runtime_preflight = runtime_mounts(self.snapshot, self.runtime, self.run_mode)?;

        Ok(PreflightReport {
            host_nix_mounts: host_nix_mounts(
                nix_client_source,
                self.snapshot.nix.config.custom_conf.needs_static_mount,
            ),
            runtime_mounts: runtime_preflight.mounts,
            runtime_environment: runtime_preflight.environment,
        })
    }

    fn validate_host_tools(&self) -> Result<()> {
        if !self.snapshot.host.has_git {
            return Err(Error::msg(
                "`git` was not found on PATH; install `git` or add it to PATH",
            ));
        }

        if !self.snapshot.host.has_podman {
            return Err(Error::msg(
                "`podman` was not found on PATH; install `podman` or add it to PATH",
            ));
        }

        Ok(())
    }

    fn validate_nix_daemon(&self) -> Result<()> {
        if !self.snapshot.nix.has_daemon_socket {
            return Err(Error::msg(format!(
                "Missing host nix-daemon socket at: {NIX_DAEMON_SOCKET_PATH}. Mount /nix:/nix:ro."
            )));
        }

        Ok(())
    }

    fn nix_client_source(&self) -> Result<&Utf8Path> {
        self.snapshot.nix.client_source.as_deref().ok_or_else(|| {
            Error::msg(
                "`nix` was not found on PATH; install Nix or add the host `nix` client to PATH",
            )
        })
    }

    fn validate_nix_config(&self) -> Result<()> {
        let config = &self.snapshot.nix.config;

        if !config.has_etc_nix_mount {
            return Err(Error::msg(
                "Missing /etc/nix host mount. Mount /etc/nix:/etc/nix:ro so the wrapper inherits the host config and registry.",
            ));
        }

        if !config.has_readable_nix_conf {
            return Err(Error::msg(
                "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro.",
            ));
        }

        if config.custom_conf.present && !config.custom_conf.has_readable_target {
            return Err(Error::msg(
                "Missing readable target for /etc/nix/nix.custom.conf. Mount /etc/static/nix:/etc/static/nix:ro when /etc/nix points there.",
            ));
        }

        Ok(())
    }
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::runtime::{RuntimeHostStateMount, RuntimeHostStateSource, RuntimeKind, RuntimeMount};
use crate::{Error, Result};

use super::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, HostDirectoryPreflightSnapshot,
    NIX_CLIENT_DESTINATION, NIX_DAEMON_SOCKET_PATH, NIX_STORE_DESTINATION, PreflightSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub host_nix_mounts: Vec<RuntimeMount>,
    pub runtime_mounts: Vec<RuntimeMount>,
}

pub fn check_host_prerequisites_for_runtime(runtime: RuntimeKind) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot(&PreflightSnapshot::detect(runtime), runtime)
}

pub fn check_host_prerequisites_with_snapshot(
    snapshot: &PreflightSnapshot,
    runtime: RuntimeKind,
) -> Result<PreflightReport> {
    PreflightCheck { snapshot, runtime }.run()
}

struct PreflightCheck<'a> {
    snapshot: &'a PreflightSnapshot,
    runtime: RuntimeKind,
}

impl PreflightCheck<'_> {
    fn run(&self) -> Result<PreflightReport> {
        self.validate_host_tools()?;
        self.validate_nix_daemon()?;
        let nix_client_source = self.nix_client_source()?;
        self.validate_nix_config()?;
        let runtime_mounts = self.runtime_mounts()?;

        Ok(PreflightReport {
            host_nix_mounts: host_nix_mounts(
                nix_client_source,
                self.snapshot.nix.config.custom_conf.needs_static_mount,
            ),
            runtime_mounts,
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

    fn runtime_mounts(&self) -> Result<Vec<RuntimeMount>> {
        self.runtime
            .host_state_mounts()
            .iter()
            .map(|spec| {
                let state = self.host_state_snapshot(spec)?;
                HostStateMountRequirement {
                    runtime: self.runtime,
                    spec,
                    state,
                }
                .validate()
            })
            .collect()
    }

    fn host_state_snapshot(
        &self,
        spec: &RuntimeHostStateMount,
    ) -> Result<&HostDirectoryPreflightSnapshot> {
        self.snapshot
            .host
            .runtime_state
            .get(spec.destination)
            .ok_or_else(|| {
                Error::msg(format!(
                    "missing preflight snapshot for runtime host-state mount `{}`",
                    spec.destination
                ))
            })
    }
}

struct HostStateMountRequirement<'a> {
    runtime: RuntimeKind,
    spec: &'a RuntimeHostStateMount,
    state: &'a HostDirectoryPreflightSnapshot,
}

impl<'a> HostStateMountRequirement<'a> {
    fn validate(self) -> Result<RuntimeMount> {
        let Some(source) = self.state.source.as_ref() else {
            return Err(self.missing_source_error());
        };

        if !self.state.exists {
            let source_expression = self.spec.source_expression();
            return Err(Error::msg(format!(
                "Missing host {} {} directory: {source}. Run `{}` on the host first so {} exists, then retry `agentbox run --runtime {}`.",
                self.spec.product_name,
                self.spec.description,
                self.runtime,
                source_expression,
                self.runtime,
            )));
        }

        if !self.state.is_directory {
            return Err(Error::msg(format!(
                "Host {} {} path is not a directory: {source}",
                self.spec.product_name, self.spec.description,
            )));
        }

        if !self.state.readable || !self.state.writable || !self.state.searchable {
            return Err(Error::msg(format!(
                "Host {} {} directory is not readable and writable: {source}",
                self.spec.product_name, self.spec.description,
            )));
        }

        Ok(RuntimeMount::bind(
            source.to_string(),
            self.spec.destination,
        ))
    }

    fn missing_source_error(&self) -> Error {
        let source_expression = self.spec.source_expression();
        match self.spec.source {
            RuntimeHostStateSource::HomeOnly { .. } => Error::msg(format!(
                "`HOME` is not set; cannot locate host {} {} directory {} for `run --runtime {}`",
                self.spec.product_name, self.spec.description, source_expression, self.runtime,
            )),
            RuntimeHostStateSource::XdgOrHome { .. } => Error::msg(format!(
                "Cannot locate host {} {} directory {} for `run --runtime {}`; set `HOME` or the matching XDG environment variable, then retry.",
                self.spec.product_name, self.spec.description, source_expression, self.runtime,
            )),
        }
    }
}

const HOST_NIX_MOUNTS: &[HostNixMountSpec] = &[
    HostNixMountSpec::SamePath(NIX_STORE_DESTINATION),
    HostNixMountSpec::NixClient(NIX_CLIENT_DESTINATION),
    HostNixMountSpec::SamePath(ETC_NIX_DESTINATION),
    HostNixMountSpec::StaticNix(ETC_STATIC_NIX_DESTINATION),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostNixMountSpec {
    SamePath(&'static str),
    NixClient(&'static str),
    StaticNix(&'static str),
}

impl HostNixMountSpec {
    fn mount(self, nix_client_source: &Utf8Path) -> RuntimeMount {
        match self {
            Self::SamePath(path) | Self::StaticNix(path) => {
                RuntimeMount::read_only_bind(path, path)
            }
            Self::NixClient(destination) => {
                RuntimeMount::read_only_bind(nix_client_source.to_string(), destination)
            }
        }
    }

    fn is_included(self, include_static_nix_mount: bool) -> bool {
        !matches!(self, Self::StaticNix(_)) || include_static_nix_mount
    }

    fn destination(self) -> &'static str {
        match self {
            Self::SamePath(destination)
            | Self::NixClient(destination)
            | Self::StaticNix(destination) => destination,
        }
    }
}

fn host_nix_mounts(
    nix_client_source: &Utf8Path,
    include_static_nix_mount: bool,
) -> Vec<RuntimeMount> {
    HOST_NIX_MOUNTS
        .iter()
        .copied()
        .filter(|mount| mount.is_included(include_static_nix_mount))
        .map(|mount| mount.mount(nix_client_source))
        .collect()
}

pub fn required_host_mount_destinations() -> Vec<&'static str> {
    HOST_NIX_MOUNTS
        .iter()
        .copied()
        .map(HostNixMountSpec::destination)
        .collect()
}

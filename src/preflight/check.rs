// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

use crate::runtime::{
    RuntimeHostStateMount, RuntimeHostStateSource, RuntimeHostStateSourceLookup, RuntimeKind,
    RuntimeMount,
};
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

pub fn check_host_prerequisites(
    target_directory: Option<&Utf8Path>,
    git_root: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    check_host_prerequisites_for_runtime(RuntimeKind::Opencode, target_directory, git_root)
}

pub fn check_host_prerequisites_for_runtime(
    runtime: RuntimeKind,
    target_directory: Option<&Utf8Path>,
    git_root: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot(
        &PreflightSnapshot::detect(target_directory, git_root),
        target_directory,
        runtime,
    )
}

pub fn check_host_prerequisites_with_snapshot(
    snapshot: &PreflightSnapshot,
    target_directory: Option<&Utf8Path>,
    runtime: RuntimeKind,
) -> Result<PreflightReport> {
    PreflightCheck {
        snapshot,
        target_directory,
        runtime,
    }
    .run()
}

struct PreflightCheck<'a> {
    snapshot: &'a PreflightSnapshot,
    target_directory: Option<&'a Utf8Path>,
    runtime: RuntimeKind,
}

impl PreflightCheck<'_> {
    fn run(&self) -> Result<PreflightReport> {
        self.validate_host_tools()?;
        self.validate_direnv()?;
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

    fn validate_direnv(&self) -> Result<()> {
        if self.snapshot.host.direnv.required && !self.snapshot.host.direnv.available {
            let target = self
                .target_directory
                .map(ToString::to_string)
                .unwrap_or_else(|| ".".to_string());
            return Err(Error::msg(format!(
                "`.envrc` applies to `{target}`, but `direnv` was not found on PATH; install `direnv` or add it to PATH"
            )));
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
                HostStateMountRequirement {
                    runtime: self.runtime,
                    spec,
                    state: self.host_state_snapshot(spec.source),
                }
                .validate()
            })
            .collect()
    }

    fn host_state_snapshot(
        &self,
        source: RuntimeHostStateSource,
    ) -> &HostDirectoryPreflightSnapshot {
        match source {
            RuntimeHostStateSource::CodexConfig => &self.snapshot.host.codex,
            RuntimeHostStateSource::OpenCodeConfig => &self.snapshot.host.opencode.config,
            RuntimeHostStateSource::OpenCodeData => &self.snapshot.host.opencode.data,
        }
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
            return Err(Error::msg(format!(
                "Missing host {} {} directory: {source}. Run `{}` on the host first so {} exists, then retry `agentbox run --runtime {}`.",
                self.spec.product_name,
                self.spec.description,
                self.runtime,
                self.spec.source_expression,
                self.runtime,
            )));
        }

        if !self.state.is_directory {
            return Err(Error::msg(format!(
                "Host {} {} path is not a directory: {source}",
                self.spec.product_name, self.spec.description,
            )));
        }

        if !self.state.readable || !self.state.writable {
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
        match self.spec.source_lookup {
            RuntimeHostStateSourceLookup::HomeOnly => Error::msg(format!(
                "`HOME` is not set; cannot locate host {} {} directory {} for `run --runtime {}`",
                self.spec.product_name,
                self.spec.description,
                self.spec.source_expression,
                self.runtime,
            )),
            RuntimeHostStateSourceLookup::XdgOrHome => Error::msg(format!(
                "Cannot locate host {} {} directory {} for `run --runtime {}`; set `HOME` or the matching XDG environment variable, then retry.",
                self.spec.product_name,
                self.spec.description,
                self.spec.source_expression,
                self.runtime,
            )),
        }
    }
}

fn host_nix_mounts(
    nix_client_source: &Utf8Path,
    include_static_nix_mount: bool,
) -> Vec<RuntimeMount> {
    let mut mounts = vec![RuntimeMount::read_only_bind(
        NIX_STORE_DESTINATION,
        NIX_STORE_DESTINATION,
    )];
    mounts.push(RuntimeMount::read_only_bind(
        nix_client_source.to_string(),
        NIX_CLIENT_DESTINATION,
    ));
    mounts.push(RuntimeMount::read_only_bind(
        ETC_NIX_DESTINATION,
        ETC_NIX_DESTINATION,
    ));

    if include_static_nix_mount {
        mounts.push(RuntimeMount::read_only_bind(
            ETC_STATIC_NIX_DESTINATION,
            ETC_STATIC_NIX_DESTINATION,
        ));
    }

    mounts
}

pub fn required_host_mount_destinations() -> [&'static str; 4] {
    [
        NIX_STORE_DESTINATION,
        NIX_CLIENT_DESTINATION,
        ETC_NIX_DESTINATION,
        ETC_STATIC_NIX_DESTINATION,
    ]
}

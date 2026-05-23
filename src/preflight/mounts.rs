// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::runtime::{
    RuntimeHostStateMount, RuntimeHostStateSource, RuntimeKind, RuntimeMount, RuntimeRunMode,
};
use crate::{Error, Result};

use super::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, HostDirectoryPreflightSnapshot,
    NIX_CLIENT_DESTINATION, NIX_STORE_DESTINATION, PreflightSnapshot,
};

pub(super) fn runtime_mounts(
    snapshot: &PreflightSnapshot,
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
) -> Result<RuntimeHostStatePreflight> {
    let mut preflight = RuntimeHostStatePreflight::default();

    for spec in runtime.host_state_mounts(run_mode) {
        let state = host_state_snapshot(snapshot, spec)?;
        let requirement = HostStateMountRequirement {
            runtime,
            spec,
            state,
        }
        .validate()?;

        preflight.mounts.push(requirement.mount);
        preflight.environment.extend(requirement.environment);
    }

    Ok(preflight)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RuntimeHostStatePreflight {
    pub(super) mounts: Vec<RuntimeMount>,
    pub(super) environment: BTreeMap<String, String>,
}

fn host_state_snapshot<'a>(
    snapshot: &'a PreflightSnapshot,
    spec: &RuntimeHostStateMount,
) -> Result<&'a HostDirectoryPreflightSnapshot> {
    snapshot
        .host
        .runtime_state
        .get(spec.snapshot_key())
        .ok_or_else(|| {
            Error::msg(format!(
                "missing preflight snapshot for runtime host-state mount `{}`",
                spec.snapshot_key()
            ))
        })
}

struct ValidatedHostStateMount {
    mount: RuntimeMount,
    environment: BTreeMap<String, String>,
}

struct HostStateMountRequirement<'a> {
    runtime: RuntimeKind,
    spec: &'a RuntimeHostStateMount,
    state: &'a HostDirectoryPreflightSnapshot,
}

impl<'a> HostStateMountRequirement<'a> {
    fn validate(self) -> Result<ValidatedHostStateMount> {
        if let Some(error) = &self.state.source_error {
            return Err(Error::msg(error.clone()));
        }

        let Some(source) = self.state.source.as_ref() else {
            return Err(self.missing_source_error());
        };

        let destination = self.spec.destination_for(
            source.as_ref(),
            self.state.source_environment_variable.as_deref(),
        )?;
        let environment = self
            .spec
            .container_environment_for(
                source.as_ref(),
                self.state.source_environment_variable.as_deref(),
            )
            .into_iter()
            .collect();

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

        Ok(ValidatedHostStateMount {
            mount: RuntimeMount::bind(source.to_string(), destination),
            environment,
        })
    }

    fn missing_source_error(&self) -> Error {
        let source_expression = self.spec.source_expression();
        match self.spec.source {
            RuntimeHostStateSource::EnvironmentOrHome { .. } => Error::msg(format!(
                "Cannot locate host {} {} directory {source_expression} for `run --runtime {}`; set `CODEX_HOME` or `HOME`, then retry.",
                self.spec.product_name, self.spec.description, self.runtime,
            )),
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

pub(super) fn host_nix_mounts(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::{
        CODEX_CONFIG_DESTINATION, HostPreflightSnapshot, NixConfigPreflightSnapshot,
        NixCustomConfPreflightSnapshot, NixPreflightSnapshot,
    };

    #[test]
    fn host_nix_mounts_include_static_mount_only_when_needed() {
        let nix = Utf8Path::new("/run/current-system/sw/bin/nix");

        let normal_mounts = host_nix_mounts(nix, false);
        assert_eq!(
            normal_mounts
                .iter()
                .map(RuntimeMount::destination)
                .collect::<Vec<_>>(),
            vec![
                NIX_STORE_DESTINATION,
                NIX_CLIENT_DESTINATION,
                ETC_NIX_DESTINATION,
            ]
        );

        let static_mounts = host_nix_mounts(nix, true);
        assert_eq!(
            static_mounts
                .iter()
                .map(RuntimeMount::destination)
                .collect::<Vec<_>>(),
            vec![
                NIX_STORE_DESTINATION,
                NIX_CLIENT_DESTINATION,
                ETC_NIX_DESTINATION,
                ETC_STATIC_NIX_DESTINATION,
            ]
        );
    }

    #[test]
    fn runtime_mounts_validate_runtime_host_state_snapshots() {
        let snapshot = snapshot_with_codex_host_state(HostDirectoryPreflightSnapshot {
            source: Some("/home/example/.codex".into()),
            source_environment_variable: None,
            source_error: None,
            exists: true,
            is_directory: true,
            readable: true,
            writable: true,
            searchable: true,
        });

        let preflight = runtime_mounts(
            &snapshot,
            RuntimeKind::Codex,
            RuntimeRunMode::ManagedSession,
        )
        .unwrap();

        assert!(preflight.environment.is_empty());
        assert_eq!(preflight.mounts.len(), 1);
        assert_eq!(preflight.mounts[0].source(), "/home/example/.codex");
        assert_eq!(preflight.mounts[0].destination(), CODEX_CONFIG_DESTINATION);
    }

    #[test]
    fn runtime_mounts_pass_codex_home_to_server_containers() {
        let snapshot = snapshot_with_codex_host_state(HostDirectoryPreflightSnapshot {
            source: Some("/custom/codex".into()),
            source_environment_variable: Some("CODEX_HOME".to_string()),
            source_error: None,
            exists: true,
            is_directory: true,
            readable: true,
            writable: true,
            searchable: true,
        });

        let preflight = runtime_mounts(
            &snapshot,
            RuntimeKind::Codex,
            RuntimeRunMode::ManagedSession,
        )
        .unwrap();

        assert_eq!(preflight.mounts.len(), 1);
        assert_eq!(preflight.mounts[0].source(), "/custom/codex");
        assert_eq!(preflight.mounts[0].destination(), "/custom/codex");
        assert_eq!(
            preflight.environment,
            [("CODEX_HOME".to_string(), "/custom/codex".to_string())].into()
        );
    }

    #[test]
    fn runtime_mounts_report_missing_home_source_for_home_only_mounts() {
        let snapshot = snapshot_with_codex_host_state(HostDirectoryPreflightSnapshot {
            source: None,
            source_environment_variable: None,
            source_error: None,
            exists: false,
            is_directory: false,
            readable: false,
            writable: false,
            searchable: false,
        });

        let error =
            runtime_mounts(&snapshot, RuntimeKind::Codex, RuntimeRunMode::Foreground).unwrap_err();

        assert_eq!(
            error.to_string(),
            "`HOME` is not set; cannot locate host Codex configuration directory `${HOME}/.codex` for `run --runtime codex`"
        );
    }

    fn snapshot_with_codex_host_state(state: HostDirectoryPreflightSnapshot) -> PreflightSnapshot {
        PreflightSnapshot {
            host: HostPreflightSnapshot {
                has_git: true,
                has_podman: true,
                runtime_state: [(CODEX_CONFIG_DESTINATION.to_string(), state)].into(),
            },
            nix: NixPreflightSnapshot {
                has_daemon_socket: true,
                client_source: Some("/run/current-system/sw/bin/nix".into()),
                config: NixConfigPreflightSnapshot {
                    has_etc_nix_mount: true,
                    has_readable_nix_conf: true,
                    custom_conf: NixCustomConfPreflightSnapshot {
                        present: false,
                        has_readable_target: true,
                        needs_static_mount: false,
                    },
                },
            },
        }
    }
}

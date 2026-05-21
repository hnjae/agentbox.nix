// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::runtime::{RuntimeHostStateMount, RuntimeHostStateSource, RuntimeKind, RuntimeMount};
use crate::{Error, Result};

use super::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, HostDirectoryPreflightSnapshot,
    NIX_CLIENT_DESTINATION, NIX_STORE_DESTINATION, PreflightSnapshot,
};

pub(super) fn runtime_mounts(
    snapshot: &PreflightSnapshot,
    runtime: RuntimeKind,
) -> Result<Vec<RuntimeMount>> {
    runtime
        .host_state_mounts()
        .iter()
        .map(|spec| {
            let state = host_state_snapshot(snapshot, spec)?;
            HostStateMountRequirement {
                runtime,
                spec,
                state,
            }
            .validate()
        })
        .collect()
}

fn host_state_snapshot<'a>(
    snapshot: &'a PreflightSnapshot,
    spec: &RuntimeHostStateMount,
) -> Result<&'a HostDirectoryPreflightSnapshot> {
    snapshot
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
            exists: true,
            is_directory: true,
            readable: true,
            writable: true,
            searchable: true,
        });

        let mounts = runtime_mounts(&snapshot, RuntimeKind::Codex).unwrap();

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source(), "/home/example/.codex");
        assert_eq!(mounts[0].destination(), CODEX_CONFIG_DESTINATION);
    }

    #[test]
    fn runtime_mounts_report_missing_home_source_for_home_only_mounts() {
        let snapshot = snapshot_with_codex_host_state(HostDirectoryPreflightSnapshot {
            source: None,
            exists: false,
            is_directory: false,
            readable: false,
            writable: false,
            searchable: false,
        });

        let error = runtime_mounts(&snapshot, RuntimeKind::Codex).unwrap_err();

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

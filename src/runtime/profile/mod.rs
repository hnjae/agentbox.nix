// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod codex;
mod opencode;

use crate::Result;

use super::command::{DirectCommandTemplate, HostClientCommandTemplate, ServerCommandTemplate};
use super::default_image::DefaultImageBuildContext;
use super::host_state::RuntimeHostStateMount;
use super::kind::RuntimeKind;
use super::spec::{RuntimeAttachSpec, RuntimeHealthCheck, RuntimeRunMode};

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";
const NPM_INSTALL_SOURCE: &str = "npm";

const RUNTIME_PROFILES: &[RuntimeProfile] = &[opencode::PROFILE, codex::PROFILE];

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeProfile {
    pub(super) kind: RuntimeKind,
    pub(super) name: &'static str,
    pub(super) materialize_default_image_context: fn() -> Result<DefaultImageBuildContext>,
    pub(super) package: RuntimePackageSpec,
    pub(super) attach: RuntimeAttachSpec,
    pub(super) health_check: RuntimeHealthCheck,
    pub(super) host_state_mounts: fn(RuntimeRunMode) -> &'static [RuntimeHostStateMount],
    pub(super) default_env: &'static [RuntimeDefaultEnv],
    pub(super) server_command: ServerCommandTemplate,
    pub(super) host_client_command: HostClientCommandTemplate,
    pub(super) foreground_command: DirectCommandTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimePackageSpec {
    pub(crate) name: &'static str,
    pub(crate) install_source: &'static str,
    pub(crate) build_arg: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeDefaultEnv {
    pub(super) name: &'static str,
    pub(super) value: &'static str,
}

pub(super) fn runtime_profile(kind: RuntimeKind) -> &'static RuntimeProfile {
    RUNTIME_PROFILES
        .iter()
        .find(|profile| profile.kind == kind)
        .unwrap_or_else(|| panic!("missing runtime profile for `{kind:?}`"))
}

#[cfg(test)]
fn all_host_state_mounts(
    run_mode: RuntimeRunMode,
) -> impl Iterator<Item = &'static RuntimeHostStateMount> {
    RUNTIME_PROFILES
        .iter()
        .flat_map(move |profile| (profile.host_state_mounts)(run_mode).iter())
}

pub(super) fn runtime_kind_from_name(value: &str) -> Option<RuntimeKind> {
    RUNTIME_PROFILES
        .iter()
        .find(|profile| profile.name == value)
        .map(|profile| profile.kind)
}

pub(super) fn supported_runtime_names() -> String {
    backticked_runtime_names().join(" and ")
}

pub(super) fn supported_runtime_placeholder() -> String {
    format!("<{}>", runtime_names().join("|"))
}

pub(super) fn supported_runtime_values() -> Vec<&'static str> {
    runtime_names()
}

fn runtime_names() -> Vec<&'static str> {
    RUNTIME_PROFILES
        .iter()
        .map(|profile| profile.name)
        .collect()
}

fn backticked_runtime_names() -> Vec<String> {
    runtime_names()
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
}

#[cfg(test)]
fn runtime_kinds() -> &'static [RuntimeKind] {
    <RuntimeKind as clap::ValueEnum>::value_variants()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::preflight::{
        CODEX_CONFIG_DESTINATION, OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION,
    };

    use super::*;

    #[test]
    fn runtime_profile_table_covers_value_enum_variants() {
        let mut profile_kinds = RUNTIME_PROFILES
            .iter()
            .map(|profile| profile.kind)
            .collect::<Vec<_>>();
        profile_kinds.sort_by_key(|kind| kind.as_str());
        profile_kinds.dedup();

        let mut value_enum_kinds = runtime_kinds().to_vec();
        value_enum_kinds.sort_by_key(|kind| kind.as_str());

        assert_eq!(profile_kinds, value_enum_kinds);
    }

    #[test]
    fn runtime_host_state_mount_destinations_are_unique() {
        for run_mode in [
            RuntimeRunMode::ManagedSession,
            RuntimeRunMode::TransientServer,
            RuntimeRunMode::Foreground,
        ] {
            let mut destinations = BTreeSet::new();
            for mount in all_host_state_mounts(run_mode) {
                assert!(
                    destinations.insert(mount.snapshot_key()),
                    "duplicate runtime host-state mount destination `{}` for `{run_mode:?}`",
                    mount.snapshot_key(),
                );
            }
        }
    }

    #[test]
    fn runtime_profile_server_host_state_sources_match_expected_locations() {
        let expressions = all_host_state_mounts(RuntimeRunMode::ManagedSession)
            .map(|mount| (mount.snapshot_key(), mount.source_expression()))
            .collect::<Vec<_>>();

        assert_eq!(
            expressions,
            vec![
                (
                    OPENCODE_CONFIG_DESTINATION,
                    "`${XDG_CONFIG_HOME:-$HOME/.config}/opencode`".to_string(),
                ),
                (
                    OPENCODE_DATA_DESTINATION,
                    "`${XDG_DATA_HOME:-$HOME/.local/share}/opencode`".to_string(),
                ),
                (
                    CODEX_CONFIG_DESTINATION,
                    "`CODEX_HOME` or `$HOME/.codex`".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn runtime_profile_foreground_host_state_sources_match_expected_locations() {
        let expressions = all_host_state_mounts(RuntimeRunMode::Foreground)
            .map(|mount| (mount.snapshot_key(), mount.source_expression()))
            .collect::<Vec<_>>();

        assert_eq!(
            expressions,
            vec![
                (
                    OPENCODE_CONFIG_DESTINATION,
                    "`${XDG_CONFIG_HOME:-$HOME/.config}/opencode`".to_string(),
                ),
                (
                    OPENCODE_DATA_DESTINATION,
                    "`${XDG_DATA_HOME:-$HOME/.local/share}/opencode`".to_string(),
                ),
                (CODEX_CONFIG_DESTINATION, "`${HOME}/.codex`".to_string()),
            ]
        );
    }
}

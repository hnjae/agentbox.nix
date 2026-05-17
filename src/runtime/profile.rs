// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::preflight::{
    CODEX_CONFIG_DESTINATION, OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION,
};

use super::command::{
    DirectCommandArg, DirectCommandTemplate, HostClientCommandArg, HostClientCommandTemplate,
    ServerCommandArg, ServerCommandTemplate,
};
use super::default_image::{self, DefaultImageBuildContext};
use super::host_state::{RuntimeHostStateMount, RuntimeHostStateSource};
use super::kind::RuntimeKind;
use super::spec::{RuntimeAttachSpec, RuntimeHealthCheck, RuntimeHealthResponsePolicy};

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";
const NPM_INSTALL_SOURCE: &str = "npm";

const OPENCODE_NPM_PACKAGE: &str = "opencode-ai";
const CODEX_NPM_PACKAGE: &str = "@openai/codex";
const CODEX_YOLO_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";
const OPENCODE_HEALTH_PATH: &str = "/global/health";
const CODEX_READY_PATH: &str = "/readyz";

const OPENCODE_SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("opencode"),
    ServerCommandArg::Literal("serve"),
    ServerCommandArg::Literal("--hostname"),
    ServerCommandArg::ContainerListenIp,
    ServerCommandArg::Literal("--port"),
    ServerCommandArg::ContainerPort,
]);
const OPENCODE_HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("opencode"),
    HostClientCommandArg::Literal("attach"),
    HostClientCommandArg::AttachEndpoint,
]);
const OPENCODE_FOREGROUND_COMMAND: DirectCommandTemplate =
    DirectCommandTemplate::new(&[DirectCommandArg::Literal("opencode")]);

const CODEX_SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("codex"),
    ServerCommandArg::Literal(CODEX_YOLO_FLAG),
    ServerCommandArg::Literal("app-server"),
    ServerCommandArg::Literal("--listen"),
    ServerCommandArg::ContainerListenEndpoint,
]);
// Codex 0.128.0 requires the YOLO flag on the attaching `codex --remote`
// client as well as on the app-server process.
const CODEX_HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("codex"),
    HostClientCommandArg::Literal(CODEX_YOLO_FLAG),
    HostClientCommandArg::Literal("--remote"),
    HostClientCommandArg::AttachEndpoint,
]);
const CODEX_FOREGROUND_COMMAND: DirectCommandTemplate = DirectCommandTemplate::new(&[
    DirectCommandArg::Literal("codex"),
    DirectCommandArg::Literal(CODEX_YOLO_FLAG),
]);

const OPENCODE_DEFAULT_ENV: &[RuntimeDefaultEnv] = &[
    RuntimeDefaultEnv {
        name: "OPENCODE_CONFIG_CONTENT",
        value: r#"{"autoupdate":false}"#,
    },
    RuntimeDefaultEnv {
        name: "OPENCODE_PERMISSION",
        value: r#"{"*":"allow"}"#,
    },
];
const CODEX_DEFAULT_ENV: &[RuntimeDefaultEnv] = &[];

const OPENCODE_HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[
    RuntimeHostStateMount {
        source: RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_CONFIG_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".config", "opencode"],
        },
        product_name: "OpenCode",
        description: "configuration",
        destination: OPENCODE_CONFIG_DESTINATION,
    },
    RuntimeHostStateMount {
        source: RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_DATA_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".local", "share", "opencode"],
        },
        product_name: "OpenCode",
        description: "data",
        destination: OPENCODE_DATA_DESTINATION,
    },
];
const CODEX_HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[RuntimeHostStateMount {
    source: RuntimeHostStateSource::HomeOnly {
        home_relative_components: &[".codex"],
    },
    product_name: "Codex",
    description: "configuration",
    destination: CODEX_CONFIG_DESTINATION,
}];

const OPENCODE_PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Opencode,
    name: "opencode",
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: OPENCODE_NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "OPENCODE_NPM_VERSION",
    },
    attach: RuntimeAttachSpec {
        scheme: "http",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 4096,
    },
    health_check: RuntimeHealthCheck {
        path: OPENCODE_HEALTH_PATH,
        response_policy: RuntimeHealthResponsePolicy::JsonHealthyFlag,
    },
    host_state_mounts: OPENCODE_HOST_STATE_MOUNTS,
    default_env: OPENCODE_DEFAULT_ENV,
    server_command: OPENCODE_SERVER_COMMAND,
    host_client_command: OPENCODE_HOST_CLIENT_COMMAND,
    foreground_command: OPENCODE_FOREGROUND_COMMAND,
};

const CODEX_PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Codex,
    name: "codex",
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: CODEX_NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "CODEX_NPM_VERSION",
    },
    attach: RuntimeAttachSpec {
        scheme: "ws",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 1455,
    },
    health_check: RuntimeHealthCheck {
        path: CODEX_READY_PATH,
        response_policy: RuntimeHealthResponsePolicy::HttpOk,
    },
    host_state_mounts: CODEX_HOST_STATE_MOUNTS,
    default_env: CODEX_DEFAULT_ENV,
    server_command: CODEX_SERVER_COMMAND,
    host_client_command: CODEX_HOST_CLIENT_COMMAND,
    foreground_command: CODEX_FOREGROUND_COMMAND,
};

const RUNTIME_PROFILES: &[RuntimeProfile] = &[OPENCODE_PROFILE, CODEX_PROFILE];

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeProfile {
    pub(super) kind: RuntimeKind,
    pub(super) name: &'static str,
    pub(super) materialize_default_image_context: fn() -> Result<DefaultImageBuildContext>,
    pub(super) package: RuntimePackageSpec,
    pub(super) attach: RuntimeAttachSpec,
    pub(super) health_check: RuntimeHealthCheck,
    pub(super) host_state_mounts: &'static [RuntimeHostStateMount],
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
fn all_host_state_mounts() -> impl Iterator<Item = &'static RuntimeHostStateMount> {
    RUNTIME_PROFILES
        .iter()
        .flat_map(|profile| profile.host_state_mounts.iter())
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
        let mut destinations = BTreeSet::new();

        for mount in all_host_state_mounts() {
            assert!(
                destinations.insert(mount.destination),
                "duplicate runtime host-state mount destination `{}`",
                mount.destination,
            );
        }
    }

    #[test]
    fn runtime_profile_host_state_sources_match_expected_locations() {
        let expressions = all_host_state_mounts()
            .map(|mount| (mount.destination, mount.source_expression()))
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

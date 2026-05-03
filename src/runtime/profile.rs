// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;
use crate::metadata::{
    LABEL_CODEX_INSTALL_SOURCE, LABEL_CODEX_PACKAGE, LABEL_CODEX_RESOLVED_AT, LABEL_CODEX_VERSION,
    LABEL_OPENCODE_INSTALL_SOURCE, LABEL_OPENCODE_PACKAGE, LABEL_OPENCODE_RESOLVED_AT,
    LABEL_OPENCODE_VERSION,
};

use super::command::{
    HostClientCommandArg, HostClientCommandTemplate, ServerCommandArg, ServerCommandTemplate,
};
use super::default_image::{self, DefaultImageBuildContext};
use super::kind::RuntimeKind;
use super::spec::RuntimeAttachSpec;

const CONTAINER_LISTEN_IP: &str = "0.0.0.0";
const NPM_INSTALL_SOURCE: &str = "npm";

const OPENCODE_NPM_PACKAGE: &str = "opencode-ai";
const CODEX_NPM_PACKAGE: &str = "@openai/codex";

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

const CODEX_SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("codex"),
    ServerCommandArg::Literal("--dangerously-bypass-approvals-and-sandbox"),
    ServerCommandArg::Literal("app-server"),
    ServerCommandArg::Literal("--listen"),
    ServerCommandArg::ContainerListenEndpoint,
]);
const CODEX_HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("codex"),
    HostClientCommandArg::Literal("--remote"),
    HostClientCommandArg::AttachEndpoint,
]);

const OPENCODE_DEFAULT_ENV: &[RuntimeDefaultEnv] = &[RuntimeDefaultEnv {
    name: "OPENCODE_CONFIG_CONTENT",
    value: r#"{"autoupdate":false}"#,
}];
const CODEX_DEFAULT_ENV: &[RuntimeDefaultEnv] = &[];

const OPENCODE_PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Opencode,
    name: "opencode",
    default_image: default_image::OPENCODE_DEFAULT_IMAGE,
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: OPENCODE_NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "OPENCODE_NPM_VERSION",
        package_label: LABEL_OPENCODE_PACKAGE,
        version_label: LABEL_OPENCODE_VERSION,
        install_source_label: LABEL_OPENCODE_INSTALL_SOURCE,
        resolved_at_label: LABEL_OPENCODE_RESOLVED_AT,
    },
    attach: RuntimeAttachSpec {
        scheme: "http",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 4096,
    },
    default_env: OPENCODE_DEFAULT_ENV,
    server_command: OPENCODE_SERVER_COMMAND,
    host_client_command: OPENCODE_HOST_CLIENT_COMMAND,
};

const CODEX_PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Codex,
    name: "codex",
    default_image: default_image::CODEX_DEFAULT_IMAGE,
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: CODEX_NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "CODEX_NPM_VERSION",
        package_label: LABEL_CODEX_PACKAGE,
        version_label: LABEL_CODEX_VERSION,
        install_source_label: LABEL_CODEX_INSTALL_SOURCE,
        resolved_at_label: LABEL_CODEX_RESOLVED_AT,
    },
    attach: RuntimeAttachSpec {
        scheme: "ws",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 1455,
    },
    default_env: CODEX_DEFAULT_ENV,
    server_command: CODEX_SERVER_COMMAND,
    host_client_command: CODEX_HOST_CLIENT_COMMAND,
};

const RUNTIME_PROFILES: &[RuntimeProfile] = &[OPENCODE_PROFILE, CODEX_PROFILE];

#[derive(Debug, Clone, Copy)]
pub(super) struct RuntimeProfile {
    pub(super) kind: RuntimeKind,
    pub(super) name: &'static str,
    pub(super) default_image: &'static str,
    pub(super) materialize_default_image_context: fn() -> Result<DefaultImageBuildContext>,
    pub(super) package: RuntimePackageSpec,
    pub(super) attach: RuntimeAttachSpec,
    pub(super) default_env: &'static [RuntimeDefaultEnv],
    pub(super) server_command: ServerCommandTemplate,
    pub(super) host_client_command: HostClientCommandTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimePackageSpec {
    pub(crate) name: &'static str,
    pub(crate) install_source: &'static str,
    pub(crate) build_arg: &'static str,
    pub(crate) package_label: &'static str,
    pub(crate) version_label: &'static str,
    pub(crate) install_source_label: &'static str,
    pub(crate) resolved_at_label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeDefaultEnv {
    pub(super) name: &'static str,
    pub(super) value: &'static str,
}

pub(super) fn runtime_profile(kind: RuntimeKind) -> &'static RuntimeProfile {
    match kind {
        RuntimeKind::Opencode => &OPENCODE_PROFILE,
        RuntimeKind::Codex => &CODEX_PROFILE,
    }
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
}

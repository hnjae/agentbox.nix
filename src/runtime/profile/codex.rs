// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::preflight::CODEX_CONFIG_DESTINATION;
use crate::runtime::CODEX_REMOTE_TOKEN_ENV;
use crate::runtime::command::{
    DirectCommandArg, DirectCommandTemplate, HostClientCommandArg, HostClientCommandTemplate,
    ServerCommandArg, ServerCommandTemplate,
};
use crate::runtime::default_image;
use crate::runtime::host_state::{
    RuntimeHostStateContainerEnvironment, RuntimeHostStateDestination, RuntimeHostStateMount,
    RuntimeHostStateSource,
};
use crate::runtime::kind::RuntimeKind;
use crate::runtime::spec::{
    RuntimeAttachSpec, RuntimeHealthCheck, RuntimeHealthResponsePolicy, RuntimeRunMode,
};

use super::{
    CONTAINER_LISTEN_IP, NPM_INSTALL_SOURCE, RuntimeDefaultEnv, RuntimePackageSpec, RuntimeProfile,
};

const NPM_PACKAGE: &str = "@openai/codex";
const YOLO_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";
const READY_PATH: &str = "/readyz";
const CODEX_HOME_ENV: &str = "CODEX_HOME";

const SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("codex"),
    ServerCommandArg::Literal(YOLO_FLAG),
    ServerCommandArg::Literal("app-server"),
    ServerCommandArg::Literal("--listen"),
    ServerCommandArg::ContainerListenEndpoint,
]);
// Codex 0.128.0 requires the YOLO flag on the attaching `codex --remote`
// client as well as on the app-server process.
const HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("codex"),
    HostClientCommandArg::Literal(YOLO_FLAG),
    HostClientCommandArg::Literal("--remote"),
    HostClientCommandArg::AttachEndpoint,
    HostClientCommandArg::Literal("--remote-auth-token-env"),
    HostClientCommandArg::Literal(CODEX_REMOTE_TOKEN_ENV),
]);
const FOREGROUND_COMMAND: DirectCommandTemplate = DirectCommandTemplate::new(&[
    DirectCommandArg::Literal("codex"),
    DirectCommandArg::Literal(YOLO_FLAG),
]);

const DEFAULT_ENV: &[RuntimeDefaultEnv] = &[];

const SERVER_HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[RuntimeHostStateMount {
    source: RuntimeHostStateSource::EnvironmentOrHome {
        environment_variable: CODEX_HOME_ENV,
        home_relative_components: &[".codex"],
    },
    product_name: "Codex",
    description: "configuration",
    destination: RuntimeHostStateDestination::SourcePathWhenEnvironment {
        environment_variable: CODEX_HOME_ENV,
        fallback_destination: CODEX_CONFIG_DESTINATION,
    },
    container_environment: Some(RuntimeHostStateContainerEnvironment {
        name: CODEX_HOME_ENV,
        source_environment_variable: CODEX_HOME_ENV,
    }),
}];

const FOREGROUND_HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[RuntimeHostStateMount {
    source: RuntimeHostStateSource::HomeOnly {
        home_relative_components: &[".codex"],
    },
    product_name: "Codex",
    description: "configuration",
    destination: RuntimeHostStateDestination::Fixed(CODEX_CONFIG_DESTINATION),
    container_environment: None,
}];

fn host_state_mounts(run_mode: RuntimeRunMode) -> &'static [RuntimeHostStateMount] {
    match run_mode {
        RuntimeRunMode::Foreground => FOREGROUND_HOST_STATE_MOUNTS,
        RuntimeRunMode::ManagedSession | RuntimeRunMode::TransientServer => {
            SERVER_HOST_STATE_MOUNTS
        }
    }
}

pub(super) const PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Codex,
    name: "codex",
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "CODEX_NPM_VERSION",
    },
    attach: RuntimeAttachSpec {
        scheme: "ws",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 1455,
    },
    health_check: RuntimeHealthCheck {
        path: READY_PATH,
        response_policy: RuntimeHealthResponsePolicy::HttpOk,
    },
    host_state_mounts,
    default_env: DEFAULT_ENV,
    server_command: SERVER_COMMAND,
    host_client_command: HOST_CLIENT_COMMAND,
    foreground_command: FOREGROUND_COMMAND,
};

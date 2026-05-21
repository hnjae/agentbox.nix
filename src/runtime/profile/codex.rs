// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::preflight::CODEX_CONFIG_DESTINATION;
use crate::runtime::command::{
    DirectCommandArg, DirectCommandTemplate, HostClientCommandArg, HostClientCommandTemplate,
    ServerCommandArg, ServerCommandTemplate,
};
use crate::runtime::default_image;
use crate::runtime::host_state::{RuntimeHostStateMount, RuntimeHostStateSource};
use crate::runtime::kind::RuntimeKind;
use crate::runtime::spec::{RuntimeAttachSpec, RuntimeHealthCheck, RuntimeHealthResponsePolicy};

use super::{
    CONTAINER_LISTEN_IP, NPM_INSTALL_SOURCE, RuntimeDefaultEnv, RuntimePackageSpec, RuntimeProfile,
};

const NPM_PACKAGE: &str = "@openai/codex";
const YOLO_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";
const READY_PATH: &str = "/readyz";

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
]);
const FOREGROUND_COMMAND: DirectCommandTemplate = DirectCommandTemplate::new(&[
    DirectCommandArg::Literal("codex"),
    DirectCommandArg::Literal(YOLO_FLAG),
]);

const DEFAULT_ENV: &[RuntimeDefaultEnv] = &[];

const HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[RuntimeHostStateMount {
    source: RuntimeHostStateSource::HomeOnly {
        home_relative_components: &[".codex"],
    },
    product_name: "Codex",
    description: "configuration",
    destination: CODEX_CONFIG_DESTINATION,
}];

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
    host_state_mounts: HOST_STATE_MOUNTS,
    default_env: DEFAULT_ENV,
    server_command: SERVER_COMMAND,
    host_client_command: HOST_CLIENT_COMMAND,
    foreground_command: FOREGROUND_COMMAND,
};

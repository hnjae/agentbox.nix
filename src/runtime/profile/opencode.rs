// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::preflight::{OPENCODE_CONFIG_DESTINATION, OPENCODE_DATA_DESTINATION};
use crate::runtime::command::{
    DirectCommandArg, DirectCommandTemplate, HostClientCommandArg, HostClientCommandTemplate,
    ServerCommandArg, ServerCommandTemplate,
};
use crate::runtime::default_image;
use crate::runtime::host_state::{
    RuntimeHostStateDestination, RuntimeHostStateMount, RuntimeHostStateSource,
};
use crate::runtime::kind::RuntimeKind;
use crate::runtime::spec::{
    RuntimeAttachSpec, RuntimeHealthCheck, RuntimeHealthResponsePolicy, RuntimeRunMode,
};

use super::{
    CONTAINER_LISTEN_IP, NPM_INSTALL_SOURCE, RuntimeDefaultEnv, RuntimePackageSpec, RuntimeProfile,
};

const NPM_PACKAGE: &str = "opencode-ai";
const HEALTH_PATH: &str = "/global/health";

const SERVER_COMMAND: ServerCommandTemplate = ServerCommandTemplate::new(&[
    ServerCommandArg::Literal("opencode"),
    ServerCommandArg::Literal("serve"),
    ServerCommandArg::Literal("--hostname"),
    ServerCommandArg::ContainerListenIp,
    ServerCommandArg::Literal("--port"),
    ServerCommandArg::ContainerPort,
]);
const HOST_CLIENT_COMMAND: HostClientCommandTemplate = HostClientCommandTemplate::new(&[
    HostClientCommandArg::Literal("opencode"),
    HostClientCommandArg::Literal("attach"),
    HostClientCommandArg::AttachEndpoint,
]);
const FOREGROUND_COMMAND: DirectCommandTemplate =
    DirectCommandTemplate::new(&[DirectCommandArg::Literal("opencode")]);

const DEFAULT_ENV: &[RuntimeDefaultEnv] = &[
    RuntimeDefaultEnv {
        name: "OPENCODE_CONFIG_CONTENT",
        value: r#"{"autoupdate":false}"#,
    },
    RuntimeDefaultEnv {
        name: "OPENCODE_PERMISSION",
        value: r#"{"*":"allow"}"#,
    },
];

const HOST_STATE_MOUNTS: &[RuntimeHostStateMount] = &[
    RuntimeHostStateMount {
        source: RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_CONFIG_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".config", "opencode"],
        },
        product_name: "OpenCode",
        description: "configuration",
        destination: RuntimeHostStateDestination::Fixed(OPENCODE_CONFIG_DESTINATION),
        container_environment: None,
    },
    RuntimeHostStateMount {
        source: RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_DATA_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".local", "share", "opencode"],
        },
        product_name: "OpenCode",
        description: "data",
        destination: RuntimeHostStateDestination::Fixed(OPENCODE_DATA_DESTINATION),
        container_environment: None,
    },
];

fn host_state_mounts(_run_mode: RuntimeRunMode) -> &'static [RuntimeHostStateMount] {
    HOST_STATE_MOUNTS
}

pub(super) const PROFILE: RuntimeProfile = RuntimeProfile {
    kind: RuntimeKind::Opencode,
    name: "opencode",
    materialize_default_image_context: default_image::materialize_default_image_context,
    package: RuntimePackageSpec {
        name: NPM_PACKAGE,
        install_source: NPM_INSTALL_SOURCE,
        build_arg: "OPENCODE_NPM_VERSION",
    },
    attach: RuntimeAttachSpec {
        scheme: "http",
        container_listen_ip: CONTAINER_LISTEN_IP,
        container_port: 4096,
    },
    health_check: RuntimeHealthCheck {
        path: HEALTH_PATH,
        response_policy: RuntimeHealthResponsePolicy::JsonHealthyFlag,
    },
    host_state_mounts,
    default_env: DEFAULT_ENV,
    server_command: SERVER_COMMAND,
    host_client_command: HOST_CLIENT_COMMAND,
    foreground_command: FOREGROUND_COMMAND,
};

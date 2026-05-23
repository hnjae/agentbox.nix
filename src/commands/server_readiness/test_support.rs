// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::metadata::{ManagedSessionLabelInput, managed_session_labels};
use crate::podman::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerState, PodmanHostConfig,
    PodmanNetworkSettings, PodmanPortBinding,
};
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::workspace::{WorkspaceIdentity, test_support::WorkspaceIdentityFixture};

pub(super) fn workspace() -> WorkspaceIdentity {
    WorkspaceIdentityFixture::demo().build()
}

pub(super) fn endpoint() -> AttachEndpoint {
    AttachEndpoint {
        scheme: "http".to_string(),
        host_ip: "127.0.0.1".to_string(),
        host_port: 49152,
    }
}

pub(super) fn running_inspect(
    workspace: &WorkspaceIdentity,
    host_port: Option<u16>,
) -> PodmanContainerInspect {
    let runtime = RuntimeKind::Opencode;
    let image = runtime.default_image();
    let labels = managed_session_labels(ManagedSessionLabelInput {
        canonical_git_root: workspace.canonical_git_root.as_str(),
        git_root_hash: workspace.hash12.as_str(),
        runtime,
        image: &image,
        launch_directory: workspace.canonical_target.as_str(),
        logical_name: workspace.container_name.as_str(),
        server_args: &[],
    });

    PodmanContainerInspect {
        id: "container-id".to_string(),
        path: "/usr/bin/opencode".to_string(),
        state: PodmanContainerState {
            status: "running".to_string(),
            running: true,
            pid: 4321,
            ..PodmanContainerState::default()
        },
        image_name: image,
        config: PodmanContainerConfig {
            labels,
            ..PodmanContainerConfig::default()
        },
        host_config: PodmanHostConfig {
            network_mode: Some("bridge".to_string()),
            ..PodmanHostConfig::default()
        },
        network_settings: network_settings(host_port),
        ..PodmanContainerInspect::default()
    }
}

fn network_settings(host_port: Option<u16>) -> PodmanNetworkSettings {
    let ports = host_port
        .map(|host_port| {
            BTreeMap::from([(
                "4096/tcp".to_string(),
                Some(vec![PodmanPortBinding {
                    host_ip: Some("127.0.0.1".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            )])
        })
        .unwrap_or_default();

    PodmanNetworkSettings {
        ports,
        ..PodmanNetworkSettings::default()
    }
}

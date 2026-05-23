// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::runtime::RuntimeKind;
use crate::workspace::WorkspaceIdentity;

use super::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_KIND, LABEL_CONTAINER_KIND_MANAGED_SESSION_VALUE,
    LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT,
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME,
    LABEL_MANAGED, LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
    LABEL_SERVER_ARGS,
};

/// Input values for constructing the complete managed-session container label set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSessionLabelInput<'a> {
    pub canonical_git_root: &'a str,
    pub git_root_hash: &'a str,
    pub runtime: RuntimeKind,
    pub image: &'a str,
    pub launch_directory: &'a str,
    pub logical_name: &'a str,
    pub server_args: &'a [String],
}

impl<'a> ManagedSessionLabelInput<'a> {
    pub fn from_workspace(
        workspace: &'a WorkspaceIdentity,
        image: &'a str,
        runtime: RuntimeKind,
        server_args: &'a [String],
    ) -> Self {
        Self {
            canonical_git_root: workspace.canonical_git_root.as_str(),
            git_root_hash: workspace.hash12.as_str(),
            runtime,
            image,
            launch_directory: workspace.canonical_target.as_str(),
            logical_name: workspace.container_name.as_str(),
            server_args,
        }
    }
}

/// Builds the complete label set stored on managed session containers.
pub fn managed_session_labels(input: ManagedSessionLabelInput<'_>) -> BTreeMap<String, String> {
    let mut labels = runtime_server_container_labels(input);
    labels.insert(LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string());
    labels.insert(
        LABEL_CONTAINER_KIND.to_string(),
        LABEL_CONTAINER_KIND_MANAGED_SESSION_VALUE.to_string(),
    );
    labels.insert(LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string());
    if !input.server_args.is_empty() {
        labels.insert(
            LABEL_SERVER_ARGS.to_string(),
            serde_json::to_string(input.server_args)
                .expect("serializing runtime server arguments should not fail"),
        );
    }
    labels
}

/// Builds the complete label set stored on transient run containers.
pub fn transient_run_labels(input: ManagedSessionLabelInput<'_>) -> BTreeMap<String, String> {
    let mut labels = runtime_server_container_labels(input);
    labels.insert(
        LABEL_CONTAINER_KIND.to_string(),
        LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE.to_string(),
    );
    labels
}

fn runtime_server_container_labels(
    input: ManagedSessionLabelInput<'_>,
) -> BTreeMap<String, String> {
    let attach = input.runtime.attach_spec();

    BTreeMap::from([
        (
            LABEL_GIT_ROOT.to_string(),
            input.canonical_git_root.to_string(),
        ),
        (
            LABEL_GIT_ROOT_HASH.to_string(),
            input.git_root_hash.to_string(),
        ),
        (
            LABEL_RUNTIME.to_string(),
            input.runtime.as_str().to_string(),
        ),
        (LABEL_IMAGE.to_string(), input.image.to_string()),
        (
            LABEL_LAUNCH_DIRECTORY.to_string(),
            input.launch_directory.to_string(),
        ),
        (
            LABEL_LOGICAL_NAME.to_string(),
            input.logical_name.to_string(),
        ),
        (LABEL_ATTACH_SCHEME.to_string(), attach.scheme.to_string()),
        (
            LABEL_CONTAINER_PORT.to_string(),
            attach.container_port.to_string(),
        ),
        (
            LABEL_CONTAINER_LISTEN_IP.to_string(),
            attach.container_listen_ip.to_string(),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_session_labels_include_legacy_and_kind_markers() {
        let labels = managed_session_labels(label_input());

        assert_eq!(labels[LABEL_MANAGED], LABEL_MANAGED_VALUE);
        assert_eq!(
            labels[LABEL_CONTAINER_KIND],
            LABEL_CONTAINER_KIND_MANAGED_SESSION_VALUE
        );
        assert_eq!(labels[LABEL_SCHEMA], LABEL_SCHEMA_VALUE);
    }

    #[test]
    fn transient_run_labels_use_kind_marker_without_legacy_session_marker() {
        let labels = transient_run_labels(label_input());

        assert_eq!(
            labels[LABEL_CONTAINER_KIND],
            LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE
        );
        assert!(!labels.contains_key(LABEL_MANAGED));
        assert!(!labels.contains_key(LABEL_SCHEMA));
    }

    fn label_input() -> ManagedSessionLabelInput<'static> {
        ManagedSessionLabelInput {
            canonical_git_root: "/workspace/demo",
            git_root_hash: "0123456789ab",
            runtime: RuntimeKind::Codex,
            image: "localhost/agentbox-codex:latest",
            launch_directory: "/workspace/demo",
            logical_name: "agentbox-demo",
            server_args: &[],
        }
    }
}

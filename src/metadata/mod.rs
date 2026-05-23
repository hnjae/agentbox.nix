// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::runtime::RuntimeKind;

mod default_image;
mod session;

pub(crate) use default_image::{
    DefaultRuntimeImageLabelInput, DefaultRuntimeImageMetadata, default_runtime_image_labels,
};
pub use session::{ManagedSessionLabelInput, managed_session_labels, transient_run_labels};

pub const LABEL_MANAGED: &str = "io.agentbox.managed";
pub const LABEL_CONTAINER_KIND: &str = "io.agentbox.container_kind";
pub const LABEL_SCHEMA: &str = "io.agentbox.schema";
pub const LABEL_GIT_ROOT: &str = "io.agentbox.git_root";
pub const LABEL_GIT_ROOT_HASH: &str = "io.agentbox.git_root_hash";
pub const LABEL_RUNTIME: &str = "io.agentbox.runtime";
pub const LABEL_IMAGE: &str = "io.agentbox.image";
pub const LABEL_LAUNCH_DIRECTORY: &str = "io.agentbox.launch_directory";
pub const LABEL_LOGICAL_NAME: &str = "io.agentbox.logical_name";
pub const LABEL_ATTACH_SCHEME: &str = "io.agentbox.attach_scheme";
pub const LABEL_CONTAINER_PORT: &str = "io.agentbox.container_port";
pub const LABEL_CONTAINER_LISTEN_IP: &str = "io.agentbox.container_listen_ip";
pub const LABEL_SERVER_ARGS: &str = "io.agentbox.server_args";
pub const LABEL_DEFAULT_RUNTIME_IMAGE: &str = "io.agentbox.default_runtime_image";
pub const LABEL_IMAGE_CONTEXT_HASH: &str = "io.agentbox.image_context_hash";

pub const LABEL_MANAGED_VALUE: &str = "true";
pub const LABEL_CONTAINER_KIND_MANAGED_SESSION_VALUE: &str = "managed-session";
pub const LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE: &str = "transient-run";
pub const LABEL_SCHEMA_VALUE: &str = "1";
pub const LABEL_DEFAULT_RUNTIME_IMAGE_VALUE: &str = "true";

pub const REQUIRED_SESSION_MARKER_LABEL_VALUES: &[(&str, &str)] = &[
    (LABEL_MANAGED, LABEL_MANAGED_VALUE),
    (LABEL_SCHEMA, LABEL_SCHEMA_VALUE),
];

pub const REQUIRED_SESSION_WORKSPACE_IDENTITY_LABELS: &[&str] =
    &[LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH];

pub const REQUIRED_SESSION_METADATA_LABELS: &[&str] =
    &[LABEL_IMAGE, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentboxContainerKind {
    Managed,
    Run,
}

impl AgentboxContainerKind {
    pub fn output_type(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Run => "run",
        }
    }
}

pub(crate) fn managed_label_filter() -> String {
    format!("label={LABEL_MANAGED}={LABEL_MANAGED_VALUE}")
}

pub(crate) fn agentbox_container_kind_from_labels(
    labels: &BTreeMap<String, String>,
) -> Option<AgentboxContainerKind> {
    if required_label_value(labels, LABEL_MANAGED) == Some(LABEL_MANAGED_VALUE) {
        return Some(AgentboxContainerKind::Managed);
    }

    if required_label_value(labels, LABEL_CONTAINER_KIND)
        == Some(LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE)
    {
        return Some(AgentboxContainerKind::Run);
    }

    None
}

pub(crate) fn default_runtime_image_label_filter() -> String {
    format!("label={LABEL_DEFAULT_RUNTIME_IMAGE}={LABEL_DEFAULT_RUNTIME_IMAGE_VALUE}")
}

pub(crate) fn runtime_package_version_label(runtime: RuntimeKind) -> String {
    runtime_package_metadata_label(runtime, "version")
}

fn runtime_package_metadata_label(runtime: RuntimeKind, name: &str) -> String {
    format!("io.agentbox.{}.{name}", runtime.as_str())
}

pub(crate) fn required_label_value<'a>(
    labels: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a str> {
    labels
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_package_label_names_are_derived_from_runtime_name() {
        assert_eq!(
            runtime_package_version_label(RuntimeKind::Opencode),
            "io.agentbox.opencode.version"
        );
        assert_eq!(
            runtime_package_version_label(RuntimeKind::Codex),
            "io.agentbox.codex.version"
        );
    }

    #[test]
    fn container_kind_parser_accepts_legacy_and_current_markers() {
        assert_eq!(
            agentbox_container_kind_from_labels(&BTreeMap::from([(
                LABEL_MANAGED.to_string(),
                LABEL_MANAGED_VALUE.to_string(),
            )])),
            Some(AgentboxContainerKind::Managed)
        );
        assert_eq!(
            agentbox_container_kind_from_labels(&BTreeMap::from([(
                LABEL_CONTAINER_KIND.to_string(),
                LABEL_CONTAINER_KIND_TRANSIENT_RUN_VALUE.to_string(),
            )])),
            Some(AgentboxContainerKind::Run)
        );
    }
}

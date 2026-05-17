// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::runtime::{RuntimeKind, default_image};
use crate::workspace::WorkspaceIdentity;

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

/// Input values for constructing the complete default runtime image label set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefaultRuntimeImageLabelInput<'a> {
    pub(crate) runtime: RuntimeKind,
    pub(crate) image: &'a str,
    pub(crate) image_context_hash: &'a str,
    pub(crate) version: &'a str,
    pub(crate) resolved_at: &'a str,
}

/// Validated metadata recovered from labels on a default runtime image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefaultRuntimeImageMetadata<'a> {
    runtime: RuntimeKind,
    image_context_hash: &'a str,
}

impl<'a> DefaultRuntimeImageMetadata<'a> {
    pub(crate) fn from_labels(labels: &'a BTreeMap<String, String>) -> Option<Self> {
        if required_label_value(labels, LABEL_DEFAULT_RUNTIME_IMAGE)
            != Some(LABEL_DEFAULT_RUNTIME_IMAGE_VALUE)
        {
            return None;
        }

        let runtime = required_label_value(labels, LABEL_RUNTIME)?
            .parse::<RuntimeKind>()
            .ok()?;
        let image_context_hash = required_label_value(labels, LABEL_IMAGE_CONTEXT_HASH)
            .filter(|hash| default_image::is_default_image_context_hash(hash))?;

        Some(Self {
            runtime,
            image_context_hash,
        })
    }

    pub(crate) fn runtime(self) -> RuntimeKind {
        self.runtime
    }

    pub(crate) fn image_context_hash(self) -> &'a str {
        self.image_context_hash
    }
}

/// Builds the complete label set stored on default runtime images.
pub(crate) fn default_runtime_image_labels(
    input: DefaultRuntimeImageLabelInput<'_>,
) -> BTreeMap<String, String> {
    let package = input.runtime.package_spec();
    let runtime = input.runtime;

    BTreeMap::from([
        (
            LABEL_DEFAULT_RUNTIME_IMAGE.to_string(),
            LABEL_DEFAULT_RUNTIME_IMAGE_VALUE.to_string(),
        ),
        (
            LABEL_RUNTIME.to_string(),
            input.runtime.as_str().to_string(),
        ),
        (LABEL_IMAGE.to_string(), input.image.to_string()),
        (
            LABEL_IMAGE_CONTEXT_HASH.to_string(),
            input.image_context_hash.to_string(),
        ),
        (
            runtime_package_metadata_label(runtime, "package"),
            package.name.to_string(),
        ),
        (
            runtime_package_version_label(runtime),
            input.version.to_string(),
        ),
        (
            runtime_package_metadata_label(runtime, "install_source"),
            package.install_source.to_string(),
        ),
        (
            runtime_package_metadata_label(runtime, "resolved_at"),
            input.resolved_at.to_string(),
        ),
    ])
}

/// Input values for constructing the complete managed-session container label set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSessionLabelInput<'a> {
    pub canonical_git_root: &'a str,
    pub git_root_hash: &'a str,
    pub runtime: RuntimeKind,
    pub image: &'a str,
    pub launch_directory: &'a str,
    pub logical_name: &'a str,
}

impl<'a> ManagedSessionLabelInput<'a> {
    pub fn from_workspace(
        workspace: &'a WorkspaceIdentity,
        image: &'a str,
        runtime: RuntimeKind,
    ) -> Self {
        Self {
            canonical_git_root: workspace.canonical_git_root.as_str(),
            git_root_hash: workspace.hash12.as_str(),
            runtime,
            image,
            launch_directory: workspace.canonical_target.as_str(),
            logical_name: workspace.container_name.as_str(),
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
    fn default_runtime_image_labels_round_trip_through_metadata_parser() {
        let runtime = RuntimeKind::Codex;
        let image = runtime.default_image();
        let image_context_hash = default_image::default_image_context_hash();

        let labels = default_runtime_image_labels(DefaultRuntimeImageLabelInput {
            runtime,
            image: &image,
            image_context_hash,
            version: "1.2.3",
            resolved_at: "12345",
        });

        let metadata = DefaultRuntimeImageMetadata::from_labels(&labels).unwrap();

        assert_eq!(metadata.runtime(), runtime);
        assert_eq!(metadata.image_context_hash(), image_context_hash);
        assert_eq!(labels["io.agentbox.codex.package"], "@openai/codex");
        assert_eq!(labels["io.agentbox.codex.version"], "1.2.3");
        assert_eq!(labels["io.agentbox.codex.install_source"], "npm");
        assert_eq!(labels["io.agentbox.codex.resolved_at"], "12345");
    }

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
    fn default_runtime_image_metadata_rejects_invalid_marker_or_hash() {
        let runtime = RuntimeKind::Opencode;
        let image = runtime.default_image();
        let mut labels = default_runtime_image_labels(DefaultRuntimeImageLabelInput {
            runtime,
            image: &image,
            image_context_hash: default_image::default_image_context_hash(),
            version: "1.2.3",
            resolved_at: "12345",
        });

        labels.insert(LABEL_DEFAULT_RUNTIME_IMAGE.to_string(), "false".to_string());
        assert_eq!(DefaultRuntimeImageMetadata::from_labels(&labels), None);

        labels.insert(
            LABEL_DEFAULT_RUNTIME_IMAGE.to_string(),
            LABEL_DEFAULT_RUNTIME_IMAGE_VALUE.to_string(),
        );
        labels.insert(
            LABEL_IMAGE_CONTEXT_HASH.to_string(),
            "not-a-hash".to_string(),
        );
        assert_eq!(DefaultRuntimeImageMetadata::from_labels(&labels), None);
    }
}

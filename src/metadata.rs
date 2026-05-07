// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use crate::runtime::{RuntimeKind, default_image};
use crate::workspace::WorkspaceIdentity;

pub const LABEL_MANAGED: &str = "io.agentbox.managed";
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
pub const LABEL_CODEX_PACKAGE: &str = "io.agentbox.codex.package";
pub const LABEL_CODEX_VERSION: &str = "io.agentbox.codex.version";
pub const LABEL_CODEX_INSTALL_SOURCE: &str = "io.agentbox.codex.install_source";
pub const LABEL_CODEX_RESOLVED_AT: &str = "io.agentbox.codex.resolved_at";
pub const LABEL_OPENCODE_PACKAGE: &str = "io.agentbox.opencode.package";
pub const LABEL_OPENCODE_VERSION: &str = "io.agentbox.opencode.version";
pub const LABEL_OPENCODE_INSTALL_SOURCE: &str = "io.agentbox.opencode.install_source";
pub const LABEL_OPENCODE_RESOLVED_AT: &str = "io.agentbox.opencode.resolved_at";
pub const LABEL_DEFAULT_RUNTIME_IMAGE: &str = "io.agentbox.default_runtime_image";
pub const LABEL_IMAGE_CONTEXT_HASH: &str = "io.agentbox.image_context_hash";

pub const LABEL_MANAGED_VALUE: &str = "true";
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

pub(crate) fn managed_label_filter() -> String {
    format!("label={LABEL_MANAGED}={LABEL_MANAGED_VALUE}")
}

pub(crate) fn default_runtime_image_label_filter() -> String {
    format!("label={LABEL_DEFAULT_RUNTIME_IMAGE}={LABEL_DEFAULT_RUNTIME_IMAGE_VALUE}")
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
        (package.package_label.to_string(), package.name.to_string()),
        (package.version_label.to_string(), input.version.to_string()),
        (
            package.install_source_label.to_string(),
            package.install_source.to_string(),
        ),
        (
            package.resolved_at_label.to_string(),
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
    let attach = input.runtime.attach_spec();

    BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
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
        let image_context_hash = runtime.default_image_context_hash();

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
        assert_eq!(labels[LABEL_CODEX_PACKAGE], "@openai/codex");
        assert_eq!(labels[LABEL_CODEX_VERSION], "1.2.3");
        assert_eq!(labels[LABEL_CODEX_INSTALL_SOURCE], "npm");
        assert_eq!(labels[LABEL_CODEX_RESOLVED_AT], "12345");
    }

    #[test]
    fn default_runtime_image_metadata_rejects_invalid_marker_or_hash() {
        let runtime = RuntimeKind::Opencode;
        let image = runtime.default_image();
        let mut labels = default_runtime_image_labels(DefaultRuntimeImageLabelInput {
            runtime,
            image: &image,
            image_context_hash: runtime.default_image_context_hash(),
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

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::runtime::{RuntimeKind, default_image};

use super::{
    LABEL_DEFAULT_RUNTIME_IMAGE, LABEL_DEFAULT_RUNTIME_IMAGE_VALUE, LABEL_IMAGE,
    LABEL_IMAGE_CONTEXT_HASH, LABEL_RUNTIME, required_label_value, runtime_package_metadata_label,
    runtime_package_version_label,
};

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

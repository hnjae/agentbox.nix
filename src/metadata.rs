// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use crate::runtime::RuntimeAttachSpec;
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

pub const LABEL_MANAGED_VALUE: &str = "true";
pub const LABEL_SCHEMA_VALUE: &str = "1";

pub const REQUIRED_SESSION_MARKER_LABEL_VALUES: &[(&str, &str)] = &[
    (LABEL_MANAGED, LABEL_MANAGED_VALUE),
    (LABEL_SCHEMA, LABEL_SCHEMA_VALUE),
];

pub const REQUIRED_SESSION_IDENTITY_LABELS: &[&str] = &[
    LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH,
    LABEL_IMAGE,
    LABEL_LAUNCH_DIRECTORY,
    LABEL_LOGICAL_NAME,
];

pub(crate) fn managed_label_filter() -> String {
    format!("label={LABEL_MANAGED}={LABEL_MANAGED_VALUE}")
}

/// Input values for constructing the complete managed-session container label set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSessionLabelInput<'a> {
    pub canonical_git_root: &'a str,
    pub git_root_hash: &'a str,
    pub runtime: &'a str,
    pub image: &'a str,
    pub launch_directory: &'a str,
    pub logical_name: &'a str,
    pub attach: RuntimeAttachSpec,
}

impl<'a> ManagedSessionLabelInput<'a> {
    pub fn from_workspace(
        workspace: &'a WorkspaceIdentity,
        image: &'a str,
        runtime: &'a str,
        attach: RuntimeAttachSpec,
    ) -> Self {
        Self {
            canonical_git_root: workspace.canonical_git_root.as_str(),
            git_root_hash: workspace.hash12.as_str(),
            runtime,
            image,
            launch_directory: workspace.canonical_target.as_str(),
            logical_name: workspace.container_name.as_str(),
            attach,
        }
    }
}

/// Builds the complete label set stored on managed session containers.
pub fn managed_session_labels(input: ManagedSessionLabelInput<'_>) -> BTreeMap<String, String> {
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
        (LABEL_RUNTIME.to_string(), input.runtime.to_string()),
        (LABEL_IMAGE.to_string(), input.image.to_string()),
        (
            LABEL_LAUNCH_DIRECTORY.to_string(),
            input.launch_directory.to_string(),
        ),
        (
            LABEL_LOGICAL_NAME.to_string(),
            input.logical_name.to_string(),
        ),
        (
            LABEL_ATTACH_SCHEME.to_string(),
            input.attach.scheme.to_string(),
        ),
        (
            LABEL_CONTAINER_PORT.to_string(),
            input.attach.container_port.to_string(),
        ),
        (
            LABEL_CONTAINER_LISTEN_IP.to_string(),
            input.attach.container_listen_ip.to_string(),
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

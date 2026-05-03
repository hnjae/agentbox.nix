// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use crate::workspace::WorkspaceIdentity;

pub const LABEL_MANAGED: &str = "io.agentbox.managed";
pub const LABEL_SCHEMA: &str = "io.agentbox.schema";
pub const LABEL_GIT_ROOT: &str = "io.agentbox.git_root";
pub const LABEL_GIT_ROOT_HASH: &str = "io.agentbox.git_root_hash";
pub const LABEL_RUNTIME: &str = "io.agentbox.runtime";
pub const LABEL_IMAGE: &str = "io.agentbox.image";
pub const LABEL_LOGICAL_NAME: &str = "io.agentbox.logical_name";
pub const LABEL_ATTACH_SCHEME: &str = "io.agentbox.attach_scheme";
pub const LABEL_CONTAINER_PORT: &str = "io.agentbox.container_port";
pub const LABEL_CONTAINER_LISTEN_IP: &str = "io.agentbox.container_listen_ip";

pub const LABEL_MANAGED_VALUE: &str = "true";
pub const LABEL_SCHEMA_VALUE: &str = "1";

pub const REQUIRED_SESSION_LABELS: &[&str] = &[
    LABEL_MANAGED,
    LABEL_SCHEMA,
    LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH,
    LABEL_RUNTIME,
    LABEL_IMAGE,
    LABEL_LOGICAL_NAME,
    LABEL_ATTACH_SCHEME,
    LABEL_CONTAINER_PORT,
    LABEL_CONTAINER_LISTEN_IP,
];

pub(crate) fn managed_label_filter() -> String {
    format!("label={LABEL_MANAGED}={LABEL_MANAGED_VALUE}")
}

pub(crate) fn managed_session_labels(
    workspace: &WorkspaceIdentity,
    image: &str,
    runtime: &str,
    attach_scheme: &str,
    container_port: u16,
    container_listen_ip: &str,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string()),
        (LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string()),
        (
            LABEL_GIT_ROOT.to_string(),
            workspace.canonical_git_root.to_string(),
        ),
        (LABEL_GIT_ROOT_HASH.to_string(), workspace.hash12.clone()),
        (LABEL_RUNTIME.to_string(), runtime.to_string()),
        (LABEL_IMAGE.to_string(), image.to_string()),
        (
            LABEL_LOGICAL_NAME.to_string(),
            workspace.container_name.clone(),
        ),
        (LABEL_ATTACH_SCHEME.to_string(), attach_scheme.to_string()),
        (LABEL_CONTAINER_PORT.to_string(), container_port.to_string()),
        (
            LABEL_CONTAINER_LISTEN_IP.to_string(),
            container_listen_ip.to_string(),
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

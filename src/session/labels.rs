// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8PathBuf;

use std::collections::BTreeMap;

use crate::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, required_label_string, required_label_value,
};
use crate::workspace::hash12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionLabels {
    pub(super) managed: Option<String>,
    pub(super) schema: Option<String>,
    pub(super) canonical_git_root: Option<Utf8PathBuf>,
    pub(super) git_root_hash: Option<String>,
    pub(super) runtime: Option<String>,
    pub(super) image: Option<String>,
    pub(super) logical_name: Option<String>,
    pub(super) attach_scheme: Option<String>,
    pub(super) container_port: Option<String>,
    pub(super) container_listen_ip: Option<String>,
}

impl SessionLabels {
    pub(super) fn from_map(labels: &BTreeMap<String, String>) -> Self {
        Self {
            managed: required_label_string(labels, LABEL_MANAGED),
            schema: required_label_string(labels, LABEL_SCHEMA),
            canonical_git_root: required_label_value(labels, LABEL_GIT_ROOT).map(Utf8PathBuf::from),
            git_root_hash: required_label_string(labels, LABEL_GIT_ROOT_HASH),
            runtime: required_label_string(labels, LABEL_RUNTIME),
            image: required_label_string(labels, LABEL_IMAGE),
            logical_name: required_label_string(labels, LABEL_LOGICAL_NAME),
            attach_scheme: required_label_string(labels, LABEL_ATTACH_SCHEME),
            container_port: required_label_string(labels, LABEL_CONTAINER_PORT),
            container_listen_ip: required_label_string(labels, LABEL_CONTAINER_LISTEN_IP),
        }
    }

    pub(super) fn has_required_values(&self) -> bool {
        self.managed.as_deref() == Some(LABEL_MANAGED_VALUE)
            && self.schema.as_deref() == Some(LABEL_SCHEMA_VALUE)
            && self.canonical_git_root.is_some()
            && self.git_root_hash.is_some()
            && self.runtime.is_some()
            && self.image.is_some()
            && self.logical_name.is_some()
            && self.attach_scheme.is_some()
            && self.container_port.is_some()
            && self.container_listen_ip.is_some()
    }

    pub(super) fn hash_matches_root(&self) -> bool {
        self.canonical_git_root
            .as_deref()
            .zip(self.git_root_hash.as_deref())
            .is_some_and(|(git_root, stored_hash)| {
                stored_hash == hash12(git_root.as_str().as_bytes())
            })
    }
}

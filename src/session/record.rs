// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::metadata::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, required_label_value,
};
use crate::runtime::{AttachEndpoint, RuntimeKind};

use super::status::SessionStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub container_id: String,
    pub container_name: String,
    pub metadata: SessionMetadata,
    pub runtime_kind: Option<RuntimeKind>,
    pub attach_endpoint: Option<AttachEndpoint>,
    pub status: SessionStatus,
}

impl SessionRecord {
    pub fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.metadata.canonical_git_root()
    }

    pub fn git_root_hash(&self) -> Option<&str> {
        self.metadata.git_root_hash()
    }

    pub fn runtime(&self) -> Option<&str> {
        self.metadata.runtime()
    }

    pub fn launch_directory(&self) -> Option<&Utf8Path> {
        self.metadata.launch_directory()
    }

    pub fn runtime_kind(&self) -> Option<RuntimeKind> {
        self.runtime_kind
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub(crate) labels: BTreeMap<String, String>,
}

impl SessionMetadata {
    pub fn from_labels(labels: &BTreeMap<String, String>) -> Self {
        Self {
            labels: labels.clone(),
        }
    }

    pub(crate) fn is_managed(&self) -> bool {
        self.label(LABEL_MANAGED) == Some(LABEL_MANAGED_VALUE)
    }

    pub(crate) fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.label(LABEL_GIT_ROOT).map(Utf8Path::new)
    }

    pub(crate) fn git_root_hash(&self) -> Option<&str> {
        self.label(LABEL_GIT_ROOT_HASH)
    }

    pub(crate) fn runtime(&self) -> Option<&str> {
        self.label(LABEL_RUNTIME)
    }

    pub(crate) fn launch_directory(&self) -> Option<&Utf8Path> {
        self.label(LABEL_LAUNCH_DIRECTORY).map(Utf8Path::new)
    }

    pub(crate) fn logical_name_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.label(LABEL_LOGICAL_NAME).unwrap_or(fallback)
    }

    pub(super) fn label(&self, name: &str) -> Option<&str> {
        required_label_value(&self.labels, name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::runtime::AttachEndpoint;

use super::status::{SessionFailure, SessionStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub container_id: String,
    pub container_name: String,
    pub metadata: SessionMetadata,
    pub attach_endpoint: Option<AttachEndpoint>,
    pub failure: Option<SessionFailure>,
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub(crate) labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

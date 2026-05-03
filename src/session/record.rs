// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
        self.metadata.canonical_git_root.as_deref()
    }

    pub fn git_root_hash(&self) -> Option<&str> {
        self.metadata.git_root_hash.as_deref()
    }

    pub fn runtime(&self) -> Option<&str> {
        self.metadata.runtime.as_deref()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMetadata {
    pub managed: Option<String>,
    pub schema: Option<String>,
    pub canonical_git_root: Option<Utf8PathBuf>,
    pub git_root_hash: Option<String>,
    pub runtime: Option<String>,
    pub image: Option<String>,
    pub logical_name: Option<String>,
    pub attach_scheme: Option<String>,
    pub container_port: Option<String>,
    pub container_listen_ip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGroup {
    pub canonical_git_root: Utf8PathBuf,
    pub sessions: Vec<SessionRecord>,
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8PathBuf;

use super::WorkspaceIdentity;

const DEFAULT_WORKSPACE_ROOT: &str = "/workspace/demo";
const DEFAULT_DIGEST64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const DEFAULT_HASH12: &str = "0123456789ab";
const DEFAULT_CONTAINER_NAME: &str = "agentbox-demo";

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceIdentityFixture {
    requested_target: Utf8PathBuf,
    absolute_target: Utf8PathBuf,
    canonical_target: Utf8PathBuf,
    canonical_git_root: Utf8PathBuf,
    digest64: String,
    hash12: String,
    container_name: String,
}

impl WorkspaceIdentityFixture {
    pub(crate) fn demo() -> Self {
        let root = Utf8PathBuf::from(DEFAULT_WORKSPACE_ROOT);
        Self {
            requested_target: root.clone(),
            absolute_target: root.clone(),
            canonical_target: root.clone(),
            canonical_git_root: root,
            digest64: DEFAULT_DIGEST64.to_string(),
            hash12: DEFAULT_HASH12.to_string(),
            container_name: DEFAULT_CONTAINER_NAME.to_string(),
        }
    }

    pub(crate) fn build(self) -> WorkspaceIdentity {
        WorkspaceIdentity {
            requested_target: self.requested_target,
            absolute_target: self.absolute_target,
            canonical_target: self.canonical_target,
            canonical_git_root: self.canonical_git_root,
            digest64: self.digest64,
            hash12: self.hash12,
            container_name: self.container_name,
        }
    }
}

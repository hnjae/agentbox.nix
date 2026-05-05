// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::git::Git;

const CONTAINER_PREFIX: &str = "agentbox-";
const MAX_CONTAINER_NAME_LEN: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIdentity {
    pub requested_target: Utf8PathBuf,
    pub absolute_target: Utf8PathBuf,
    pub canonical_target: Utf8PathBuf,
    pub canonical_git_root: Utf8PathBuf,
    pub digest64: String,
    pub hash12: String,
    pub container_name: String,
}

pub fn resolve_workspace_identity(directory: impl AsRef<Path>) -> Result<WorkspaceIdentity> {
    resolve_workspace_identity_with_git(directory, &Git::new())
}

pub fn resolve_workspace_identity_with_git(
    directory: impl AsRef<Path>,
    git: &Git,
) -> Result<WorkspaceIdentity> {
    let requested_target = absolute_path(directory.as_ref())?;
    let git_root = git_root_for(&requested_target, git)?;
    let canonical_git_root = canonicalize_utf8(&git_root)?;
    let canonical_target = canonicalize_utf8(&requested_target)?;

    if !is_within_root(&canonical_target, &canonical_git_root) {
        return Err(Error::escaped_git_target(
            requested_target.as_ref(),
            canonical_git_root.as_ref(),
        ));
    }

    let digest64 = git_root_digest64(canonical_git_root.as_ref());
    let hash12 = hash12_from_digest64(&digest64);
    let container_name = container_name_from_canonical_root(&canonical_git_root);

    Ok(WorkspaceIdentity {
        requested_target: requested_target.clone(),
        absolute_target: requested_target,
        canonical_target,
        canonical_git_root,
        digest64,
        hash12,
        container_name,
    })
}

pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    digest.into()
}

pub fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hash12(bytes: &[u8]) -> String {
    hash12_from_digest64(&digest64_for_bytes(bytes))
}

pub fn git_root_digest64(canonical_git_root: &Utf8Path) -> String {
    digest64_for_bytes(canonical_git_root.as_str().as_bytes())
}

pub fn git_root_hash12(canonical_git_root: &Utf8Path) -> String {
    hash12_from_digest64(&git_root_digest64(canonical_git_root))
}

fn digest64_for_bytes(bytes: &[u8]) -> String {
    hex_digest(&sha256_bytes(bytes))
}

fn hash12_from_digest64(digest64: &str) -> String {
    digest64.chars().take(12).collect()
}

pub fn container_name_from_canonical_root(root: impl AsRef<str>) -> String {
    let root = root.as_ref();
    let escaped = escape_root(root);
    let hash = git_root_hash12(Utf8Path::new(root));
    let separator_len = 1 + hash.len();
    let max_suffix_len = MAX_CONTAINER_NAME_LEN - CONTAINER_PREFIX.len() - separator_len;
    let suffix = if escaped.len() <= max_suffix_len {
        escaped
    } else {
        escaped[escaped.len() - max_suffix_len..].to_string()
    };

    format!("{CONTAINER_PREFIX}{suffix}-{hash}")
}

fn absolute_path(path: &Path) -> Result<Utf8PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Utf8PathBuf::from_path_buf(absolute)
        .map_err(|path| Error::msg(format!("non-utf8 path: {path:?}")))
}

fn canonicalize_utf8(path: &Utf8Path) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(std::fs::canonicalize(path.as_std_path())?)
        .map_err(|path| Error::msg(format!("non-utf8 path: {path:?}")))
}

fn git_root_for(directory: &Utf8Path, git: &Git) -> Result<Utf8PathBuf> {
    match git.resolve_toplevel(directory) {
        Ok(root) => Ok(root),
        Err(error) => Err(error.into_error(directory)),
    }
}

fn is_within_root(target: &Utf8Path, root: &Utf8Path) -> bool {
    target == root || target.starts_with(root)
}

fn escape_root(path: &str) -> String {
    path.chars()
        .map(|ch| match ch {
            '/' => '_',
            ch if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') => ch,
            _ => '-',
        })
        .collect()
}

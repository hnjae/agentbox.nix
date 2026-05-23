// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use camino::Utf8Path;

use crate::digest;
use crate::runtime::{RuntimeKind, RuntimeRunMode};
use crate::session::SessionRecord;
use crate::state::AgentboxStateRoot;
use crate::workspace::{WorkspaceIdentity, git_root_digest64};
use crate::{Error, Result};

const MANAGED_TOKEN_DIR: &str = "codex/ws-tokens";
const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexAttachToken {
    value: String,
    sha256: String,
}

impl CodexAttachToken {
    pub(super) fn generate() -> Result<Self> {
        let mut bytes = [0_u8; TOKEN_BYTES];
        OpenOptions::new()
            .read(true)
            .open("/dev/urandom")
            .map_err(|error| {
                Error::msg(format!(
                    "failed to open /dev/urandom for Codex attach token generation: {error}"
                ))
            })?
            .read_exact(&mut bytes)
            .map_err(|error| {
                Error::msg(format!("failed to generate Codex attach token: {error}"))
            })?;

        Ok(Self::from_value(digest::hex_lower(bytes)))
    }

    pub(super) fn from_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let sha256 = digest::sha256_hex(value.as_bytes());

        Self { value, sha256 }
    }

    pub(super) fn value(&self) -> &str {
        &self.value
    }

    pub(super) fn sha256(&self) -> &str {
        &self.sha256
    }
}

#[derive(Debug, Clone)]
pub(super) struct CodexAttachTokenStore {
    state_root: AgentboxStateRoot,
}

impl CodexAttachTokenStore {
    pub(super) fn from_xdg() -> Result<Self> {
        Ok(Self {
            state_root: AgentboxStateRoot::from_xdg()?,
        })
    }

    pub(super) fn create_managed(&self, workspace: &WorkspaceIdentity) -> Result<CodexAttachToken> {
        let token = CodexAttachToken::generate()?;
        self.write_token(&workspace.digest64, &token)?;
        Ok(token)
    }

    pub(super) fn load_managed(&self, workspace: &WorkspaceIdentity) -> Result<CodexAttachToken> {
        let path = self.token_path(&workspace.digest64);
        let value = fs::read_to_string(&path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => Error::msg(format!(
                "missing Codex attach token for managed session `{}` in `{}` at `{}`; restart or recreate the session before connecting",
                workspace.container_name,
                workspace.canonical_git_root,
                path.display(),
            )),
            _ => Error::msg(format!(
                "failed to read Codex attach token `{}` for managed session `{}`: {error}",
                path.display(),
                workspace.container_name,
            )),
        })?;
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(Error::msg(format!(
                "empty Codex attach token `{}` for managed session `{}`; restart or recreate the session before connecting",
                path.display(),
                workspace.container_name,
            )));
        }

        Ok(CodexAttachToken::from_value(value))
    }

    pub(super) fn remove_managed_for_git_root(&self, canonical_git_root: &Utf8Path) -> Result<()> {
        let digest64 = git_root_digest64(canonical_git_root);
        let path = self.token_path(&digest64);
        remove_file_if_exists(&path).map_err(|error| {
            Error::msg(format!(
                "failed to remove Codex attach token `{}`: {error}",
                path.display()
            ))
        })
    }

    fn write_token(&self, digest64: &str, token: &CodexAttachToken) -> Result<()> {
        let path = self.token_path(digest64);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                Error::msg(format!(
                    "failed to create Codex attach token `{}`: {error}",
                    path.display()
                ))
            })?;
        file.write_all(token.value().as_bytes())
            .and_then(|()| file.flush())
            .map_err(|error| {
                Error::msg(format!(
                    "failed to write Codex attach token `{}`: {error}",
                    path.display()
                ))
            })?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|error| {
            Error::msg(format!(
                "failed to set permissions on Codex attach token `{}`: {error}",
                path.display()
            ))
        })
    }

    fn token_path(&self, digest64: &str) -> PathBuf {
        self.state_root
            .join(MANAGED_TOKEN_DIR)
            .join(format!("{digest64}.token"))
    }
}

pub(super) fn prepare_codex_attach_token(
    runtime: RuntimeKind,
    run_mode: RuntimeRunMode,
    workspace: &WorkspaceIdentity,
) -> Result<Option<CodexAttachToken>> {
    if runtime != RuntimeKind::Codex {
        return Ok(None);
    }

    match run_mode {
        RuntimeRunMode::ManagedSession => CodexAttachTokenStore::from_xdg()?
            .create_managed(workspace)
            .map(Some),
        RuntimeRunMode::TransientServer => CodexAttachToken::generate().map(Some),
        RuntimeRunMode::Foreground => Ok(None),
    }
}

pub(super) fn load_codex_attach_token_for_client(
    runtime: RuntimeKind,
    workspace: &WorkspaceIdentity,
) -> Result<Option<CodexAttachToken>> {
    if runtime == RuntimeKind::Codex {
        CodexAttachTokenStore::from_xdg()?
            .load_managed(workspace)
            .map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn remove_codex_attach_token_for_session(session: &SessionRecord) -> Result<()> {
    if session.runtime_kind() != Some(RuntimeKind::Codex) {
        return Ok(());
    }

    let Some(canonical_git_root) = session.canonical_git_root() else {
        return Ok(());
    };

    CodexAttachTokenStore::from_xdg()?.remove_managed_for_git_root(canonical_git_root)
}

fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

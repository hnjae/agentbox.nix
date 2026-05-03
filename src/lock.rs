// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use directories::BaseDirs;
use fd_lock::{RwLock, RwLockWriteGuard};

use crate::error::{Error, Result};
use crate::workspace::{WorkspaceIdentity, hex_digest, sha256_bytes};

const STATE_DIR: &str = "agentbox";
const LOCKS_DIR: &str = "locks";

pub struct WorkspaceLock {
    path: PathBuf,
    lock: RwLock<File>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceLockStore {
    state_dir: PathBuf,
}

pub struct WorkspaceLockGuard<'a> {
    guard: RwLockWriteGuard<'a, File>,
}

impl WorkspaceLock {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn guard(&mut self) -> Result<WorkspaceLockGuard<'_>> {
        Ok(WorkspaceLockGuard {
            guard: self.lock.write()?,
        })
    }
}

impl<'a> std::ops::Deref for WorkspaceLockGuard<'a> {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> std::ops::DerefMut for WorkspaceLockGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl WorkspaceLockStore {
    pub fn from_xdg() -> Result<Self> {
        let base_dirs =
            BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
        let state_dir = base_dirs
            .state_dir()
            .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;

        Ok(Self::in_state_dir(state_dir))
    }

    pub fn in_state_dir(state_dir: impl AsRef<Path>) -> Self {
        Self {
            state_dir: state_dir.as_ref().to_path_buf(),
        }
    }

    pub fn lock_workspace(&self, identity: &WorkspaceIdentity) -> Result<WorkspaceLock> {
        self.lock_digest(&identity.digest64)
    }

    pub fn lock_git_root(&self, canonical_git_root: &Utf8Path) -> Result<WorkspaceLock> {
        self.lock_digest(git_root_digest64(canonical_git_root))
    }

    pub fn lock_path_for_digest(&self, digest64: impl AsRef<str>) -> PathBuf {
        self.state_dir
            .join(STATE_DIR)
            .join(LOCKS_DIR)
            .join(format!("{}.lock", digest64.as_ref()))
    }

    fn lock_digest(&self, digest64: impl AsRef<str>) -> Result<WorkspaceLock> {
        let path = self.lock_path_for_digest(digest64);
        let file = open_lock_file(&path)?;

        Ok(WorkspaceLock {
            path,
            lock: RwLock::new(file),
        })
    }
}

pub fn lock_workspace(identity: &WorkspaceIdentity) -> Result<WorkspaceLock> {
    WorkspaceLockStore::from_xdg()?.lock_workspace(identity)
}

pub fn lock_git_root(canonical_git_root: &Utf8Path) -> Result<WorkspaceLock> {
    WorkspaceLockStore::from_xdg()?.lock_git_root(canonical_git_root)
}

pub fn lock_workspace_in_state_dir(
    state_dir: impl AsRef<Path>,
    identity: &WorkspaceIdentity,
) -> Result<WorkspaceLock> {
    WorkspaceLockStore::in_state_dir(state_dir).lock_workspace(identity)
}

pub fn lock_path_for_digest(digest64: impl AsRef<str>) -> Result<PathBuf> {
    Ok(WorkspaceLockStore::from_xdg()?.lock_path_for_digest(digest64))
}

pub fn lock_path_in_state_dir(state_dir: impl AsRef<Path>, digest64: impl AsRef<str>) -> PathBuf {
    WorkspaceLockStore::in_state_dir(state_dir).lock_path_for_digest(digest64)
}

fn git_root_digest64(canonical_git_root: &Utf8Path) -> String {
    hex_digest(&sha256_bytes(canonical_git_root.as_str().as_bytes()))
}

fn open_lock_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?)
}

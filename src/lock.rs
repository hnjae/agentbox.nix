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

pub fn lock_workspace(identity: &WorkspaceIdentity) -> Result<WorkspaceLock> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let state_dir = base_dirs
        .state_dir()
        .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    lock_workspace_in_state_dir(state_dir, identity)
}

pub fn lock_git_root(canonical_git_root: &Utf8Path) -> Result<WorkspaceLock> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let state_dir = base_dirs
        .state_dir()
        .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let digest64 = hex_digest(&sha256_bytes(canonical_git_root.as_str().as_bytes()));
    let path = lock_path_in_state_dir(state_dir, &digest64);
    let file = open_lock_file(&path)?;

    Ok(WorkspaceLock {
        path,
        lock: RwLock::new(file),
    })
}

pub fn lock_workspace_in_state_dir(
    state_dir: impl AsRef<Path>,
    identity: &WorkspaceIdentity,
) -> Result<WorkspaceLock> {
    let path = lock_path_in_state_dir(state_dir, &identity.digest64);
    let file = open_lock_file(&path)?;

    Ok(WorkspaceLock {
        path,
        lock: RwLock::new(file),
    })
}

pub fn lock_path_for_digest(digest64: impl AsRef<str>) -> Result<PathBuf> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let state_dir = base_dirs
        .state_dir()
        .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    Ok(lock_path_in_state_dir(state_dir, digest64))
}

pub fn lock_path_in_state_dir(state_dir: impl AsRef<Path>, digest64: impl AsRef<str>) -> PathBuf {
    state_dir
        .as_ref()
        .join(STATE_DIR)
        .join(LOCKS_DIR)
        .join(format!("{}.lock", digest64.as_ref()))
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

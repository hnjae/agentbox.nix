// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use fd_lock::{RwLock, RwLockWriteGuard};

use crate::Error;
use crate::error::Result;
use crate::state::AgentboxStateRoot;
use crate::workspace::{WorkspaceIdentity, git_root_digest64};

const LOCKS_DIR: &str = "locks";
const LOCK_FILE_EXTENSION: &str = ".lock";

pub struct WorkspaceLock {
    path: PathBuf,
    lock: RwLock<File>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceLockStore {
    state_root: AgentboxStateRoot,
}

pub struct WorkspaceLockGuard<'a> {
    guard: RwLockWriteGuard<'a, File>,
}

impl WorkspaceLock {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn guard(&mut self) -> Result<WorkspaceLockGuard<'_>> {
        let guard = self.lock.write()?;
        if !lock_file_still_at_path(&guard, &self.path)? {
            return Err(Error::msg(
                "workspace lock file changed while acquiring it; retry the command",
            ));
        }

        Ok(WorkspaceLockGuard { guard })
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
        Ok(Self {
            state_root: AgentboxStateRoot::from_xdg()?,
        })
    }

    pub fn in_state_dir(state_dir: impl AsRef<Path>) -> Self {
        Self {
            state_root: AgentboxStateRoot::from_state_home(state_dir),
        }
    }

    pub fn lock_workspace(&self, identity: &WorkspaceIdentity) -> Result<WorkspaceLock> {
        self.lock_digest(&identity.digest64)
    }

    pub fn lock_git_root(&self, canonical_git_root: &Utf8Path) -> Result<WorkspaceLock> {
        self.lock_digest(git_root_digest64(canonical_git_root))
    }

    pub fn lock_path_for_digest(&self, digest64: impl AsRef<str>) -> PathBuf {
        self.lock_dir()
            .join(format!("{}{LOCK_FILE_EXTENSION}", digest64.as_ref()))
    }

    pub fn cleanup_lock_files(&self) -> Result<Vec<WorkspaceLockFileStatus>> {
        let entries = match std::fs::read_dir(self.lock_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut statuses = Vec::new();

        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() || !is_workspace_lock_file_name(&entry.file_name()) {
                continue;
            }

            if let Some(status) = self.cleanup_lock_file_status(entry.path())? {
                statuses.push(status);
            }
        }

        statuses.sort_by(|left, right| left.path().cmp(right.path()));
        Ok(statuses)
    }

    pub fn remove_cleanup_lock_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<WorkspaceLockFileRemoval> {
        let Some(mut lock) = open_existing_workspace_lock_file(path.as_ref())? else {
            return Ok(WorkspaceLockFileRemoval::Missing);
        };

        let _guard = match lock.try_write() {
            Ok(guard) => guard,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return Ok(WorkspaceLockFileRemoval::Locked);
            }
            Err(error) => return Err(error.into()),
        };

        match std::fs::remove_file(path.as_ref()) {
            Ok(()) => Ok(WorkspaceLockFileRemoval::Removed),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(WorkspaceLockFileRemoval::Missing)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn lock_digest(&self, digest64: impl AsRef<str>) -> Result<WorkspaceLock> {
        let path = self.lock_path_for_digest(digest64);
        let file = open_lock_file(&path)?;

        Ok(WorkspaceLock {
            path,
            lock: RwLock::new(file),
        })
    }

    fn lock_dir(&self) -> PathBuf {
        self.state_root.join(LOCKS_DIR)
    }

    fn cleanup_lock_file_status(&self, path: PathBuf) -> Result<Option<WorkspaceLockFileStatus>> {
        let Some(mut lock) = open_existing_workspace_lock_file(&path)? else {
            return Ok(None);
        };

        let file = WorkspaceLockFile { path };
        match lock.try_write() {
            Ok(_guard) => Ok(Some(WorkspaceLockFileStatus::Available(file))),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Ok(Some(WorkspaceLockFileStatus::Locked(file)))
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLockFile {
    path: PathBuf,
}

impl WorkspaceLockFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceLockFileStatus {
    Available(WorkspaceLockFile),
    Locked(WorkspaceLockFile),
}

impl WorkspaceLockFileStatus {
    pub fn path(&self) -> &Path {
        match self {
            Self::Available(file) | Self::Locked(file) => file.path(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceLockFileRemoval {
    Removed,
    Missing,
    Locked,
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

pub fn cleanup_lock_files() -> Result<Vec<WorkspaceLockFileStatus>> {
    WorkspaceLockStore::from_xdg()?.cleanup_lock_files()
}

pub fn remove_cleanup_lock_file(path: impl AsRef<Path>) -> Result<WorkspaceLockFileRemoval> {
    WorkspaceLockStore::from_xdg()?.remove_cleanup_lock_file(path)
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

fn open_existing_workspace_lock_file(path: &Path) -> Result<Option<RwLock<File>>> {
    match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => Ok(Some(RwLock::new(file))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_workspace_lock_file_name(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let Some(digest) = name.strip_suffix(LOCK_FILE_EXTENSION) else {
        return false;
    };

    digest.len() == 64 && digest.chars().all(|ch| matches!(ch, '0'..='9' | 'a'..='f'))
}

#[cfg(unix)]
fn lock_file_still_at_path(file: &File, path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let file_metadata = file.metadata()?;
    let path_metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };

    Ok(file_metadata.dev() == path_metadata.dev() && file_metadata.ino() == path_metadata.ino())
}

#[cfg(not(unix))]
fn lock_file_still_at_path(_file: &File, path: &Path) -> io::Result<bool> {
    Ok(path.exists())
}

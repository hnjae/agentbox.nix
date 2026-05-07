// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::Path;

use camino::Utf8Path;

use crate::Result;
use crate::lock::{lock_git_root, lock_workspace};
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_sessions_for_git_root};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};

pub(crate) struct LockedWorkspace<'a> {
    workspace: &'a WorkspaceIdentity,
    podman: Podman,
}

pub(crate) struct LockedGitRoot<'a> {
    git_root: &'a Utf8Path,
    podman: Podman,
}

impl LockedWorkspace<'_> {
    pub(crate) fn workspace(&self) -> &WorkspaceIdentity {
        self.workspace
    }

    pub(crate) fn podman(&self) -> &Podman {
        &self.podman
    }

    pub(crate) fn discover_sessions(&self) -> Result<Vec<SessionRecord>> {
        discover_sessions_for_git_root(&self.podman, self.workspace.canonical_git_root.as_ref())
    }
}

impl LockedGitRoot<'_> {
    pub(crate) fn git_root(&self) -> &Utf8Path {
        self.git_root
    }

    pub(crate) fn podman(&self) -> &Podman {
        &self.podman
    }

    pub(crate) fn discover_sessions(&self) -> Result<Vec<SessionRecord>> {
        discover_sessions_for_git_root(&self.podman, self.git_root)
    }
}

pub(crate) fn with_locked_workspace<T>(
    directory: &Path,
    verbose: bool,
    operation: impl FnOnce(LockedWorkspace<'_>) -> Result<T>,
) -> Result<T> {
    let workspace = resolve_workspace_identity(directory)?;
    let mut workspace_lock = lock_workspace(&workspace)?;
    let _workspace_guard = workspace_lock.guard()?;
    let locked = LockedWorkspace {
        workspace: &workspace,
        podman: Podman::new().with_verbose(verbose),
    };

    operation(locked)
}

pub(crate) fn with_locked_git_root<T>(
    git_root: &Utf8Path,
    operation: impl FnOnce(LockedGitRoot<'_>) -> Result<T>,
) -> Result<T> {
    let mut workspace_lock = lock_git_root(git_root)?;
    let _workspace_guard = workspace_lock.guard()?;
    let locked = LockedGitRoot {
        git_root,
        podman: Podman::new(),
    };

    operation(locked)
}

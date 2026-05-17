// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use camino::Utf8Path;

use crate::Result;
use crate::lock::{lock_git_root, lock_workspace};
use crate::podman::Podman;
use crate::session::{SessionDiscoveryQuery, SessionRecord};
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

    pub(crate) fn discover_managed_sessions(&self) -> Result<Vec<SessionRecord>> {
        SessionDiscoveryQuery::managed_sessions()
            .for_git_root(self.workspace.canonical_git_root.as_ref())
            .discover(&self.podman)
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
        SessionDiscoveryQuery::agentbox_containers()
            .for_git_root(self.git_root)
            .discover(&self.podman)
    }

    pub(crate) fn discover_agentbox_containers(&self) -> Result<Vec<SessionRecord>> {
        SessionDiscoveryQuery::agentbox_containers().discover(&self.podman)
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
    with_locked_git_root_verbose(git_root, false, operation)
}

pub(crate) fn with_locked_git_root_verbose<T>(
    git_root: &Utf8Path,
    verbose: bool,
    operation: impl FnOnce(LockedGitRoot<'_>) -> Result<T>,
) -> Result<T> {
    let mut workspace_lock = lock_git_root(git_root)?;
    let _workspace_guard = workspace_lock.guard()?;
    let locked = LockedGitRoot {
        git_root,
        podman: Podman::new().with_verbose(verbose),
    };

    operation(locked)
}

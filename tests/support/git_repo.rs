#![allow(dead_code)]

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub fn temp_git_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();

    init_git_repo(repo.path());

    repo
}

pub fn init_git_repo(path: &Path) {
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(path)
        .status()
        .expect("failed to run `git init` for test repository");
    assert!(status.success(), "`git init` failed with {status}");
    fs::write(path.join(".gitignore"), "\n").unwrap();
}

pub fn tempdir_outside_git() -> TempDir {
    for parent in non_git_temp_parents() {
        if let Ok(dir) = tempfile::tempdir_in(parent) {
            return dir;
        }
    }

    panic!("failed to create a temporary directory outside a git worktree")
}

fn non_git_temp_parents() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        candidates.push(PathBuf::from(path));
    }

    candidates.extend([
        PathBuf::from("/dev/shm"),
        PathBuf::from("/var/tmp"),
        std::env::temp_dir(),
    ]);

    candidates
        .into_iter()
        .filter(|path| path.is_dir() && !is_inside_git_worktree(path))
        .collect()
}

fn is_inside_git_worktree(path: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

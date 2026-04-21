// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

pub fn temp_git_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();

    run_git(&["init", repo.path().to_str().unwrap()], repo.path());
    fs::write(repo.path().join(".gitignore"), "\n").unwrap();
    run_git(&["add", ".gitignore"], repo.path());

    let output = Command::new("git")
        .current_dir(repo.path())
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git commit failed: {output:?}");

    repo
}

fn run_git(args: &[&str], directory: &Path) {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {:?} failed: {output:?}", args);
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use agentbox::workspace::{
    container_name_from_canonical_root, hash12, resolve_workspace_identity, sha256_bytes,
};
use camino::Utf8PathBuf;
use std::fs;
use tempfile::TempDir;

#[test]
fn resolves_canonical_root_and_target() {
    let repo = temp_git_repo();
    let nested = repo.path().join("nested");
    fs::create_dir(&nested).unwrap();

    let identity = resolve_workspace_identity(&nested).unwrap();

    let nested_canon =
        Utf8PathBuf::from_path_buf(nested.as_path().canonicalize().unwrap()).unwrap();
    let root_canon = Utf8PathBuf::from_path_buf(repo.path().canonicalize().unwrap()).unwrap();

    assert_eq!(identity.absolute_target, nested_canon);
    assert_eq!(identity.canonical_target, nested_canon);
    assert_eq!(identity.canonical_git_root, root_canon);
}

#[test]
fn symlinked_path_keeps_same_identity() {
    let repo = temp_git_repo();
    let alias = repo.path().join("alias-repo");
    #[cfg(unix)]
    std::os::unix::fs::symlink(repo.path(), &alias).unwrap();

    #[cfg(unix)]
    {
        let via_real = resolve_workspace_identity(repo.path()).unwrap();
        let via_link = resolve_workspace_identity(&alias).unwrap();

        assert_eq!(via_real.canonical_git_root, via_link.canonical_git_root);
        assert_eq!(via_real.container_name, via_link.container_name);
    }
}

#[test]
fn rejects_escaped_target_outside_repo_root() {
    let repo = temp_git_repo();
    let outside = tempfile::tempdir_in(repo.path().parent().unwrap()).unwrap();
    let escaped = repo.path().join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &escaped).unwrap();

    #[cfg(unix)]
    assert!(resolve_workspace_identity(&escaped).is_err());
}

#[test]
fn hashes_and_names_match_spec_example() {
    let digest = sha256_bytes(b"/aaa/bbb");
    assert_eq!(agentbox::workspace::hex_digest(&digest).len(), 64);
    assert_eq!(hash12(b"/aaa/bbb"), "2f83c6a14d91");
    assert_eq!(
        container_name_from_canonical_root("/aaa/bbb"),
        "agentbox-_aaa_bbb-2f83c6a14d91"
    );
}

#[test]
fn overlong_paths_preserve_rightmost_suffix() {
    let root = format!("/{}{}", "a".repeat(70), "-z".repeat(10));
    let name = container_name_from_canonical_root(&root);

    assert!(name.len() <= 63);
    assert!(name.starts_with("agentbox-"));
    assert!(name.ends_with(&format!("-{}", hash12(root.as_bytes()))));
    assert!(name.contains("-z"));
}

fn temp_git_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .arg("init")
        .arg(repo.path())
        .output()
        .unwrap();
    fs::write(repo.path().join(".gitignore"), "\n").unwrap();
    std::process::Command::new("git")
        .current_dir(repo.path())
        .arg("add")
        .arg(".gitignore")
        .output()
        .unwrap();
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    repo
}

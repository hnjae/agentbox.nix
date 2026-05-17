// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use agentbox::workspace::{
    container_name_from_canonical_root, git_root_digest64, git_root_hash12,
    resolve_workspace_identity, resolve_workspace_identity_with_git,
};
use agentbox::{git::Git, process::ProcessRunner};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs;

#[path = "support/mod.rs"]
mod support;

#[test]
fn resolves_canonical_root_and_target() {
    let repo = support::temp_git_repo();
    let nested = repo.path().join("nested");
    fs::create_dir(&nested).unwrap();

    let identity = resolve_workspace_identity(&nested).unwrap();

    let nested_canon =
        Utf8PathBuf::from_path_buf(nested.as_path().canonicalize().unwrap()).unwrap();
    let root_canon = Utf8PathBuf::from_path_buf(repo.path().canonicalize().unwrap()).unwrap();

    assert_eq!(identity.absolute_target, nested_canon);
    assert_eq!(identity.canonical_target, nested_canon);
    assert_eq!(identity.canonical_git_root, root_canon);
    assert!(identity.digest64.starts_with(&identity.hash12));
}

#[test]
fn symlinked_path_keeps_same_identity() {
    let repo = support::temp_git_repo();
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
    let repo = support::temp_git_repo();
    let outside = tempfile::tempdir_in(repo.path().parent().unwrap()).unwrap();
    let escaped = repo.path().join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &escaped).unwrap();

    #[cfg(unix)]
    assert!(resolve_workspace_identity(&escaped).is_err());
}

#[test]
fn does_not_accept_git_marker_when_git_rejects_the_directory() {
    let fake_bins = support::FakeBinDir::new();
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let repo_path = repo.path().to_str().unwrap();
    fake_bins.install_exact_failure(
        "git",
        &["-C", repo_path, "rev-parse", "--show-toplevel"],
        "fatal: not a git repository (or any of the parent directories): .git",
        128,
    );

    let git = Git::with_runner(ProcessRunner::new().with_path_prepend(fake_bins.path()));
    let error = resolve_workspace_identity_with_git(repo.path(), &git).unwrap_err();

    assert!(error.to_string().contains("not inside a git repository"));
}

#[test]
fn hashes_and_names_match_spec_example() {
    assert_eq!(
        git_root_digest64(Utf8Path::new("/aaa/bbb")),
        "9ae5447864f74f9137f1ebb8bfe3ff1122f09548caf8b31fde5315f21222dbff"
    );
    assert_eq!(git_root_hash12(Utf8Path::new("/aaa/bbb")), "9ae5447864f7");
    assert_eq!(
        container_name_from_canonical_root("/aaa/bbb"),
        "agentbox-_aaa_bbb-9ae5447864f7"
    );
}

#[test]
fn overlong_paths_preserve_rightmost_suffix() {
    let root = format!("/{}{}", "a".repeat(70), "-z".repeat(10));
    let name = container_name_from_canonical_root(&root);

    assert!(name.len() <= 63);
    assert!(name.starts_with("agentbox-"));
    assert!(name.ends_with(&format!("-{}", git_root_hash12(Utf8Path::new(&root)))));
    assert!(name.contains("-z"));
}

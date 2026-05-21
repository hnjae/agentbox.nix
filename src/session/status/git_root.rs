// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::git::Git;
use crate::paths::canonicalize_utf8_path;

pub(in crate::session) trait GitRootProbe: std::fmt::Debug {
    fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf>;
    fn is_directory(&self, git_root: &Utf8Path) -> bool;
    fn has_git_marker(&self, git_root: &Utf8Path) -> bool;
    fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf>;
}

#[derive(Debug)]
pub(in crate::session) struct HostGitRootProbe {
    git: Git,
}

impl HostGitRootProbe {
    pub(in crate::session) fn new() -> Self {
        Self { git: Git::new() }
    }
}

impl GitRootProbe for HostGitRootProbe {
    fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
        canonicalize_utf8_path(git_root).ok()
    }

    fn is_directory(&self, git_root: &Utf8Path) -> bool {
        git_root.as_std_path().is_dir()
    }

    fn has_git_marker(&self, git_root: &Utf8Path) -> bool {
        let git_marker = git_root.join(".git");
        git_marker.is_dir() || git_marker.is_file()
    }

    fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
        self.git.rev_parse_show_toplevel(git_root).ok()
    }
}

pub(super) fn git_root_is_orphaned(git_root: &Utf8Path, probe: &dyn GitRootProbe) -> bool {
    let canonical_git_root = match probe.canonicalize(git_root) {
        Some(canonical_git_root) if canonical_git_root == git_root => canonical_git_root,
        _ => return true,
    };

    if !probe.is_directory(&canonical_git_root) {
        return true;
    }

    if probe.has_git_marker(&canonical_git_root) {
        return false;
    }

    match probe.rev_parse_show_toplevel(&canonical_git_root) {
        Some(resolved_git_root) => match probe.canonicalize(&resolved_git_root) {
            Some(resolved_git_root) => resolved_git_root != canonical_git_root,
            None => true,
        },
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn git_root_with_git_marker_is_not_orphaned_without_rev_parse() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_directory("/repo")
            .with_git_marker("/repo");

        assert!(!git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
        assert_eq!(probe.rev_parse_calls.get(), 0);
    }

    #[test]
    fn git_root_without_marker_is_not_orphaned_when_rev_parse_matches() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_directory("/repo")
            .with_rev_parse("/repo", "/repo");

        assert!(!git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
        assert_eq!(probe.rev_parse_calls.get(), 1);
    }

    #[test]
    fn git_root_is_orphaned_when_canonical_root_changes() {
        let probe = FakeGitRootProbe::new().with_canonical("/workspace/link", "/workspace/real");

        assert!(git_root_is_orphaned(
            Utf8Path::new("/workspace/link"),
            &probe
        ));
    }

    #[test]
    fn git_root_is_orphaned_when_rev_parse_resolves_elsewhere() {
        let probe = FakeGitRootProbe::new()
            .with_canonical("/repo", "/repo")
            .with_canonical("/other", "/other")
            .with_directory("/repo")
            .with_rev_parse("/repo", "/other");

        assert!(git_root_is_orphaned(Utf8Path::new("/repo"), &probe));
    }

    #[derive(Debug)]
    struct FakeGitRootProbe {
        canonical_paths: BTreeMap<Utf8PathBuf, Utf8PathBuf>,
        directories: BTreeSet<Utf8PathBuf>,
        git_markers: BTreeSet<Utf8PathBuf>,
        rev_parse_roots: BTreeMap<Utf8PathBuf, Utf8PathBuf>,
        rev_parse_calls: Cell<usize>,
    }

    impl FakeGitRootProbe {
        fn new() -> Self {
            Self {
                canonical_paths: BTreeMap::new(),
                directories: BTreeSet::new(),
                git_markers: BTreeSet::new(),
                rev_parse_roots: BTreeMap::new(),
                rev_parse_calls: Cell::new(0),
            }
        }

        fn with_canonical(mut self, input: &str, output: &str) -> Self {
            self.canonical_paths
                .insert(Utf8PathBuf::from(input), Utf8PathBuf::from(output));
            self
        }

        fn with_directory(mut self, path: &str) -> Self {
            self.directories.insert(Utf8PathBuf::from(path));
            self
        }

        fn with_git_marker(mut self, path: &str) -> Self {
            self.git_markers.insert(Utf8PathBuf::from(path));
            self
        }

        fn with_rev_parse(mut self, path: &str, root: &str) -> Self {
            self.rev_parse_roots
                .insert(Utf8PathBuf::from(path), Utf8PathBuf::from(root));
            self
        }
    }

    impl GitRootProbe for FakeGitRootProbe {
        fn canonicalize(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
            self.canonical_paths.get(git_root).cloned()
        }

        fn is_directory(&self, git_root: &Utf8Path) -> bool {
            self.directories.contains(git_root)
        }

        fn has_git_marker(&self, git_root: &Utf8Path) -> bool {
            self.git_markers.contains(git_root)
        }

        fn rev_parse_show_toplevel(&self, git_root: &Utf8Path) -> Option<Utf8PathBuf> {
            self.rev_parse_calls
                .set(self.rev_parse_calls.get().saturating_add(1));
            self.rev_parse_roots.get(git_root).cloned()
        }
    }
}

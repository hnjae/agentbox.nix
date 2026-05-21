// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

pub(super) fn nearest_file(
    target_directory: &Utf8Path,
    git_root: &Utf8Path,
    filename: &str,
) -> Option<Utf8PathBuf> {
    ancestor_directories(target_directory, git_root)
        .into_iter()
        .map(|directory| directory.join(filename))
        .find(|candidate| candidate.is_file())
}

pub(super) fn parent_directory(path: &Utf8Path, filename: &str) -> Utf8PathBuf {
    path.parent()
        .unwrap_or_else(|| panic!("{filename} candidates always have a parent"))
        .to_path_buf()
}

fn ancestor_directories(target_directory: &Utf8Path, git_root: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut directories = Vec::new();
    for candidate in target_directory.ancestors() {
        directories.push(candidate.to_path_buf());
        if candidate == git_root {
            break;
        }
    }
    directories
}

pub(super) fn path_flake_ref(path: &Utf8Path) -> String {
    format!("path:{path}")
}

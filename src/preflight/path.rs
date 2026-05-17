// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::paths::path_is_or_descendant;

pub(super) fn symlink_or_path_exists(path: &Utf8Path) -> bool {
    fs::symlink_metadata(path.as_std_path()).is_ok()
}

pub(super) fn path_reaches_mount_root(path: &Utf8Path, mount_root: &Utf8Path) -> bool {
    let mount_root = normalize_path(mount_root);

    if normalized_path_reaches_mount_root(path, &mount_root)
        || normalized_path_reaches_mount_root(&resolve_path(path), &mount_root)
    {
        return true;
    }

    path.ancestors()
        .any(|ancestor| symlink_expansion_reaches_mount_root(path, ancestor, &mount_root))
}

fn normalized_path_reaches_mount_root(path: &Utf8Path, normalized_mount_root: &Utf8Path) -> bool {
    path_is_or_descendant(&normalize_path(path), normalized_mount_root)
}

fn symlink_expansion_reaches_mount_root(
    path: &Utf8Path,
    ancestor: &Utf8Path,
    normalized_mount_root: &Utf8Path,
) -> bool {
    let Some(target) = read_symlink_target(ancestor) else {
        return false;
    };
    let target_path = resolve_symlink_target(ancestor, &target);
    let expanded_path = expanded_symlink_path(path, ancestor, &target_path);

    normalized_path_reaches_mount_root(&target_path, normalized_mount_root)
        || normalized_path_reaches_mount_root(&expanded_path, normalized_mount_root)
}

fn expanded_symlink_path(
    path: &Utf8Path,
    ancestor: &Utf8Path,
    target_path: &Utf8Path,
) -> Utf8PathBuf {
    match path.strip_prefix(ancestor) {
        Ok(suffix) if !suffix.as_str().is_empty() => target_path.join(suffix),
        _ => target_path.to_owned(),
    }
}

fn read_symlink_target(path: &Utf8Path) -> Option<Utf8PathBuf> {
    fs::read_link(path.as_std_path())
        .ok()
        .and_then(|target| Utf8PathBuf::from_path_buf(target).ok())
}

fn resolve_symlink_target(link_path: &Utf8Path, target: &Utf8Path) -> Utf8PathBuf {
    if target.is_absolute() {
        return target.to_owned();
    }

    link_path
        .parent()
        .map(|parent| parent.join(target))
        .unwrap_or_else(|| target.to_owned())
}

fn normalize_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut normalized = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            camino::Utf8Component::Prefix(prefix) => normalized.push(prefix.as_str()),
            camino::Utf8Component::RootDir => normalized.push("/"),
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push("..");
                }
            }
            camino::Utf8Component::Normal(part) => normalized.push(part),
        }
    }

    if normalized.as_str().is_empty() {
        Utf8PathBuf::from(".")
    } else {
        normalized
    }
}

pub(super) fn resolve_path(path: &Utf8Path) -> Utf8PathBuf {
    fs::canonicalize(path.as_std_path())
        .ok()
        .and_then(|value| Utf8PathBuf::from_path_buf(value).ok())
        .unwrap_or_else(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[cfg(unix)]
    #[test]
    fn path_reaches_mount_root_when_symlink_points_through_mount_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc_nix = root.join("etc/nix");
        let static_nix = root.join("etc/static/nix");
        let store = root.join("nix/store");
        fs::create_dir_all(&etc_nix).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("nix.conf"), "sandbox = false\n").unwrap();

        symlink(
            store.join("nix.conf").as_std_path(),
            static_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();
        symlink(
            static_nix.join("nix.custom.conf").as_std_path(),
            etc_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();

        assert!(path_reaches_mount_root(
            &etc_nix.join("nix.custom.conf"),
            &static_nix,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn path_reaches_mount_root_when_parent_symlink_points_to_mount_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc = root.join("etc");
        let static_nix = root.join("etc/static/nix");
        fs::create_dir_all(&etc).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::write(static_nix.join("nix.custom.conf"), "sandbox = false\n").unwrap();
        symlink(static_nix.as_std_path(), etc.join("nix").as_std_path()).unwrap();

        assert!(path_reaches_mount_root(
            &etc.join("nix/nix.custom.conf"),
            &static_nix,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn path_does_not_reach_mount_root_when_symlink_points_directly_elsewhere() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc_nix = root.join("etc/nix");
        let static_nix = root.join("etc/static/nix");
        let store = root.join("nix/store");
        fs::create_dir_all(&etc_nix).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("nix.conf"), "sandbox = false\n").unwrap();
        symlink(
            store.join("nix.conf").as_std_path(),
            etc_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();

        assert!(!path_reaches_mount_root(
            &etc_nix.join("nix.custom.conf"),
            &static_nix,
        ));
    }
}

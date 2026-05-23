// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Result;
use crate::runtime::RuntimeMount;

pub(super) const CONTAINER_GIT_EXCLUDES_FILE: &str = "/run/agentbox/git-ignore";
pub(super) const GIT_EXCLUDES_FILE_KEY: &str = "core.excludesFile";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct GitExcludesFilePassthrough {
    pub(super) mount: Option<RuntimeMount>,
    pub(super) git_config_entries: Vec<(String, String)>,
}

pub(super) fn detect_with(
    git_root: &Utf8Path,
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    git_config_path: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> GitExcludesFilePassthrough {
    let Some(source) = source_path(git_root, environment, git_config_path, warning) else {
        return GitExcludesFilePassthrough::default();
    };

    if !readable_regular_file(&source, warning) {
        return GitExcludesFilePassthrough::default();
    }

    GitExcludesFilePassthrough {
        mount: Some(RuntimeMount::read_only_bind(
            source.to_string(),
            CONTAINER_GIT_EXCLUDES_FILE,
        )),
        git_config_entries: vec![(
            GIT_EXCLUDES_FILE_KEY.to_string(),
            CONTAINER_GIT_EXCLUDES_FILE.to_string(),
        )],
    }
}

fn source_path(
    git_root: &Utf8Path,
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    git_config_path: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Option<Utf8PathBuf> {
    match git_config_path(git_root, GIT_EXCLUDES_FILE_KEY) {
        Ok(Some(value)) if value.is_empty() => return None,
        Ok(Some(value)) => return Some(resolve_configured_path(git_root, value)),
        Ok(None) => {}
        Err(error) => {
            warning(format!(
                "failed to read host Git config `{GIT_EXCLUDES_FILE_KEY}` for Git excludes file passthrough: {error}"
            ));
            return None;
        }
    }

    default_excludes_file(environment, warning)
}

fn resolve_configured_path(git_root: &Utf8Path, value: String) -> Utf8PathBuf {
    let source = Utf8PathBuf::from(value);
    if source.is_absolute() {
        return source;
    }

    git_root.join(source)
}

fn default_excludes_file(
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    warning: &mut impl FnMut(String),
) -> Option<Utf8PathBuf> {
    if let Some(value) = environment("XDG_CONFIG_HOME") {
        if !value.is_empty() {
            return environment_value_path("XDG_CONFIG_HOME", value, warning)
                .map(|xdg_config_home| xdg_config_home.join("git").join("ignore"));
        }
    }

    let home = environment("HOME")?;
    if home.is_empty() {
        return None;
    }

    environment_value_path("HOME", home, warning)
        .map(|home| home.join(".config").join("git").join("ignore"))
}

fn environment_value_path(
    name: &'static str,
    value: OsString,
    warning: &mut impl FnMut(String),
) -> Option<Utf8PathBuf> {
    match Utf8PathBuf::from_path_buf(PathBuf::from(value)) {
        Ok(path) => Some(path),
        Err(path) => {
            warning(format!(
                "`{name}` must be a UTF-8 path for Git excludes file passthrough: {}",
                path.display()
            ));
            None
        }
    }
}

fn readable_regular_file(path: &Utf8Path, warning: &mut impl FnMut(String)) -> bool {
    let metadata = match fs::metadata(path.as_std_path()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            warning(format!(
                "failed to inspect host Git excludes file `{path}`: {error}; Git excludes file passthrough disabled"
            ));
            return false;
        }
    };

    if !metadata.is_file() {
        warning(format!(
            "host Git excludes file is not a regular file: {path}; Git excludes file passthrough disabled"
        ));
        return false;
    }

    if let Err(error) = fs::File::open(path.as_std_path()) {
        warning(format!(
            "host Git excludes file is not readable: {path}: {error}; Git excludes file passthrough disabled"
        ));
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_absolute_excludes_file_adds_read_only_mount_and_git_config() {
        let sandbox = tempfile::tempdir().unwrap();
        let excludes = Utf8PathBuf::from_path_buf(sandbox.path().join("ignore")).unwrap();
        fs::write(&excludes, "target\n").unwrap();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |_| None,
            &mut |_git_root, key| {
                assert_eq!(key, GIT_EXCLUDES_FILE_KEY);
                Ok(Some(excludes.to_string()))
            },
            &mut panic_warning,
        );

        assert_eq!(
            passthrough.mount,
            Some(RuntimeMount::read_only_bind(
                excludes.to_string(),
                CONTAINER_GIT_EXCLUDES_FILE
            ))
        );
        assert_eq!(
            passthrough.git_config_entries,
            [(
                GIT_EXCLUDES_FILE_KEY.to_string(),
                CONTAINER_GIT_EXCLUDES_FILE.to_string()
            )]
        );
    }

    #[test]
    fn configured_relative_excludes_file_is_resolved_from_git_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().join("repo")).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("ignore"), "target\n").unwrap();

        let passthrough = detect_with(
            &root,
            &mut |_| None,
            &mut |_git_root, _key| Ok(Some("ignore".to_string())),
            &mut panic_warning,
        );

        assert_eq!(
            passthrough.mount,
            Some(RuntimeMount::read_only_bind(
                root.join("ignore").to_string(),
                CONTAINER_GIT_EXCLUDES_FILE
            ))
        );
    }

    #[test]
    fn default_excludes_file_prefers_xdg_config_home() {
        let sandbox = tempfile::tempdir().unwrap();
        let xdg = Utf8PathBuf::from_path_buf(sandbox.path().join("xdg")).unwrap();
        let home = Utf8PathBuf::from_path_buf(sandbox.path().join("home")).unwrap();
        fs::create_dir_all(xdg.join("git")).unwrap();
        fs::create_dir_all(home.join(".config/git")).unwrap();
        let xdg_ignore = xdg.join("git/ignore");
        let home_ignore = home.join(".config/git/ignore");
        fs::write(&xdg_ignore, "xdg\n").unwrap();
        fs::write(&home_ignore, "home\n").unwrap();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |name| match name {
                "XDG_CONFIG_HOME" => Some(xdg.as_os_str().to_os_string()),
                "HOME" => Some(home.as_os_str().to_os_string()),
                _ => None,
            },
            &mut |_git_root, _key| Ok(None),
            &mut panic_warning,
        );

        assert_eq!(
            passthrough.mount,
            Some(RuntimeMount::read_only_bind(
                xdg_ignore.to_string(),
                CONTAINER_GIT_EXCLUDES_FILE
            ))
        );
    }

    #[test]
    fn missing_excludes_file_is_skipped_without_warning() {
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |_| None,
            &mut |_git_root, _key| Ok(Some("/missing/git-ignore".to_string())),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, GitExcludesFilePassthrough::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn empty_configured_excludes_file_is_skipped_without_default_fallback() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = Utf8PathBuf::from_path_buf(sandbox.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".config/git")).unwrap();
        fs::write(home.join(".config/git/ignore"), "home\n").unwrap();
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |name| match name {
                "HOME" => Some(home.as_os_str().to_os_string()),
                _ => None,
            },
            &mut |_git_root, _key| Ok(Some(String::new())),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, GitExcludesFilePassthrough::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn directory_excludes_file_warns_and_is_skipped() {
        let sandbox = tempfile::tempdir().unwrap();
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |_| None,
            &mut |_git_root, _key| Ok(Some(sandbox.path().display().to_string())),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, GitExcludesFilePassthrough::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not a regular file"));
    }

    #[test]
    fn git_config_failure_warns_and_is_skipped() {
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            &mut |_| None,
            &mut |_git_root, _key| Err(crate::Error::msg("git exploded")),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, GitExcludesFilePassthrough::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("failed to read host Git config `core.excludesFile`"));
    }

    fn panic_warning(warning: String) {
        panic!("unexpected warning: {warning}");
    }
}

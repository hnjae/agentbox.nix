// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;

use crate::paths::path_buf_to_utf8;
use crate::workspace::resolve_workspace_identity;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StopTargetInput {
    Cli(PathBuf),
    StableId(String),
}

impl StopTargetInput {
    pub(super) fn display(&self) -> String {
        match self {
            Self::Cli(path) => path.display().to_string(),
            Self::StableId(id) => id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StopTarget {
    ResolvedGitRoot(Utf8PathBuf),
    ExactStoredGitRootPath(Utf8PathBuf),
    StableId(String),
}

pub(super) fn resolve_stop_target(target: &StopTargetInput) -> Result<StopTarget> {
    match target {
        StopTargetInput::Cli(path) => resolve_cli_stop_target(path),
        StopTargetInput::StableId(prefix) => Ok(StopTarget::StableId(prefix.clone())),
    }
}

fn resolve_cli_stop_target(target: &Path) -> Result<StopTarget> {
    if target.exists() {
        return resolve_workspace_identity(target)
            .map(|workspace| StopTarget::ResolvedGitRoot(workspace.canonical_git_root));
    }

    classify_missing_cli_stop_target(target)
}

fn classify_missing_cli_stop_target(target: &Path) -> Result<StopTarget> {
    if target.is_absolute() {
        let git_root = path_buf_to_utf8(target.to_path_buf())?;
        return Ok(StopTarget::ExactStoredGitRootPath(git_root));
    }

    let prefix = target.to_str().ok_or_else(|| {
        Error::msg(format!(
            "non-utf8 target `{}` cannot be used as a stable id prefix",
            target.display()
        ))
    })?;

    Ok(StopTarget::StableId(prefix.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_absolute_cli_target_is_exact_stored_git_root_path() {
        let sandbox = tempfile::tempdir().unwrap();
        let target = sandbox.path().join("missing-root");

        let resolved = classify_missing_cli_stop_target(&target).unwrap();

        assert_eq!(
            resolved,
            StopTarget::ExactStoredGitRootPath(Utf8PathBuf::from_path_buf(target).unwrap())
        );
    }

    #[test]
    fn missing_relative_cli_target_is_stable_id_prefix() {
        let target = classify_missing_cli_stop_target(Path::new("abc123")).unwrap();

        assert_eq!(target, StopTarget::StableId("abc123".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn missing_non_utf8_relative_cli_target_is_rejected() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let target = Path::new(OsStr::from_bytes(b"\xff"));
        let error = classify_missing_cli_stop_target(target).unwrap_err();

        assert!(error.to_string().contains("non-utf8 target"));
    }
}

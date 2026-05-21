// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::{Utf8Path, Utf8PathBuf};

use crate::process::ProcessRunner;
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Git {
    runner: ProcessRunner,
}

impl Git {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn rev_parse_show_toplevel(&self, directory: &Utf8Path) -> Result<Utf8PathBuf> {
        self.resolve_toplevel(directory)
            .map_err(|error| error.into_error(directory))
    }

    pub(crate) fn resolve_toplevel(
        &self,
        directory: &Utf8Path,
    ) -> std::result::Result<Utf8PathBuf, GitRootError> {
        let output = self
            .runner
            .capture_status("git", |command| {
                command
                    .arg("-C")
                    .arg(directory.as_str())
                    .args(["rev-parse", "--show-toplevel"]);
            })
            .map_err(GitRootError::Failed)?;

        if !output.status.success() {
            let detail = output.stderr.trim();
            if detail.contains("not a git repository") {
                return Err(GitRootError::NotRepository);
            }

            let detail = output.stderr_or_status_detail();
            return Err(GitRootError::Failed(Error::msg(format!(
                "failed to resolve git root for `{directory}` via `git -C {directory} rev-parse --show-toplevel`: {detail}. Choose a directory inside a readable git worktree."
            ))));
        }

        let root = output.stdout.trim();
        if root.is_empty() {
            return Err(GitRootError::Failed(Error::msg(
                "`git rev-parse --show-toplevel` returned an empty path",
            )));
        }

        Ok(Utf8PathBuf::from(root.to_owned()))
    }

    pub(crate) fn config_get(&self, git_root: &Utf8Path, key: &str) -> Result<Option<String>> {
        let output = self.runner.capture_status("git", |command| {
            command
                .arg("-C")
                .arg(git_root.as_str())
                .args(["config", "--get", key]);
        })?;

        if output.status.success() {
            return Ok(Some(trim_config_output(&output.stdout)));
        }

        if output.status.code() == Some(1) {
            return Ok(None);
        }

        let detail = output.stderr_or_status_detail();
        Err(Error::msg(format!(
            "failed to read git config `{key}` for `{git_root}`: {detail}"
        )))
    }

    pub(crate) fn remote_urls(&self, git_root: &Utf8Path) -> Result<Vec<String>> {
        let output = self.runner.capture_status("git", |command| {
            command
                .arg("-C")
                .arg(git_root.as_str())
                .args(["remote", "-v"]);
        })?;

        if output.status.success() {
            return Ok(parse_remote_urls(&output.stdout));
        }

        let detail = output.stderr_or_status_detail();
        Err(Error::msg(format!(
            "failed to read git remotes for `{git_root}`: {detail}"
        )))
    }
}

fn trim_config_output(output: &str) -> String {
    output.trim_end_matches(['\r', '\n']).to_string()
}

fn parse_remote_urls(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug)]
pub(crate) enum GitRootError {
    NotRepository,
    Failed(Error),
}

impl GitRootError {
    pub(crate) fn into_error(self, directory: &Utf8Path) -> Error {
        match self {
            Self::NotRepository => Error::non_git_target(directory),
            Self::Failed(error) => error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_urls_from_verbose_git_output() {
        assert_eq!(
            parse_remote_urls(
                "origin\tgit@github.com:owner/repo.git (fetch)\norigin\tgit@github.com:owner/repo.git (push)\nupstream https://example.test/repo.git (fetch)\n"
            ),
            [
                "git@github.com:owner/repo.git",
                "git@github.com:owner/repo.git",
                "https://example.test/repo.git"
            ]
        );
    }
}

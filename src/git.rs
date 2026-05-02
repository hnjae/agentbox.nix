// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::ErrorKind;

use camino::{Utf8Path, Utf8PathBuf};

use crate::process::{ProcessRunner, format_status};
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
        let mut command = self.runner.command("git").map_err(GitRootError::Failed)?;
        command
            .arg("-C")
            .arg(directory.as_str())
            .args(["rev-parse", "--show-toplevel"]);

        let output = command.output().map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                GitRootError::GitNotFound
            } else {
                GitRootError::Failed(Error::msg(format!(
                    "failed to run `git -C {directory} rev-parse --show-toplevel`: {error}"
                )))
            }
        })?;

        let stderr =
            String::from_utf8(output.stderr).map_err(|error| GitRootError::Failed(error.into()))?;
        if !output.status.success() {
            let detail = stderr.trim();
            if detail.contains("not a git repository") {
                return Err(GitRootError::NotRepository);
            }

            let detail = if detail.is_empty() {
                format_status(output.status)
            } else {
                detail.to_string()
            };
            return Err(GitRootError::Failed(Error::msg(format!(
                "failed to resolve git root for `{directory}` via `git -C {directory} rev-parse --show-toplevel`: {detail}. Choose a directory inside a readable git worktree."
            ))));
        }

        let stdout =
            String::from_utf8(output.stdout).map_err(|error| GitRootError::Failed(error.into()))?;
        let root = stdout.trim();
        if root.is_empty() {
            return Err(GitRootError::Failed(Error::msg(
                "`git rev-parse --show-toplevel` returned an empty path",
            )));
        }

        Ok(Utf8PathBuf::from(root.to_owned()))
    }
}

#[derive(Debug)]
pub(crate) enum GitRootError {
    GitNotFound,
    NotRepository,
    Failed(Error),
}

impl GitRootError {
    fn into_error(self, directory: &Utf8Path) -> Error {
        match self {
            Self::GitNotFound => {
                Error::msg("`git` was not found on PATH; install `git` or add it to PATH")
            }
            Self::NotRepository => Error::non_git_target(directory),
            Self::Failed(error) => error,
        }
    }
}

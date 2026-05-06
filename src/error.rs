// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Cli(#[from] clap::Error),
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub fn non_git_target(target: &Utf8Path) -> Self {
        Self::msg(format!(
            "`{target}` is not inside a git repository; choose a directory within a git worktree or initialize one with `git init`"
        ))
    }

    pub fn escaped_git_target(target: &Utf8Path, git_root: &Utf8Path) -> Self {
        Self::msg(format!(
            "requested directory `{target}` resolves outside the git root `{git_root}`; choose a path within `{git_root}`"
        ))
    }

    pub fn managed_session_requires_action(
        git_root: &Utf8Path,
        container_name: &str,
        detail: &str,
        next_step: &str,
    ) -> Self {
        Self::msg(format!(
            "managed session `{container_name}` for `{git_root}` {detail}; {next_step}"
        ))
    }

    pub fn duplicate_managed_sessions(git_root: &Utf8Path) -> Self {
        Self::msg(format!(
            "duplicate managed sessions exist for `{git_root}`; remove extras before retrying"
        ))
    }

    pub fn orphaned_managed_session(git_root: &Utf8Path, container_name: &str) -> Self {
        Self::msg(format!(
            "managed session `{container_name}` for `{git_root}` is orphaned after the repository moved; remove or recreate it before retrying"
        ))
    }

    pub fn failed_managed_session(git_root: &Utf8Path, container_name: &str) -> Self {
        Self::msg(format!(
            "managed session `{container_name}` for `{git_root}` is in a failed state; clean up or recreate it before retrying"
        ))
    }

    pub fn runtime_command_failed(
        git_root: &Utf8Path,
        container_name: &str,
        action: &str,
        detail: &str,
    ) -> Self {
        Self::msg(format!(
            "failed to {action} for managed session `{container_name}` in `{git_root}`: {detail}. Verify the runtime image still provides `/entrypoint` and the expected runtime tools, then retry or recreate the session."
        ))
    }
}

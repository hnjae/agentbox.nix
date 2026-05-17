// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::metadata::AgentboxContainerKind;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Cli(#[from] clap::Error),
    #[error("process exited with code {0}")]
    ExitCode(u8),
    #[error("{message}")]
    ExitCodeWithMessage { code: u8, message: String },
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
        Self::agentbox_container_requires_action(
            AgentboxContainerKind::Managed,
            git_root,
            container_name,
            detail,
            next_step,
        )
    }

    pub fn agentbox_container_requires_action(
        container_kind: AgentboxContainerKind,
        git_root: &Utf8Path,
        container_name: &str,
        detail: &str,
        next_step: &str,
    ) -> Self {
        let resource = match container_kind {
            AgentboxContainerKind::Managed => "managed session",
            AgentboxContainerKind::Run => "transient run container",
        };
        Self::msg(format!(
            "{resource} `{container_name}` for `{git_root}` {detail}; {next_step}"
        ))
    }

    pub fn duplicate_managed_sessions(git_root: &Utf8Path) -> Self {
        Self::msg(format!(
            "duplicate managed sessions exist for `{git_root}`; remove extras before retrying"
        ))
    }

    pub fn duplicate_agentbox_containers(git_root: &Utf8Path) -> Self {
        Self::msg(format!(
            "duplicate agentbox containers exist for `{git_root}`; remove extras before retrying"
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

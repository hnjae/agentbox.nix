// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub(crate) struct ProcessStatusOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl ProcessStatusOutput {
    pub(crate) fn output_detail(&self) -> String {
        output_detail(&self.stdout, &self.stderr)
    }

    pub(crate) fn stderr_or_status_detail(&self) -> String {
        let detail = self.stderr.trim();
        if detail.is_empty() {
            format_status(self.status)
        } else {
            detail.to_string()
        }
    }

    pub(crate) fn status_with_output_detail(&self) -> String {
        format!("{}: {}", format_status(self.status), self.output_detail())
    }
}

#[derive(Debug)]
pub(crate) enum ProcessCaptureError {
    Setup(Error),
    Spawn {
        context: CommandContext,
        source: std::io::Error,
    },
    Utf8(std::string::FromUtf8Error),
}

impl ProcessCaptureError {
    pub(crate) fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Spawn {
                source,
                ..
            } if source.kind() == ErrorKind::NotFound
        )
    }

    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Setup(error) => error,
            Self::Spawn { context, source } => context.spawn_error(source),
            Self::Utf8(error) => error.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessRunner {
    path_prepend: Vec<PathBuf>,
}

impl ProcessRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path_prepend(mut self, path: impl Into<PathBuf>) -> Self {
        self.path_prepend.push(path.into());
        self
    }

    fn command(&self, program: &str) -> Result<Command> {
        let mut command = Command::new(program);

        if !self.path_prepend.is_empty() {
            let mut paths = self.path_prepend.clone();
            if let Some(current_path) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&current_path));
            }

            let joined = std::env::join_paths(paths).map_err(|error| {
                Error::msg(format!(
                    "failed to construct PATH override for `{program}`: {error}"
                ))
            })?;
            command.env("PATH", joined);
        }

        Ok(command)
    }

    pub(crate) fn configured_command(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessCommand> {
        let mut command = self.command(program)?;
        configure(&mut command);
        Ok(ProcessCommand { command })
    }

    pub fn capture(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessOutput> {
        self.configured_command(program, configure)?.capture()
    }

    pub(crate) fn capture_status(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessStatusOutput> {
        self.try_capture_status(program, configure)
            .map_err(ProcessCaptureError::into_error)
    }

    pub(crate) fn try_capture_status(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> std::result::Result<ProcessStatusOutput, ProcessCaptureError> {
        let command = self
            .configured_command(program, configure)
            .map_err(ProcessCaptureError::Setup)?;
        command.try_capture_status()
    }
}

#[derive(Debug)]
pub(crate) struct ProcessCommand {
    command: Command,
}

impl ProcessCommand {
    pub(crate) fn description(&self) -> String {
        describe_command(&self.command)
    }

    pub(crate) fn capture(mut self) -> Result<ProcessOutput> {
        run_command(&mut self.command)
    }

    pub(crate) fn try_capture_status(
        mut self,
    ) -> std::result::Result<ProcessStatusOutput, ProcessCaptureError> {
        try_run_command_capture_status(&mut self.command)
    }

    pub(crate) fn status(mut self) -> Result<ExitStatus> {
        run_command_status(&mut self.command)
    }
}

fn run_command(command: &mut Command) -> Result<ProcessOutput> {
    let output = run_command_capture_status(command)?;
    let ProcessStatusOutput {
        status,
        stdout,
        stderr,
    } = output;

    if !status.success() {
        let context = CommandContext::from_command(command);
        return Err(context.exit_error(status, &stdout, &stderr));
    }

    Ok(ProcessOutput { stdout, stderr })
}

fn run_command_capture_status(command: &mut Command) -> Result<ProcessStatusOutput> {
    try_run_command_capture_status(command).map_err(ProcessCaptureError::into_error)
}

fn try_run_command_capture_status(
    command: &mut Command,
) -> std::result::Result<ProcessStatusOutput, ProcessCaptureError> {
    let context = CommandContext::from_command(command);
    let output = command
        .output()
        .map_err(|source| ProcessCaptureError::Spawn { context, source })?;

    let stdout = String::from_utf8(output.stdout).map_err(ProcessCaptureError::Utf8)?;
    let stderr = String::from_utf8(output.stderr).map_err(ProcessCaptureError::Utf8)?;

    Ok(ProcessStatusOutput {
        status: output.status,
        stdout,
        stderr,
    })
}

fn run_command_status(command: &mut Command) -> Result<ExitStatus> {
    let context = CommandContext::from_command(command);
    command.status().map_err(|error| context.spawn_error(error))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandContext {
    description: String,
    program: String,
}

impl CommandContext {
    fn from_command(command: &Command) -> Self {
        Self {
            description: describe_command(command),
            program: command.get_program().to_string_lossy().into_owned(),
        }
    }

    fn spawn_error(&self, error: std::io::Error) -> Error {
        if error.kind() == ErrorKind::NotFound {
            Error::msg(format!(
                "`{}` was not found on PATH; install `{}` or add it to PATH",
                self.program, self.program
            ))
        } else {
            Error::msg(format!("failed to run `{}`: {error}", self.description))
        }
    }

    fn exit_error(&self, status: ExitStatus, stdout: &str, stderr: &str) -> Error {
        Error::msg(format!(
            "`{}` exited with {}: {detail}",
            self.description,
            format_status(status),
            detail = output_detail(stdout, stderr),
        ))
    }
}

fn describe_command(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.is_empty()
        || text
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '\'' || ch == '"')
    {
        format!("{text:?}")
    } else {
        text.into_owned()
    }
}

pub(crate) fn format_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit status {code}"),
        None => "signal".to_string(),
    }
}

fn output_detail(stdout: &str, stderr: &str) -> String {
    [stderr.trim(), stdout.trim()]
        .into_iter()
        .find(|detail| !detail.is_empty())
        .unwrap_or("no output")
        .to_string()
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    use super::*;

    #[test]
    fn stderr_or_status_detail_prefers_stderr() {
        let output = ProcessStatusOutput {
            status: exit_status(42),
            stdout: "stdout detail\n".to_string(),
            stderr: "stderr detail\n".to_string(),
        };

        assert_eq!(output.stderr_or_status_detail(), "stderr detail");
    }

    #[test]
    fn stderr_or_status_detail_falls_back_to_status() {
        let output = ProcessStatusOutput {
            status: exit_status(42),
            stdout: "stdout detail\n".to_string(),
            stderr: String::new(),
        };

        assert_eq!(output.stderr_or_status_detail(), "exit status 42");
    }

    #[test]
    fn status_with_output_detail_includes_status_and_best_output_detail() {
        let output = ProcessStatusOutput {
            status: exit_status(42),
            stdout: "stdout detail\n".to_string(),
            stderr: String::new(),
        };

        assert_eq!(
            output.status_with_output_detail(),
            "exit status 42: stdout detail"
        );
    }

    fn exit_status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }
}

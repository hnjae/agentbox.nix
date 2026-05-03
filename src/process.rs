// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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

    pub fn command(&self, program: &str) -> Result<Command> {
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

    pub fn capture(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessOutput> {
        let mut command = self.command(program)?;
        configure(&mut command);
        run_command(&mut command)
    }

    pub fn status(
        &self,
        program: &str,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ExitStatus> {
        let mut command = self.command(program)?;
        configure(&mut command);
        run_command_status(&mut command)
    }
}

pub fn run_command(command: &mut Command) -> Result<ProcessOutput> {
    let context = CommandContext::from_command(command);
    let output = command
        .output()
        .map_err(|error| context.spawn_error(error))?;

    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    if !output.status.success() {
        return Err(context.exit_error(output.status, &stdout, &stderr));
    }

    Ok(ProcessOutput { stdout, stderr })
}

pub fn run_command_status(command: &mut Command) -> Result<ExitStatus> {
    let context = CommandContext::from_command(command);
    command.status().map_err(|error| context.spawn_error(error))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandContext {
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

pub fn describe_command(command: &Command) -> String {
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

pub fn format_status(status: ExitStatus) -> String {
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

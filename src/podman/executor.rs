// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::{Command, ExitStatus};

use serde::de::DeserializeOwned;

use crate::Result;
use crate::diagnostic;
use crate::process::{ProcessCommand, ProcessOutput, ProcessRunner};

use super::model::parse_json;

const PODMAN_PROGRAM: &str = "podman";

#[derive(Debug, Clone, Default)]
pub(super) struct PodmanExecutor {
    runner: ProcessRunner,
    verbose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PodmanStatus {
    pub(super) description: String,
    pub(super) status: ExitStatus,
}

impl PodmanExecutor {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn with_runner(runner: ProcessRunner) -> Self {
        Self {
            runner,
            verbose: false,
        }
    }

    pub(super) fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub(super) fn run_quiet(&self, configure: impl FnOnce(&mut Command)) -> Result<ProcessOutput> {
        let command = self.podman_command(configure)?;
        self.capture(command)
    }

    pub(super) fn run_json<T: DeserializeOwned>(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<T> {
        let command = self.podman_command(configure)?;
        let context = format!("`{}`", command.description());
        let output = self.capture(command)?;
        parse_json(&context, &output.stdout)
    }

    pub(super) fn run_forwarding_output_when_verbose(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessOutput> {
        let output = self.run_quiet(configure)?;
        if self.verbose {
            emit_output(&output);
        }

        Ok(output)
    }

    pub(super) fn status(&self, configure: impl FnOnce(&mut Command)) -> Result<PodmanStatus> {
        let command = self.podman_command(configure)?;
        let description = command.description();
        self.trace(&command);
        let status = command.status()?;

        Ok(PodmanStatus {
            description,
            status,
        })
    }

    fn podman_command(&self, configure: impl FnOnce(&mut Command)) -> Result<ProcessCommand> {
        self.runner.configured_command(PODMAN_PROGRAM, configure)
    }

    fn capture(&self, command: ProcessCommand) -> Result<ProcessOutput> {
        self.trace(&command);
        command.capture()
    }

    fn trace(&self, command: &ProcessCommand) {
        if self.verbose {
            diagnostic::debug(format!("running {}", command.description()));
        }
    }
}

fn emit_output(output: &ProcessOutput) {
    emit_stream(&output.stdout);
    emit_stream(&output.stderr);
}

fn emit_stream(text: &str) {
    if text.is_empty() {
        return;
    }

    diagnostic::debug(text);
}

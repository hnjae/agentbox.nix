// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::Command;

use camino::Utf8Path;

use crate::metadata::managed_label_filter;
use crate::process::{
    ProcessOutput, ProcessRunner, describe_command, format_status, run_command, run_command_status,
};
use crate::runtime::RuntimeCreateSpec;
use crate::{Error, Result};

mod model;
mod run;

use model::parse_json;
pub use model::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
    PodmanHealth, PodmanHostConfig, PodmanNamespaces, PodmanNetworkEndpoint, PodmanNetworkSettings,
    PodmanPortBinding, PodmanPsContainer, PodmanPsPort,
};

#[derive(Debug, Clone, Default)]
pub struct Podman {
    runner: ProcessRunner,
    verbose: bool,
}

impl Podman {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self {
            runner,
            verbose: false,
        }
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn ps(&self) -> Result<Vec<PodmanPsContainer>> {
        let mut command = self.runner.command("podman")?;
        command.args(["ps", "--all", "--filter"]);
        command.arg(managed_label_filter());
        command.args(["--format", "json"]);
        let output = self.run_quiet(&mut command)?;

        parse_json("`podman ps --all --format json`", &output.stdout)
    }

    pub fn inspect(&self, name: &str) -> Result<Vec<PodmanContainerInspect>> {
        let mut command = self.runner.command("podman")?;
        command.args(["inspect", name]);
        let output = self.run_quiet(&mut command)?;

        parse_json("`podman inspect`", &output.stdout)
    }

    pub fn inspect_one(&self, name: &str) -> Result<PodmanContainerInspect> {
        let mut containers = self.inspect(name)?;
        if containers.is_empty() {
            return Err(Error::msg(format!(
                "`podman inspect` returned no containers for `{name}`"
            )));
        }

        Ok(containers.remove(0))
    }

    pub fn image_exists(&self, image: &str) -> Result<bool> {
        let mut command = self.runner.command("podman")?;
        command.args(["image", "exists", image]);
        let status = self.status(&mut command)?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::msg(format!(
                "`podman image exists {image}` exited with {}",
                format_status(status),
            ))),
        }
    }

    pub fn container_exists(&self, container_name: &str) -> Result<bool> {
        let mut command = self.runner.command("podman")?;
        command.args(["container", "exists", container_name]);
        let status = self.status(&mut command)?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::msg(format!(
                "`podman container exists {container_name}` exited with {}",
                format_status(status),
            ))),
        }
    }

    pub fn build_image(
        &self,
        image: &str,
        containerfile: &Utf8Path,
        context_dir: &Utf8Path,
    ) -> Result<()> {
        let mut command = self.runner.command("podman")?;
        command.args([
            "build",
            "-t",
            image,
            "-f",
            containerfile.as_str(),
            context_dir.as_str(),
        ]);
        self.run_forwarding_output_when_verbose(&mut command)
            .map(|_| ())
    }

    pub fn stop_ignore(&self, container_name: &str) -> Result<()> {
        let mut command = self.runner.command("podman")?;
        command.args(["stop", "--ignore", container_name]);
        self.run_quiet(&mut command).map(|_| ())
    }

    pub fn logs_tail(&self, container_name: &str, line_count: usize) -> Result<String> {
        let mut command = self.runner.command("podman")?;
        command.args(["logs", "--tail", &line_count.to_string(), container_name]);
        self.run_quiet(&mut command).map(|output| {
            let mut logs = output.stdout;
            if !logs.is_empty() && !logs.ends_with('\n') && !output.stderr.is_empty() {
                logs.push('\n');
            }
            logs.push_str(&output.stderr);
            logs
        })
    }

    pub fn run_detached(
        &self,
        container_name: &str,
        spec: &RuntimeCreateSpec,
        workdir: Option<&str>,
    ) -> Result<()> {
        let mut command = self.runner.command("podman")?;
        command.args(run::run_detached_args(container_name, spec, workdir));
        self.run_forwarding_output_when_verbose(&mut command)
            .map(|_| ())
    }

    fn run_quiet(&self, command: &mut Command) -> Result<ProcessOutput> {
        self.trace(command);
        run_command(command)
    }

    fn run_forwarding_output_when_verbose(&self, command: &mut Command) -> Result<ProcessOutput> {
        let output = self.run_quiet(command)?;
        if self.verbose {
            emit_output(&output);
        }

        Ok(output)
    }

    fn status(&self, command: &mut Command) -> Result<std::process::ExitStatus> {
        self.trace(command);
        run_command_status(command)
    }

    fn trace(&self, command: &Command) {
        if self.verbose {
            eprintln!("agentbox: running {}", describe_command(command));
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

    eprint!("{text}");
    if !text.ends_with('\n') {
        eprintln!();
    }
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::process::{Command, ExitStatus, Stdio};

use camino::Utf8Path;
use serde::de::DeserializeOwned;

use crate::diagnostic;
use crate::metadata::managed_label_filter;
use crate::process::{
    ProcessOutput, ProcessRunner, describe_command, format_status, run_command, run_command_status,
};
use crate::runtime::RuntimeRunSpec;
use crate::{Error, Result};

mod args;
mod build;
mod model;
mod run;

use model::parse_json;
pub use model::{
    PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerMountKind,
    PodmanContainerState, PodmanHealth, PodmanHostConfig, PodmanImage, PodmanNamespaces,
    PodmanNetworkEndpoint, PodmanNetworkSettings, PodmanPortBinding, PodmanPsContainer,
    PodmanPsPort, PodmanVolume,
};

const PODMAN_PROGRAM: &str = "podman";

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
        self.run_podman_json(|command| {
            command.args(["ps", "--all", "--filter"]);
            command.arg(managed_label_filter());
            command.args(["--format", "json"]);
        })
    }

    pub fn ps_all(&self) -> Result<Vec<PodmanPsContainer>> {
        self.run_podman_json(|command| {
            command.args(["ps", "--all", "--format", "json"]);
        })
    }

    pub fn volumes(&self) -> Result<Vec<PodmanVolume>> {
        self.run_podman_json(|command| {
            command.args(["volume", "ls", "--format", "json"]);
        })
    }

    pub fn images_with_label(&self, label_filter: &str) -> Result<Vec<PodmanImage>> {
        self.run_podman_json(|command| {
            command.args(["image", "ls", "--filter", label_filter, "--format", "json"]);
        })
    }

    pub fn inspect(&self, name: &str) -> Result<Vec<PodmanContainerInspect>> {
        self.run_podman_json(|command| {
            command.args(["inspect", name]);
        })
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
        self.exists_status(|command| {
            command.args(["image", "exists", image]);
        })
    }

    pub fn remove_image(&self, image: &str) -> Result<()> {
        self.run_podman_quiet(|command| {
            command.args(["image", "rm", image]);
        })
        .map(|_| ())
    }

    pub fn remove_volume(&self, volume: &str) -> Result<()> {
        self.run_podman_quiet(|command| {
            command.args(["volume", "rm", volume]);
        })
        .map(|_| ())
    }

    pub fn volume_exists(&self, volume: &str) -> Result<bool> {
        self.exists_status(|command| {
            command.args(["volume", "exists", volume]);
        })
    }

    pub fn container_exists(&self, container_name: &str) -> Result<bool> {
        self.exists_status(|command| {
            command.args(["container", "exists", container_name]);
        })
    }

    fn exists_status(&self, configure: impl FnOnce(&mut Command)) -> Result<bool> {
        let mut command = self.podman_command(configure)?;
        let description = describe_command(&command);
        let status = self.status(&mut command)?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::msg(format!(
                "`{description}` exited with {}",
                format_status(status),
            ))),
        }
    }

    pub fn build_image(
        &self,
        image: &str,
        containerfile: &Utf8Path,
        context_dir: &Utf8Path,
        options: &PodmanBuildOptions,
    ) -> Result<()> {
        self.run_podman_forwarding_output_when_verbose(|command| {
            command.args(build::build_image_args(
                image,
                containerfile,
                context_dir,
                options,
            ));
        })
        .map(|_| ())
    }

    pub fn stop_ignore(&self, container_name: &str) -> Result<()> {
        self.run_podman_quiet(|command| {
            command.args(["stop", "--ignore", container_name]);
        })
        .map(|_| ())
    }

    pub fn logs_tail(&self, container_name: &str, line_count: usize) -> Result<String> {
        self.run_podman_quiet(|command| {
            command.args(["logs", "--tail"]);
            command.arg(line_count.to_string());
            command.arg(container_name);
        })
        .map(|output| {
            let mut logs = output.stdout;
            if !logs.is_empty() && !logs.ends_with('\n') && !output.stderr.is_empty() {
                logs.push('\n');
            }
            logs.push_str(&output.stderr);
            logs
        })
    }

    pub fn run_detached(&self, container_name: &str, spec: &RuntimeRunSpec) -> Result<()> {
        let host_gid = run::current_primary_gid();
        self.run_podman_forwarding_output_when_verbose(|command| {
            command.args(run::run_detached_args(container_name, spec, host_gid));
        })
        .map(|_| ())
    }

    pub fn run_foreground(
        &self,
        container_name: &str,
        spec: &RuntimeRunSpec,
        use_tty: bool,
    ) -> Result<ExitStatus> {
        let host_gid = run::current_primary_gid();
        let mut command = self.podman_command(|command| {
            command.args(run::run_foreground_args(
                container_name,
                spec,
                host_gid,
                use_tty,
            ));
            command.stdin(Stdio::inherit());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        })?;
        self.status(&mut command)
    }

    fn podman_command(&self, configure: impl FnOnce(&mut Command)) -> Result<Command> {
        let mut command = self.runner.command(PODMAN_PROGRAM)?;
        configure(&mut command);
        Ok(command)
    }

    fn run_podman_quiet(&self, configure: impl FnOnce(&mut Command)) -> Result<ProcessOutput> {
        let mut command = self.podman_command(configure)?;
        self.run_quiet(&mut command)
    }

    fn run_podman_json<T: DeserializeOwned>(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<T> {
        let mut command = self.podman_command(configure)?;
        let context = format!("`{}`", describe_command(&command));
        let output = self.run_quiet(&mut command)?;
        parse_json(&context, &output.stdout)
    }

    fn run_podman_forwarding_output_when_verbose(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessOutput> {
        let mut command = self.podman_command(configure)?;
        self.run_forwarding_output_when_verbose(&mut command)
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
            diagnostic::debug(format!("running {}", describe_command(command)));
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PodmanBuildOptions {
    pub build_args: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
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

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn json_parse_errors_describe_the_configured_command() {
        let sandbox = tempfile::tempdir().unwrap();
        let fake_podman = sandbox.path().join(PODMAN_PROGRAM);
        fs::write(&fake_podman, "#!/bin/sh\nprintf 'not json\\n'\n").unwrap();
        fs::set_permissions(&fake_podman, fs::Permissions::from_mode(0o700)).unwrap();

        let runner = ProcessRunner::new().with_path_prepend(sandbox.path());
        let error = Podman::with_runner(runner).ps().unwrap_err();

        assert!(error.to_string().contains(
            "failed to parse `podman ps --all --filter label=io.agentbox.managed=true --format json`"
        ));
    }
}

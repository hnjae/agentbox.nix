// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

use crate::metadata::managed_label_filter;
use crate::process::{ProcessRunner, format_status, run_command};
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
}

impl Podman {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn ps(&self) -> Result<Vec<PodmanPsContainer>> {
        let output = self.runner.capture("podman", |command| {
            command.args(["ps", "--all", "--filter"]);
            command.arg(managed_label_filter());
            command.args(["--format", "json"]);
        })?;

        parse_json("`podman ps --all --format json`", &output.stdout)
    }

    pub fn inspect(&self, name: &str) -> Result<Vec<PodmanContainerInspect>> {
        let output = self.runner.capture("podman", |command| {
            command.args(["inspect", name]);
        })?;

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
        let status = self.runner.status("podman", |command| {
            command.args(["image", "exists", image]);
        })?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::msg(format!(
                "`podman image exists {image}` exited with {}",
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
        self.runner
            .capture("podman", |command| {
                command.args([
                    "build",
                    "-t",
                    image,
                    "-f",
                    containerfile.as_str(),
                    context_dir.as_str(),
                ]);
            })
            .map(|_| ())
    }

    pub fn stop_ignore(&self, container_name: &str) -> Result<()> {
        self.runner
            .capture("podman", |command| {
                command.args(["stop", "--ignore", container_name]);
            })
            .map(|_| ())
    }

    pub fn run_detached(
        &self,
        container_name: &str,
        spec: &RuntimeCreateSpec,
        workdir: Option<&str>,
    ) -> Result<()> {
        let mut command = self.runner.command("podman")?;
        command.args(run::run_detached_args(container_name, spec, workdir));
        run_command(&mut command).map(|_| ())
    }
}

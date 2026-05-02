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
use crate::runtime::{RuntimeCreateSpec, RuntimeMount};
use crate::{Error, Result};

mod model;

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
        command.args(run_detached_args(container_name, spec, workdir));
        run_command(&mut command).map(|_| ())
    }
}

fn run_detached_args(
    container_name: &str,
    spec: &RuntimeCreateSpec,
    workdir: Option<&str>,
) -> Vec<String> {
    let mut args = strings(["run", "--detach", "--rm", "--rmi", "--name"]);
    args.push(container_name.to_string());

    if let Some(workdir) = workdir {
        args.extend(strings(["--workdir", workdir]));
    }

    for (name, value) in &spec.labels {
        args.extend(strings(["--label"]));
        args.push(format!("{name}={value}"));
    }

    for mount in &spec.mounts {
        args.extend(strings(["--mount"]));
        args.push(render_mount(mount));
    }

    for (name, value) in &spec.default_env {
        args.extend(strings(["--env"]));
        args.push(format!("{name}={value}"));
    }

    if !spec.network_enabled {
        args.extend(strings(["--network=none"]));
    }

    for port in &spec.published_ports {
        args.extend(strings(["--publish", port]));
    }

    args.push(spec.image.clone());
    args.extend(spec.command.iter().cloned());
    args
}

fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_string).collect()
}

fn render_mount(mount: &RuntimeMount) -> String {
    let mut options = vec![
        format!("type={}", mount.kind.podman_type()),
        format!("src={}", mount.source),
        format!("dst={}", mount.destination),
    ];
    if mount.read_only {
        options.push("ro".to_string());
    }
    options.join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn run_detached_args_are_stable_and_complete() {
        let spec = RuntimeCreateSpec {
            image: "localhost/agentbox-opencode:local".to_string(),
            labels: BTreeMap::from([
                ("io.agentbox.managed".to_string(), "true".to_string()),
                ("io.agentbox.runtime".to_string(), "opencode".to_string()),
            ]),
            mounts: vec![
                RuntimeMount::read_only_bind("/workspace", "/workspace"),
                RuntimeMount::volume("agentbox-cache", "/home/user/.cache/nix"),
            ],
            command: strings(["opencode", "serve", "--port", "4096"]),
            default_env: BTreeMap::from([(
                "NIX_CONFIG".to_string(),
                "sandbox = false".to_string(),
            )]),
            network_enabled: false,
            published_ports: vec!["127.0.0.1::4096".to_string()],
        };

        assert_eq!(
            run_detached_args("agentbox-demo", &spec, Some("/workspace")),
            strings([
                "run",
                "--detach",
                "--rm",
                "--rmi",
                "--name",
                "agentbox-demo",
                "--workdir",
                "/workspace",
                "--label",
                "io.agentbox.managed=true",
                "--label",
                "io.agentbox.runtime=opencode",
                "--mount",
                "type=bind,src=/workspace,dst=/workspace,ro",
                "--mount",
                "type=volume,src=agentbox-cache,dst=/home/user/.cache/nix",
                "--env",
                "NIX_CONFIG=sandbox = false",
                "--network=none",
                "--publish",
                "127.0.0.1::4096",
                "localhost/agentbox-opencode:local",
                "opencode",
                "serve",
                "--port",
                "4096",
            ])
        );
    }
}

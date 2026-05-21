// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::{Command, ExitStatus, Stdio};

use camino::Utf8Path;

use crate::metadata::managed_label_filter;
use crate::process::{ProcessOutput, ProcessRunner, format_status};
use crate::runtime::RuntimeRunSpec;
use crate::{Error, Result};

use super::build::{self, PodmanBuildOptions};
use super::executor::PodmanExecutor;
use super::run;
use super::{PodmanContainerInspect, PodmanImage, PodmanPsContainer, PodmanVolume};

#[derive(Debug, Clone, Default)]
pub struct Podman {
    executor: PodmanExecutor,
}

impl Podman {
    pub fn new() -> Self {
        Self {
            executor: PodmanExecutor::new(),
        }
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self {
            executor: PodmanExecutor::with_runner(runner),
        }
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.executor = self.executor.with_verbose(verbose);
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
            command.args(["container", "inspect", name]);
        })
    }

    pub fn inspect_one(&self, name: &str) -> Result<PodmanContainerInspect> {
        let mut containers = self.inspect(name)?;
        if containers.is_empty() {
            return Err(Error::msg(format!(
                "`podman container inspect` returned no containers for `{name}`"
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
        let status = self.executor.status(configure)?;

        match status.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error::msg(format!(
                "`{}` exited with {}",
                status.description,
                format_status(status.status),
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
        self.executor
            .status(|command| {
                command.args(run::run_foreground_args(
                    container_name,
                    spec,
                    host_gid,
                    use_tty,
                ));
                command.stdin(Stdio::inherit());
                command.stdout(Stdio::inherit());
                command.stderr(Stdio::inherit());
            })
            .map(|status| status.status)
    }

    fn run_podman_quiet(&self, configure: impl FnOnce(&mut Command)) -> Result<ProcessOutput> {
        self.executor.run_quiet(configure)
    }

    fn run_podman_json<T: serde::de::DeserializeOwned>(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<T> {
        self.executor.run_json(configure)
    }

    fn run_podman_forwarding_output_when_verbose(
        &self,
        configure: impl FnOnce(&mut Command),
    ) -> Result<ProcessOutput> {
        self.executor.run_forwarding_output_when_verbose(configure)
    }
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
        let fake_podman = sandbox.path().join("podman");
        fs::write(&fake_podman, "#!/bin/sh\nprintf 'not json\\n'\n").unwrap();
        fs::set_permissions(&fake_podman, fs::Permissions::from_mode(0o700)).unwrap();

        let runner = ProcessRunner::new().with_path_prepend(sandbox.path());
        let error = Podman::with_runner(runner).ps().unwrap_err();

        assert!(error.to_string().contains(
            "failed to parse `podman ps --all --filter label=io.agentbox.managed=true --format json`"
        ));
    }
}

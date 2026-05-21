// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::metadata::{DefaultRuntimeImageLabelInput, default_runtime_image_labels};
use crate::podman::{Podman, PodmanBuildOptions};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeKind;
use crate::runtime::default_image::default_image_context_hash;
use crate::{Error, Result};

use super::image_state::{RuntimeImageState, read_runtime_image_state, write_runtime_image_state};

pub(super) trait RuntimeImageEnvironment {
    fn image_exists(&mut self, image: &str) -> Result<bool>;
    fn read_state(&mut self, runtime: RuntimeKind) -> Result<Option<RuntimeImageState>>;
    fn write_state(&mut self, runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()>;
    fn resolve_latest_version(&mut self, package: &str) -> Result<String>;
    fn build_image(&mut self, runtime: RuntimeKind, version: &str) -> Result<()>;
    fn now_unix_seconds(&mut self) -> Result<u64>;
}

pub(super) struct ProductionRuntimeImageEnvironment<'a> {
    pub(super) podman: &'a Podman,
}

impl RuntimeImageEnvironment for ProductionRuntimeImageEnvironment<'_> {
    fn image_exists(&mut self, image: &str) -> Result<bool> {
        self.podman.image_exists(image)
    }

    fn read_state(&mut self, runtime: RuntimeKind) -> Result<Option<RuntimeImageState>> {
        read_runtime_image_state(runtime)
    }

    fn write_state(&mut self, runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()> {
        write_runtime_image_state(runtime, state)
    }

    fn resolve_latest_version(&mut self, package: &str) -> Result<String> {
        resolve_latest_runtime_version(package)
    }

    fn build_image(&mut self, runtime: RuntimeKind, version: &str) -> Result<()> {
        build_runtime_image(self.podman, runtime, version)
    }

    fn now_unix_seconds(&mut self) -> Result<u64> {
        now_unix_seconds()
    }
}

fn build_runtime_image(podman: &Podman, runtime: RuntimeKind, version: &str) -> Result<()> {
    let package = runtime.package_spec();
    let context = runtime.materialize_default_image_context()?;
    let image = runtime.default_image();
    let resolved_at = now_unix_seconds()?.to_string();
    let options = PodmanBuildOptions {
        build_args: BTreeMap::from([
            ("AGENTBOX_RUNTIME".to_string(), runtime.as_str().to_string()),
            (package.build_arg.to_string(), version.to_string()),
        ]),
        labels: default_runtime_image_labels(DefaultRuntimeImageLabelInput {
            runtime,
            image: &image,
            image_context_hash: default_image_context_hash(),
            version,
            resolved_at: &resolved_at,
        }),
    };

    podman.build_image(
        &image,
        context.containerfile().as_ref(),
        context.root(),
        &options,
    )
}

fn resolve_latest_runtime_version(package: &str) -> Result<String> {
    let output = ProcessRunner::new().capture("npm", |command| {
        command.args(["view", package, "version", "--silent"]);
    })?;
    let version = output.stdout.trim();
    if version.is_empty() {
        return Err(Error::msg(format!(
            "`npm view {package} version --silent` returned an empty version"
        )));
    }

    Ok(version.to_string())
}

fn now_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::msg(format!("system clock is before Unix epoch: {error}")))?
        .as_secs())
}

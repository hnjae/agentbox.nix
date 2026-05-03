// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8Path;

use crate::cli::{RuntimeArgs, RuntimeCommand};
use crate::podman::{Podman, PodmanBuildOptions};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

mod image_state;

use image_state::{RuntimeImageState, read_runtime_image_state, write_runtime_image_state};

pub fn run(args: RuntimeArgs, verbose: bool) -> Result<()> {
    match args.command {
        RuntimeCommand::Update(args) => update(args.runtime, verbose),
    }
}

pub(super) fn ensure_default_runtime_image(
    podman: &Podman,
    runtime: RuntimeKind,
    workspace_root: &Utf8Path,
    mut phase: impl FnMut(String),
) -> Result<Option<String>> {
    let default_image = runtime.default_image();
    if podman.image_exists(default_image)? {
        phase(format!("using runtime image `{default_image}`"));
        return installed_version_if_known(runtime, default_image);
    }

    phase(format!("building runtime image `{default_image}`"));
    build_default_runtime_image(podman, runtime).map_err(|error| {
        Error::msg(format!(
            "failed to build default runtime image `{default_image}` for `{}`: {error}",
            workspace_root,
        ))
    })
}

fn update(runtime: RuntimeKind, verbose: bool) -> Result<()> {
    let package = runtime.package_spec();
    let podman = Podman::new().with_verbose(verbose);
    eprintln!("agentbox: resolving latest `{}` version", package.name);
    let latest_version = resolve_latest_runtime_version(package.name)?;
    let image = runtime.default_image();
    let image_exists = podman.image_exists(image)?;
    let prior_state = read_runtime_image_state(runtime)?;

    if image_exists
        && prior_state
            .as_ref()
            .is_some_and(|state| state.installed_version == latest_version && state.image == image)
    {
        let state = prior_state
            .expect("state exists because the up-to-date predicate was true")
            .with_latest_check(latest_version.clone(), now_unix_seconds()?);
        write_runtime_image_state(runtime, &state)?;
        println!("{runtime} runtime image `{image}` is already up to date at {latest_version}");
        return Ok(());
    }

    eprintln!(
        "agentbox: building runtime image `{image}` with `{}@{latest_version}`",
        package.name
    );
    build_runtime_image(&podman, runtime, &latest_version)?;
    let now = now_unix_seconds()?;
    let state = RuntimeImageState::new(runtime, latest_version.clone(), now, now);
    write_runtime_image_state(runtime, &state)?;
    println!("updated {runtime} runtime image `{image}` to {latest_version}");
    Ok(())
}

fn build_default_runtime_image(podman: &Podman, runtime: RuntimeKind) -> Result<Option<String>> {
    let package = runtime.package_spec();
    let latest_version = resolve_latest_runtime_version(package.name)?;
    build_runtime_image(podman, runtime, &latest_version)?;
    let now = now_unix_seconds()?;
    write_runtime_image_state(
        runtime,
        &RuntimeImageState::new(runtime, latest_version.clone(), now, now),
    )?;
    Ok(Some(latest_version))
}

fn build_runtime_image(podman: &Podman, runtime: RuntimeKind, version: &str) -> Result<()> {
    let package = runtime.package_spec();
    let context = runtime.materialize_default_image_context()?;
    let resolved_at = now_unix_seconds()?.to_string();
    let options = PodmanBuildOptions {
        build_args: BTreeMap::from([
            ("AGENTBOX_RUNTIME".to_string(), runtime.as_str().to_string()),
            (package.build_arg.to_string(), version.to_string()),
        ]),
        labels: BTreeMap::from([
            (package.package_label.to_string(), package.name.to_string()),
            (package.version_label.to_string(), version.to_string()),
            (
                package.install_source_label.to_string(),
                package.install_source.to_string(),
            ),
            (package.resolved_at_label.to_string(), resolved_at),
        ]),
    };

    podman.build_image(
        runtime.default_image(),
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

fn installed_version_if_known(runtime: RuntimeKind, image: &str) -> Result<Option<String>> {
    Ok(read_runtime_image_state(runtime)?
        .filter(|state| state.image == image)
        .map(|state| state.installed_version))
}

fn now_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::msg(format!("system clock is before Unix epoch: {error}")))?
        .as_secs())
}

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8Path;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::cli::{RuntimeArgs, RuntimeCommand};
use crate::metadata::{
    LABEL_CODEX_INSTALL_SOURCE, LABEL_CODEX_PACKAGE, LABEL_CODEX_RESOLVED_AT, LABEL_CODEX_VERSION,
    LABEL_OPENCODE_INSTALL_SOURCE, LABEL_OPENCODE_PACKAGE, LABEL_OPENCODE_RESOLVED_AT,
    LABEL_OPENCODE_VERSION,
};
use crate::podman::{Podman, PodmanBuildOptions};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

const CODEX_NPM_PACKAGE: &str = "@openai/codex";
const OPENCODE_NPM_PACKAGE: &str = "opencode-ai";
const NPM_INSTALL_SOURCE: &str = "npm";

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
    let package = runtime_package_spec(runtime);
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
    let package = runtime_package_spec(runtime);
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
    let package = runtime_package_spec(runtime);
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
                NPM_INSTALL_SOURCE.to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RuntimeImageState {
    runtime: String,
    package: String,
    install_source: String,
    image: String,
    installed_version: String,
    latest_seen_version: String,
    latest_checked_at: u64,
    image_built_at: u64,
}

impl RuntimeImageState {
    fn new(
        runtime: RuntimeKind,
        version: String,
        latest_checked_at: u64,
        image_built_at: u64,
    ) -> Self {
        let package = runtime_package_spec(runtime);
        Self {
            runtime: runtime.as_str().to_string(),
            package: package.name.to_string(),
            install_source: NPM_INSTALL_SOURCE.to_string(),
            image: runtime.default_image().to_string(),
            installed_version: version.clone(),
            latest_seen_version: version,
            latest_checked_at,
            image_built_at,
        }
    }

    fn with_latest_check(mut self, latest_version: String, latest_checked_at: u64) -> Self {
        self.latest_seen_version = latest_version;
        self.latest_checked_at = latest_checked_at;
        self
    }
}

fn read_runtime_image_state(runtime: RuntimeKind) -> Result<Option<RuntimeImageState>> {
    let path = runtime_image_state_path(runtime)?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map(Some).map_err(|error| {
        Error::msg(format!(
            "failed to parse {runtime} runtime image state `{}`: {error}",
            path.display()
        ))
    })
}

fn write_runtime_image_state(runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()> {
    let path = runtime_image_state_path(runtime)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(|error| {
        Error::msg(format!(
            "failed to serialize {runtime} runtime image state: {error}"
        ))
    })?;
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

fn runtime_image_state_path(runtime: RuntimeKind) -> Result<PathBuf> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let state_dir = base_dirs
        .state_dir()
        .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;

    Ok(state_dir
        .join("agentbox")
        .join("runtime")
        .join(format!("{}.json", runtime.as_str())))
}

#[derive(Debug, Clone, Copy)]
struct RuntimePackageSpec {
    name: &'static str,
    build_arg: &'static str,
    package_label: &'static str,
    version_label: &'static str,
    install_source_label: &'static str,
    resolved_at_label: &'static str,
}

fn runtime_package_spec(runtime: RuntimeKind) -> RuntimePackageSpec {
    match runtime {
        RuntimeKind::Opencode => RuntimePackageSpec {
            name: OPENCODE_NPM_PACKAGE,
            build_arg: "OPENCODE_NPM_VERSION",
            package_label: LABEL_OPENCODE_PACKAGE,
            version_label: LABEL_OPENCODE_VERSION,
            install_source_label: LABEL_OPENCODE_INSTALL_SOURCE,
            resolved_at_label: LABEL_OPENCODE_RESOLVED_AT,
        },
        RuntimeKind::Codex => RuntimePackageSpec {
            name: CODEX_NPM_PACKAGE,
            build_arg: "CODEX_NPM_VERSION",
            package_label: LABEL_CODEX_PACKAGE,
            version_label: LABEL_CODEX_VERSION,
            install_source_label: LABEL_CODEX_INSTALL_SOURCE,
            resolved_at_label: LABEL_CODEX_RESOLVED_AT,
        },
    }
}

fn now_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::msg(format!("system clock is before Unix epoch: {error}")))?
        .as_secs())
}

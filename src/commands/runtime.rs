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
};
use crate::podman::{Podman, PodmanBuildOptions};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

const CODEX_NPM_PACKAGE: &str = "@openai/codex";
const CODEX_INSTALL_SOURCE: &str = "npm";
const CODEX_STATE_RELATIVE_PATH: &[&str] = &["agentbox", "runtime", "codex.json"];

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
        return codex_installed_version_if_known(runtime, default_image);
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
    if runtime != RuntimeKind::Codex {
        return Err(Error::msg(format!(
            "`agentbox runtime update` supports `codex` only in the MVP; unsupported runtime `{runtime}`"
        )));
    }

    let podman = Podman::new().with_verbose(verbose);
    eprintln!("agentbox: resolving latest `{CODEX_NPM_PACKAGE}` version");
    let latest_version = resolve_latest_codex_version()?;
    let image = RuntimeKind::Codex.default_image();
    let image_exists = podman.image_exists(image)?;
    let prior_state = read_codex_image_state()?;

    if image_exists
        && prior_state
            .as_ref()
            .is_some_and(|state| state.installed_version == latest_version && state.image == image)
    {
        let state = prior_state
            .expect("state exists because the up-to-date predicate was true")
            .with_latest_check(latest_version.clone(), now_unix_seconds()?);
        write_codex_image_state(&state)?;
        println!("codex runtime image `{image}` is already up to date at {latest_version}");
        return Ok(());
    }

    eprintln!(
        "agentbox: building runtime image `{image}` with `{CODEX_NPM_PACKAGE}@{latest_version}`"
    );
    build_codex_runtime_image(&podman, &latest_version)?;
    let now = now_unix_seconds()?;
    let state = CodexImageState::new(latest_version.clone(), now, now);
    write_codex_image_state(&state)?;
    println!("updated codex runtime image `{image}` to {latest_version}");
    Ok(())
}

fn build_default_runtime_image(podman: &Podman, runtime: RuntimeKind) -> Result<Option<String>> {
    match runtime {
        RuntimeKind::Opencode => {
            build_opencode_runtime_image(podman)?;
            Ok(None)
        }
        RuntimeKind::Codex => {
            let latest_version = resolve_latest_codex_version()?;
            build_codex_runtime_image(podman, &latest_version)?;
            let now = now_unix_seconds()?;
            write_codex_image_state(&CodexImageState::new(latest_version.clone(), now, now))?;
            Ok(Some(latest_version))
        }
    }
}

fn build_opencode_runtime_image(podman: &Podman) -> Result<()> {
    let runtime = RuntimeKind::Opencode;
    let context = runtime.materialize_default_image_context()?;
    podman.build_image(
        runtime.default_image(),
        context.containerfile().as_ref(),
        context.root(),
        &PodmanBuildOptions::default(),
    )
}

fn build_codex_runtime_image(podman: &Podman, version: &str) -> Result<()> {
    let runtime = RuntimeKind::Codex;
    let context = runtime.materialize_default_image_context()?;
    let resolved_at = now_unix_seconds()?.to_string();
    let options = PodmanBuildOptions {
        build_args: BTreeMap::from([
            ("AGENTBOX_RUNTIME".to_string(), "codex".to_string()),
            ("CODEX_NPM_VERSION".to_string(), version.to_string()),
        ]),
        labels: BTreeMap::from([
            (
                LABEL_CODEX_PACKAGE.to_string(),
                CODEX_NPM_PACKAGE.to_string(),
            ),
            (LABEL_CODEX_VERSION.to_string(), version.to_string()),
            (
                LABEL_CODEX_INSTALL_SOURCE.to_string(),
                CODEX_INSTALL_SOURCE.to_string(),
            ),
            (LABEL_CODEX_RESOLVED_AT.to_string(), resolved_at),
        ]),
    };

    podman.build_image(
        runtime.default_image(),
        context.containerfile().as_ref(),
        context.root(),
        &options,
    )
}

fn resolve_latest_codex_version() -> Result<String> {
    let output = ProcessRunner::new().capture("npm", |command| {
        command.args(["view", CODEX_NPM_PACKAGE, "version", "--silent"]);
    })?;
    let version = output.stdout.trim();
    if version.is_empty() {
        return Err(Error::msg(format!(
            "`npm view {CODEX_NPM_PACKAGE} version --silent` returned an empty version"
        )));
    }

    Ok(version.to_string())
}

fn codex_installed_version_if_known(runtime: RuntimeKind, image: &str) -> Result<Option<String>> {
    if runtime != RuntimeKind::Codex {
        return Ok(None);
    }

    Ok(read_codex_image_state()?
        .filter(|state| state.image == image)
        .map(|state| state.installed_version))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CodexImageState {
    runtime: String,
    package: String,
    install_source: String,
    image: String,
    installed_version: String,
    latest_seen_version: String,
    latest_checked_at: u64,
    image_built_at: u64,
}

impl CodexImageState {
    fn new(version: String, latest_checked_at: u64, image_built_at: u64) -> Self {
        Self {
            runtime: RuntimeKind::Codex.as_str().to_string(),
            package: CODEX_NPM_PACKAGE.to_string(),
            install_source: CODEX_INSTALL_SOURCE.to_string(),
            image: RuntimeKind::Codex.default_image().to_string(),
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

fn read_codex_image_state() -> Result<Option<CodexImageState>> {
    let path = codex_image_state_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map(Some).map_err(|error| {
        Error::msg(format!(
            "failed to parse Codex runtime image state `{}`: {error}",
            path.display()
        ))
    })
}

fn write_codex_image_state(state: &CodexImageState) -> Result<()> {
    let path = codex_image_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(|error| {
        Error::msg(format!(
            "failed to serialize Codex runtime image state: {error}"
        ))
    })?;
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

fn codex_image_state_path() -> Result<PathBuf> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;
    let state_dir = base_dirs
        .state_dir()
        .ok_or_else(|| Error::msg("failed to resolve XDG state directory"))?;

    Ok(CODEX_STATE_RELATIVE_PATH
        .iter()
        .fold(state_dir.to_path_buf(), |path, component| {
            path.join(component)
        }))
}

fn now_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::msg(format!("system clock is before Unix epoch: {error}")))?
        .as_secs())
}

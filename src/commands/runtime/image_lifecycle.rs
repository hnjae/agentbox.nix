// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8Path;

use crate::diagnostic;
use crate::metadata::{DefaultRuntimeImageLabelInput, default_runtime_image_labels};
use crate::podman::{Podman, PodmanBuildOptions};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeKind;
use crate::runtime::default_image::default_image_context_hash;
use crate::{Error, Result};

use super::image_state::{RuntimeImageState, read_runtime_image_state, write_runtime_image_state};

pub(super) fn update_default_runtime_image(runtime: RuntimeKind, verbose: bool) -> Result<()> {
    let package = runtime.package_spec();
    let podman = Podman::new().with_verbose(verbose);
    diagnostic::info(format!("resolving latest `{}` version", package.name));
    let latest_version = resolve_latest_runtime_version(package.name)?;
    let image = runtime.default_image();
    let image_exists = podman.image_exists(&image)?;
    let prior_state = read_runtime_image_state(runtime)?;

    match plan_runtime_image_update(runtime, latest_version, image_exists, prior_state) {
        RuntimeImageUpdatePlan::RefreshState {
            state,
            latest_version,
        } => {
            let state = state.with_latest_check(latest_version.clone(), now_unix_seconds()?);
            write_runtime_image_state(runtime, &state)?;
            diagnostic::info(format!(
                "{runtime} runtime image `{image}` is already up to date at {latest_version}"
            ));
        }
        RuntimeImageUpdatePlan::Rebuild { version } => {
            diagnostic::info(format!(
                "building runtime image `{image}` with `{}@{version}`",
                package.name
            ));
            build_runtime_image_and_record_state(&podman, runtime, &version)?;
            diagnostic::info(format!(
                "updated {runtime} runtime image `{image}` to {version}"
            ));
        }
    }

    Ok(())
}

pub(crate) fn ensure_default_runtime_image(
    podman: &Podman,
    runtime: RuntimeKind,
    workspace_root: &Utf8Path,
    mut phase: impl FnMut(String),
) -> Result<Option<String>> {
    let default_image = runtime.default_image();
    if podman.image_exists(&default_image)? {
        phase(format!("using runtime image `{default_image}`"));
        return installed_version_if_known(runtime, &default_image);
    }

    phase(format!("building runtime image `{default_image}`"));
    build_default_runtime_image(podman, runtime).map_err(|error| {
        Error::msg(format!(
            "failed to build default runtime image `{default_image}` for `{}`: {error}",
            workspace_root,
        ))
    })
}

pub(crate) fn remove_default_runtime_image_state_if_image(
    runtime: RuntimeKind,
    image: &str,
) -> Result<()> {
    super::image_state::remove_runtime_image_state_if_image(runtime, image)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeImageUpdatePlan {
    RefreshState {
        state: RuntimeImageState,
        latest_version: String,
    },
    Rebuild {
        version: String,
    },
}

fn plan_runtime_image_update(
    runtime: RuntimeKind,
    latest_version: String,
    image_exists: bool,
    prior_state: Option<RuntimeImageState>,
) -> RuntimeImageUpdatePlan {
    let image = runtime.default_image();
    let context_hash = default_image_context_hash();

    if let Some(state) = prior_state.filter(|state| {
        image_exists
            && state.installed_version == latest_version
            && state.image == image
            && state.image_context_hash == context_hash
    }) {
        RuntimeImageUpdatePlan::RefreshState {
            state,
            latest_version,
        }
    } else {
        RuntimeImageUpdatePlan::Rebuild {
            version: latest_version,
        }
    }
}

fn build_default_runtime_image(podman: &Podman, runtime: RuntimeKind) -> Result<Option<String>> {
    let package = runtime.package_spec();
    let latest_version = resolve_latest_runtime_version(package.name)?;
    build_runtime_image_and_record_state(podman, runtime, &latest_version)?;
    Ok(Some(latest_version))
}

fn build_runtime_image_and_record_state(
    podman: &Podman,
    runtime: RuntimeKind,
    version: &str,
) -> Result<()> {
    build_runtime_image(podman, runtime, version)?;
    let now = now_unix_seconds()?;
    write_runtime_image_state(
        runtime,
        &RuntimeImageState::new(runtime, version.to_string(), now, now),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_plan_refreshes_state_when_image_state_and_version_are_current() {
        let runtime = RuntimeKind::Codex;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.3".to_string(), true, Some(state));

        assert!(matches!(
            plan,
            RuntimeImageUpdatePlan::RefreshState {
                latest_version,
                ..
            } if latest_version == "1.2.3"
        ));
    }

    #[test]
    fn update_plan_rebuilds_when_image_is_missing_even_if_state_is_current() {
        let runtime = RuntimeKind::Codex;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.3".to_string(), false, Some(state));

        assert_eq!(
            plan,
            RuntimeImageUpdatePlan::Rebuild {
                version: "1.2.3".to_string(),
            }
        );
    }

    #[test]
    fn update_plan_rebuilds_when_state_version_is_stale() {
        let runtime = RuntimeKind::Opencode;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9);

        let plan = plan_runtime_image_update(runtime, "1.2.4".to_string(), true, Some(state));

        assert_eq!(
            plan,
            RuntimeImageUpdatePlan::Rebuild {
                version: "1.2.4".to_string(),
            }
        );
    }
}

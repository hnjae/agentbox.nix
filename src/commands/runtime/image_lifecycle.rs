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
    let podman = Podman::new().with_verbose(verbose);
    let mut environment = ProductionRuntimeImageEnvironment { podman: &podman };

    match update_default_runtime_image_with(runtime, &mut environment)? {
        RuntimeImageUpdateOutcome::AlreadyUpToDate { image, version } => {
            diagnostic::info(format!(
                "{runtime} runtime image `{image}` is already up to date at {version}"
            ));
        }
        RuntimeImageUpdateOutcome::Rebuilt { image, version } => {
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
    phase: impl FnMut(String),
) -> Result<Option<String>> {
    let default_image = runtime.default_image();
    let mut environment = ProductionRuntimeImageEnvironment { podman };
    ensure_default_runtime_image_with(
        &mut environment,
        runtime,
        workspace_root,
        phase,
        &default_image,
    )
}

pub(crate) fn remove_default_runtime_image_state_if_image(
    runtime: RuntimeKind,
    image: &str,
) -> Result<()> {
    super::image_state::remove_runtime_image_state_if_image(runtime, image)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeImageUpdateOutcome {
    AlreadyUpToDate { image: String, version: String },
    Rebuilt { image: String, version: String },
}

trait RuntimeImageEnvironment {
    fn image_exists(&mut self, image: &str) -> Result<bool>;
    fn read_state(&mut self, runtime: RuntimeKind) -> Result<Option<RuntimeImageState>>;
    fn write_state(&mut self, runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()>;
    fn resolve_latest_version(&mut self, package: &str) -> Result<String>;
    fn build_image(&mut self, runtime: RuntimeKind, version: &str) -> Result<()>;
    fn now_unix_seconds(&mut self) -> Result<u64>;
}

struct ProductionRuntimeImageEnvironment<'a> {
    podman: &'a Podman,
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

fn update_default_runtime_image_with(
    runtime: RuntimeKind,
    environment: &mut impl RuntimeImageEnvironment,
) -> Result<RuntimeImageUpdateOutcome> {
    let package = runtime.package_spec();
    diagnostic::info(format!("resolving latest `{}` version", package.name));
    let latest_version = environment.resolve_latest_version(package.name)?;
    let image = runtime.default_image();
    let image_exists = environment.image_exists(&image)?;
    let prior_state = environment.read_state(runtime)?;

    match plan_runtime_image_update(runtime, latest_version, image_exists, prior_state) {
        RuntimeImageUpdatePlan::RefreshState {
            state,
            latest_version,
        } => {
            let state =
                state.with_latest_check(latest_version.clone(), environment.now_unix_seconds()?);
            environment.write_state(runtime, &state)?;
            Ok(RuntimeImageUpdateOutcome::AlreadyUpToDate {
                image,
                version: latest_version,
            })
        }
        RuntimeImageUpdatePlan::Rebuild { version } => {
            diagnostic::info(format!(
                "building runtime image `{image}` with `{}@{version}`",
                package.name
            ));
            build_runtime_image_and_record_state(environment, runtime, &version)?;
            Ok(RuntimeImageUpdateOutcome::Rebuilt { image, version })
        }
    }
}

fn ensure_default_runtime_image_with(
    environment: &mut impl RuntimeImageEnvironment,
    runtime: RuntimeKind,
    workspace_root: &Utf8Path,
    mut phase: impl FnMut(String),
    default_image: &str,
) -> Result<Option<String>> {
    if environment.image_exists(default_image)? {
        phase(format!("using runtime image `{default_image}`"));
        return installed_version_if_known(environment, runtime, default_image);
    }

    phase(format!("building runtime image `{default_image}`"));
    build_default_runtime_image(environment, runtime).map_err(|error| {
        Error::msg(format!(
            "failed to build default runtime image `{default_image}` for `{}`: {error}",
            workspace_root,
        ))
    })
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

fn build_default_runtime_image(
    environment: &mut impl RuntimeImageEnvironment,
    runtime: RuntimeKind,
) -> Result<Option<String>> {
    let package = runtime.package_spec();
    let latest_version = environment.resolve_latest_version(package.name)?;
    build_runtime_image_and_record_state(environment, runtime, &latest_version)?;
    Ok(Some(latest_version))
}

fn build_runtime_image_and_record_state(
    environment: &mut impl RuntimeImageEnvironment,
    runtime: RuntimeKind,
    version: &str,
) -> Result<()> {
    environment.build_image(runtime, version)?;
    let now = environment.now_unix_seconds()?;
    environment.write_state(
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

fn installed_version_if_known(
    environment: &mut impl RuntimeImageEnvironment,
    runtime: RuntimeKind,
    image: &str,
) -> Result<Option<String>> {
    Ok(environment
        .read_state(runtime)?
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
    use camino::Utf8Path;

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

    #[test]
    fn update_runtime_image_refreshes_state_without_rebuild() {
        let runtime = RuntimeKind::Codex;
        let mut environment = TestRuntimeImageEnvironment::new()
            .with_image_exists(true)
            .with_latest_version("1.2.3")
            .with_state(Some(RuntimeImageState::new(
                runtime,
                "1.2.3".to_string(),
                10,
                9,
            )))
            .with_now(42);

        let outcome = update_default_runtime_image_with(runtime, &mut environment).unwrap();

        assert_eq!(
            outcome,
            RuntimeImageUpdateOutcome::AlreadyUpToDate {
                image: runtime.default_image(),
                version: "1.2.3".to_string(),
            }
        );
        assert!(environment.builds.is_empty());
        assert_eq!(environment.writes.len(), 1);
        let (_, state) = &environment.writes[0];
        assert_eq!(
            state,
            &RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 9)
                .with_latest_check("1.2.3".to_string(), 42)
        );
    }

    #[test]
    fn update_runtime_image_rebuilds_missing_image_and_records_state() {
        let runtime = RuntimeKind::Opencode;
        let mut environment = TestRuntimeImageEnvironment::new()
            .with_image_exists(false)
            .with_latest_version("2.0.0")
            .with_state(Some(RuntimeImageState::new(
                runtime,
                "2.0.0".to_string(),
                10,
                9,
            )))
            .with_now(55);

        let outcome = update_default_runtime_image_with(runtime, &mut environment).unwrap();

        assert_eq!(
            outcome,
            RuntimeImageUpdateOutcome::Rebuilt {
                image: runtime.default_image(),
                version: "2.0.0".to_string(),
            }
        );
        assert_eq!(environment.builds, vec![(runtime, "2.0.0".to_string())]);
        assert_eq!(environment.writes.len(), 1);
        let (_, state) = &environment.writes[0];
        assert_eq!(
            state,
            &RuntimeImageState::new(runtime, "2.0.0".to_string(), 55, 55)
        );
    }

    #[test]
    fn ensure_runtime_image_uses_existing_state_without_resolving_or_building() {
        let runtime = RuntimeKind::Codex;
        let mut environment = TestRuntimeImageEnvironment::new()
            .with_image_exists(true)
            .with_state(Some(RuntimeImageState::new(
                runtime,
                "1.2.3".to_string(),
                10,
                9,
            )));
        let mut phases = Vec::new();

        let version = ensure_default_runtime_image_with(
            &mut environment,
            runtime,
            Utf8Path::new("/workspace/demo"),
            |phase| phases.push(phase),
            &runtime.default_image(),
        )
        .unwrap();

        assert_eq!(version, Some("1.2.3".to_string()));
        assert_eq!(
            phases,
            vec![format!("using runtime image `{}`", runtime.default_image())]
        );
        assert!(environment.resolved_packages.is_empty());
        assert!(environment.builds.is_empty());
        assert!(environment.writes.is_empty());
    }

    #[test]
    fn ensure_runtime_image_builds_missing_image_and_records_state() {
        let runtime = RuntimeKind::Codex;
        let mut environment = TestRuntimeImageEnvironment::new()
            .with_image_exists(false)
            .with_latest_version("1.2.4")
            .with_now(88);
        let mut phases = Vec::new();

        let version = ensure_default_runtime_image_with(
            &mut environment,
            runtime,
            Utf8Path::new("/workspace/demo"),
            |phase| phases.push(phase),
            &runtime.default_image(),
        )
        .unwrap();

        assert_eq!(version, Some("1.2.4".to_string()));
        assert_eq!(
            phases,
            vec![format!(
                "building runtime image `{}`",
                runtime.default_image()
            )]
        );
        assert_eq!(environment.builds, vec![(runtime, "1.2.4".to_string())]);
        assert_eq!(environment.writes.len(), 1);
    }

    #[derive(Debug, Default)]
    struct TestRuntimeImageEnvironment {
        image_exists: bool,
        state: Option<RuntimeImageState>,
        latest_versions: Vec<String>,
        now_values: Vec<u64>,
        resolved_packages: Vec<String>,
        builds: Vec<(RuntimeKind, String)>,
        writes: Vec<(RuntimeKind, RuntimeImageState)>,
    }

    impl TestRuntimeImageEnvironment {
        fn new() -> Self {
            Self::default()
        }

        fn with_image_exists(mut self, image_exists: bool) -> Self {
            self.image_exists = image_exists;
            self
        }

        fn with_state(mut self, state: Option<RuntimeImageState>) -> Self {
            self.state = state;
            self
        }

        fn with_latest_version(mut self, version: impl Into<String>) -> Self {
            self.latest_versions.push(version.into());
            self
        }

        fn with_now(mut self, value: u64) -> Self {
            self.now_values.push(value);
            self
        }
    }

    impl RuntimeImageEnvironment for TestRuntimeImageEnvironment {
        fn image_exists(&mut self, _image: &str) -> Result<bool> {
            Ok(self.image_exists)
        }

        fn read_state(&mut self, _runtime: RuntimeKind) -> Result<Option<RuntimeImageState>> {
            Ok(self.state.clone())
        }

        fn write_state(&mut self, runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()> {
            self.writes.push((runtime, state.clone()));
            self.state = Some(state.clone());
            Ok(())
        }

        fn resolve_latest_version(&mut self, package: &str) -> Result<String> {
            self.resolved_packages.push(package.to_string());
            Ok(self.latest_versions.remove(0))
        }

        fn build_image(&mut self, runtime: RuntimeKind, version: &str) -> Result<()> {
            self.builds.push((runtime, version.to_string()));
            Ok(())
        }

        fn now_unix_seconds(&mut self) -> Result<u64> {
            Ok(self.now_values.remove(0))
        }
    }
}

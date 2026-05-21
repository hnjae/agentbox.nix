// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

use super::image_environment::{ProductionRuntimeImageEnvironment, RuntimeImageEnvironment};
use super::image_plan::{RuntimeImageUpdatePlan, plan_runtime_image_update};
use super::image_state::{RuntimeImageState, RuntimeImageStateStore};

pub(super) fn update_default_runtime_image(runtime: RuntimeKind, verbose: bool) -> Result<()> {
    let podman = Podman::new().with_verbose(verbose);
    let mut environment = ProductionRuntimeImageEnvironment::new(&podman)?;

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
    let mut environment = ProductionRuntimeImageEnvironment::new(podman)?;
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
    RuntimeImageStateStore::from_xdg()?.remove_if_image(runtime, image)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeImageUpdateOutcome {
    AlreadyUpToDate { image: String, version: String },
    Rebuilt { image: String, version: String },
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

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    use super::*;

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

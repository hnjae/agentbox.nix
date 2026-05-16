// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;
use crate::cli::DevEnvMode;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::metadata::runtime_package_version_label;
use crate::preflight::{PreflightReport, check_host_prerequisites_for_runtime};
use crate::runtime::{RuntimeInvocation, RuntimeKind, RuntimeRunMode, RuntimeRunSpec};
use crate::session::{existing_session_error, select_single_session};
use crate::ssh_signing::apply_ssh_commit_signing_passthrough;

use super::runtime::ensure_default_runtime_image;
use super::runtime_command::{ensure_host_runtime_client_available, server_runtime_command};
use super::workspace_flow::LockedWorkspace;

#[derive(Debug)]
struct ContainerLaunchPreparation {
    preflight: PreflightReport,
    dev_env: DevEnvironment,
    runtime_image_version: Option<String>,
}

#[derive(Debug)]
pub(super) struct RuntimeLaunchPreparation {
    pub(super) run_spec: RuntimeRunSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostClientRequirement {
    Required,
    NotRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerLaunchMode {
    ManagedSession,
    TransientServer,
}

impl ServerLaunchMode {
    fn runtime_run_mode(self) -> RuntimeRunMode {
        match self {
            Self::ManagedSession => RuntimeRunMode::ManagedSession,
            Self::TransientServer => RuntimeRunMode::TransientServer,
        }
    }

    fn host_client_requirement(self) -> HostClientRequirement {
        match self {
            Self::ManagedSession => HostClientRequirement::NotRequired,
            Self::TransientServer => HostClientRequirement::Required,
        }
    }

    fn records_runtime_image_version(self) -> bool {
        matches!(self, Self::ManagedSession)
    }
}

pub(super) fn prepare_server_launch(
    locked: &LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    mode: ServerLaunchMode,
) -> Result<RuntimeLaunchPreparation> {
    let workspace = locked.workspace();
    let preparation = prepare_container_launch(
        locked,
        runtime,
        dev_env_mode,
        mode.host_client_requirement(),
    )?;
    let server_run = server_runtime_command(
        runtime,
        workspace.canonical_target.as_ref(),
        &preparation.dev_env,
    );
    let mut run_spec = runtime.run_spec(
        mode.runtime_run_mode(),
        workspace,
        &preparation.preflight.host_nix_mounts,
        &preparation.preflight.runtime_mounts,
        server_run,
    );
    apply_ssh_commit_signing_passthrough(&mut run_spec, workspace.canonical_git_root.as_ref());
    record_runtime_image_version(runtime, &preparation, &mut run_spec, mode);

    Ok(RuntimeLaunchPreparation { run_spec })
}

pub(super) fn prepare_foreground_launch(
    locked: &LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    invocation: impl FnOnce(&DevEnvironment) -> RuntimeInvocation,
) -> Result<RuntimeLaunchPreparation> {
    let workspace = locked.workspace();
    let preparation = prepare_container_launch(
        locked,
        runtime,
        dev_env_mode,
        HostClientRequirement::NotRequired,
    )?;
    let mut run_spec = runtime.run_spec(
        RuntimeRunMode::Foreground,
        workspace,
        &preparation.preflight.host_nix_mounts,
        &preparation.preflight.runtime_mounts,
        invocation(&preparation.dev_env),
    );
    apply_ssh_commit_signing_passthrough(&mut run_spec, workspace.canonical_git_root.as_ref());

    Ok(RuntimeLaunchPreparation { run_spec })
}

fn prepare_container_launch(
    locked: &LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    host_client: HostClientRequirement,
) -> Result<ContainerLaunchPreparation> {
    let workspace = locked.workspace();

    diagnostic::info("checking workspace prerequisites");
    let preflight = check_host_prerequisites_for_runtime(runtime)?;

    diagnostic::info("checking existing managed sessions");
    let podman = locked.podman();
    let sessions = locked.discover_sessions()?;
    if let Some(session) = select_single_session(&sessions, workspace)? {
        return Err(existing_session_error(podman, workspace, session));
    }

    let dev_env = DevEnvironment::resolve(
        dev_env_mode,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    )?;
    diagnostic::info(format!("selected development environment: {dev_env}"));

    if host_client == HostClientRequirement::Required {
        ensure_host_runtime_client_available(runtime)?;
    }

    let runtime_image_version = ensure_default_runtime_image(
        podman,
        runtime,
        workspace.canonical_git_root.as_ref(),
        diagnostic::info,
    )?;

    Ok(ContainerLaunchPreparation {
        preflight,
        dev_env,
        runtime_image_version,
    })
}

fn record_runtime_image_version(
    runtime: RuntimeKind,
    preparation: &ContainerLaunchPreparation,
    run_spec: &mut RuntimeRunSpec,
    mode: ServerLaunchMode,
) {
    if !mode.records_runtime_image_version() {
        return;
    }

    if let Some(version) = &preparation.runtime_image_version {
        run_spec
            .create_mut()
            .labels
            .insert(runtime_package_version_label(runtime), version.clone());
    }
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::dev_env::DevEnvMode;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::metadata::runtime_package_version_label;
use crate::preflight::{PreflightReport, check_host_prerequisites_for_runtime};
use crate::runtime::{RuntimeKind, RuntimeRunSpec};
use crate::ssh_signing::{SshPassthroughGuard, apply_ssh_passthrough};
use crate::workspace::WorkspaceIdentity;

use super::runtime::ensure_default_runtime_image;
use super::runtime_command::ensure_host_runtime_client_available;

mod existing_resources;
mod policy;
mod request;

use existing_resources::ensure_required_resources_absent;
use policy::{ExistingResourceCheck, HostClientRequirement, RuntimeLaunchPolicy};
use request::{RuntimeLaunchRequest, build_runtime_invocation};
pub(super) use request::{
    foreground_launch_request, managed_server_launch_request, replacement_server_launch_request,
    transient_server_launch_request,
};

#[derive(Debug)]
struct ContainerLaunchPreparation {
    preflight: PreflightReport,
    dev_env: DevEnvironment,
    runtime_image_version: Option<String>,
}

#[derive(Debug)]
pub(super) struct RuntimeLaunchPreparation {
    pub(super) run_spec: RuntimeRunSpec,
    _ssh_passthrough: SshPassthroughGuard,
}

pub(super) fn prepare_runtime_launch(
    request: RuntimeLaunchRequest<'_>,
) -> Result<RuntimeLaunchPreparation> {
    let RuntimeLaunchRequest {
        podman,
        workspace,
        runtime,
        dev_env_mode,
        policy,
        invocation,
    } = request;
    let preparation = prepare_container_launch_for_workspace(
        podman,
        workspace,
        runtime,
        dev_env_mode,
        policy.host_client,
        policy.existing_check,
    )?;
    let mut run_spec = runtime.run_spec(
        policy.run_mode,
        workspace,
        &preparation.preflight.host_nix_mounts,
        &preparation.preflight.runtime_mounts,
        build_runtime_invocation(invocation, runtime, workspace, &preparation.dev_env),
    );
    let ssh_passthrough =
        apply_ssh_passthrough(&mut run_spec, workspace.canonical_git_root.as_ref());
    record_runtime_image_version(runtime, &preparation, &mut run_spec, policy);

    Ok(RuntimeLaunchPreparation {
        run_spec,
        _ssh_passthrough: ssh_passthrough,
    })
}

fn prepare_container_launch_for_workspace(
    podman: &crate::podman::Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    host_client: HostClientRequirement,
    existing_check: ExistingResourceCheck,
) -> Result<ContainerLaunchPreparation> {
    diagnostic::info("checking workspace prerequisites");
    let preflight = check_host_prerequisites_for_runtime(runtime)?;

    ensure_required_resources_absent(podman, workspace, existing_check)?;

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
    policy: RuntimeLaunchPolicy,
) {
    if !policy.record_runtime_image_version {
        return;
    }

    if let Some(version) = &preparation.runtime_image_version {
        run_spec
            .create_mut()
            .labels
            .insert(runtime_package_version_label(runtime), version.clone());
    }
}

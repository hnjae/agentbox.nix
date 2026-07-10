// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::dev_env::DevEnvMode;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::metadata::{
    LABEL_RESOURCE_LIMIT_CPUS, LABEL_RESOURCE_LIMIT_MEMORY, runtime_package_version_label,
};
use crate::preflight::{PreflightReport, check_host_prerequisites_for_runtime_mode};
use crate::runtime::{RuntimeKind, RuntimeRunSpec};
use crate::ssh_signing::{SshPassthroughGuard, apply_git_and_ssh_passthrough};
use crate::workspace::WorkspaceIdentity;

use super::codex_attach_auth::{CodexAttachToken, prepare_codex_attach_token};
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
    pub(super) codex_attach_token: Option<CodexAttachToken>,
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
        git_identity,
        invocation,
        server_args,
        resource_limits,
    } = request;
    let preparation = prepare_container_launch_for_workspace(
        podman,
        workspace,
        runtime,
        dev_env_mode,
        policy.run_mode,
        policy.host_client,
        policy.existing_check,
    )?;
    let codex_attach_token = prepare_codex_attach_token(runtime, policy.run_mode, workspace)?;
    let mut run_spec = runtime.run_spec(
        policy.run_mode,
        workspace,
        &preparation.preflight.host_nix_mounts,
        &preparation.preflight.runtime_mounts,
        build_runtime_invocation(
            invocation,
            runtime,
            workspace,
            &preparation.dev_env,
            codex_attach_token.as_ref(),
            &server_args,
        )?,
        &server_args,
        resource_limits.clone(),
    );
    run_spec.extend_create_default_env(preparation.preflight.runtime_environment.clone());
    let ssh_passthrough = apply_git_and_ssh_passthrough(
        &mut run_spec,
        workspace.canonical_git_root.as_ref(),
        git_identity,
    );
    record_runtime_image_version(runtime, &preparation, &mut run_spec, policy);
    record_managed_resource_limits(&mut run_spec, policy);

    Ok(RuntimeLaunchPreparation {
        run_spec,
        codex_attach_token,
        _ssh_passthrough: ssh_passthrough,
    })
}

fn record_managed_resource_limits(run_spec: &mut RuntimeRunSpec, policy: RuntimeLaunchPolicy) {
    if policy.run_mode != crate::runtime::RuntimeRunMode::ManagedSession {
        return;
    }

    let labels = run_spec.create().resource_limits().stored_or_zero();
    run_spec.insert_create_label(LABEL_RESOURCE_LIMIT_CPUS, labels.cpus);
    run_spec.insert_create_label(LABEL_RESOURCE_LIMIT_MEMORY, labels.memory);
}

fn prepare_container_launch_for_workspace(
    podman: &crate::podman::Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    run_mode: crate::runtime::RuntimeRunMode,
    host_client: HostClientRequirement,
    existing_check: ExistingResourceCheck,
) -> Result<ContainerLaunchPreparation> {
    diagnostic::info("checking workspace prerequisites");
    let preflight = check_host_prerequisites_for_runtime_mode(runtime, run_mode)?;

    ensure_required_resources_absent(podman, workspace, existing_check)?;

    let dev_env = DevEnvironment::resolve(
        dev_env_mode,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    )?;
    diagnostic::info_rendered(|color| {
        let dev_env = dev_env
            .display_with_provider_style(|provider| diagnostic::bold_bright_cyan(provider, color));
        format!("selected development environment: {dev_env}")
    });

    if host_client == HostClientRequirement::Required {
        ensure_host_runtime_client_available(runtime)?;
    }

    let runtime_image_version =
        ensure_default_runtime_image(podman, runtime, workspace.canonical_git_root.as_ref())?;

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
        run_spec.insert_create_label(runtime_package_version_label(runtime), version.clone());
    }
}

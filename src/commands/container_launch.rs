// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::cli::DevEnvMode;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::metadata::runtime_package_version_label;
use crate::preflight::{PreflightReport, check_host_prerequisites_for_runtime};
use crate::runtime::{RuntimeInvocation, RuntimeKind, RuntimeRunMode, RuntimeRunSpec};
use crate::session::{
    duplicate_agentbox_containers_error, existing_session_error, select_single_session,
};
use crate::ssh_signing::apply_ssh_commit_signing_passthrough;
use crate::workspace::WorkspaceIdentity;

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
enum ExistingResourceScope {
    ManagedSessions,
    AgentboxContainers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingResourceCheck {
    RequireAbsent(ExistingResourceScope),
    AllowExisting,
}

pub(super) struct RuntimeLaunchRequest<'a, I> {
    podman: &'a crate::podman::Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    kind: RuntimeLaunchKind,
    invocation: RuntimeLaunchInvocation<I>,
}

enum RuntimeLaunchInvocation<I> {
    Server,
    Custom(I),
}

impl ExistingResourceScope {
    fn diagnostic_message(self) -> &'static str {
        match self {
            Self::ManagedSessions => "checking existing managed sessions",
            Self::AgentboxContainers => "checking existing agentbox containers",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeLaunchKind {
    ManagedServer,
    TransientServer,
    Foreground,
    ReplacementServer { connect_after_start: bool },
}

impl RuntimeLaunchKind {
    fn runtime_run_mode(self) -> RuntimeRunMode {
        match self {
            Self::ManagedServer | Self::ReplacementServer { .. } => RuntimeRunMode::ManagedSession,
            Self::TransientServer => RuntimeRunMode::TransientServer,
            Self::Foreground => RuntimeRunMode::Foreground,
        }
    }

    fn host_client_requirement(self) -> HostClientRequirement {
        match self {
            Self::ManagedServer | Self::Foreground => HostClientRequirement::NotRequired,
            Self::TransientServer => HostClientRequirement::Required,
            Self::ReplacementServer {
                connect_after_start: true,
            } => HostClientRequirement::Required,
            Self::ReplacementServer {
                connect_after_start: false,
            } => HostClientRequirement::NotRequired,
        }
    }

    fn existing_resource_check(self) -> ExistingResourceCheck {
        match self {
            Self::ManagedServer | Self::TransientServer => {
                ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
            }
            Self::Foreground => {
                ExistingResourceCheck::RequireAbsent(ExistingResourceScope::ManagedSessions)
            }
            Self::ReplacementServer { .. } => ExistingResourceCheck::AllowExisting,
        }
    }

    fn records_runtime_image_version(self) -> bool {
        matches!(self, Self::ManagedServer | Self::ReplacementServer { .. })
    }
}

pub(super) fn managed_server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
) -> RuntimeLaunchRequest<'a, fn(&DevEnvironment) -> RuntimeInvocation> {
    server_launch_request(
        locked,
        runtime,
        dev_env_mode,
        RuntimeLaunchKind::ManagedServer,
    )
}

pub(super) fn transient_server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
) -> RuntimeLaunchRequest<'a, fn(&DevEnvironment) -> RuntimeInvocation> {
    server_launch_request(
        locked,
        runtime,
        dev_env_mode,
        RuntimeLaunchKind::TransientServer,
    )
}

fn server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    kind: RuntimeLaunchKind,
) -> RuntimeLaunchRequest<'a, fn(&DevEnvironment) -> RuntimeInvocation> {
    let workspace = locked.workspace();
    RuntimeLaunchRequest {
        podman: locked.podman(),
        workspace,
        runtime,
        dev_env_mode,
        kind,
        invocation: RuntimeLaunchInvocation::Server,
    }
}

pub(super) fn foreground_launch_request<'a, I>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    invocation: I,
) -> RuntimeLaunchRequest<'a, I>
where
    I: FnOnce(&DevEnvironment) -> RuntimeInvocation + 'a,
{
    RuntimeLaunchRequest {
        podman: locked.podman(),
        workspace: locked.workspace(),
        runtime,
        dev_env_mode,
        kind: RuntimeLaunchKind::Foreground,
        invocation: RuntimeLaunchInvocation::Custom(invocation),
    }
}

pub(super) fn replacement_server_launch_request<'a>(
    podman: &'a crate::podman::Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    connect_after_start: bool,
) -> RuntimeLaunchRequest<'a, fn(&DevEnvironment) -> RuntimeInvocation> {
    RuntimeLaunchRequest {
        podman,
        workspace,
        runtime,
        dev_env_mode,
        kind: RuntimeLaunchKind::ReplacementServer {
            connect_after_start,
        },
        invocation: RuntimeLaunchInvocation::Server,
    }
}

pub(super) fn prepare_runtime_launch<I>(
    request: RuntimeLaunchRequest<'_, I>,
) -> Result<RuntimeLaunchPreparation>
where
    I: FnOnce(&DevEnvironment) -> RuntimeInvocation,
{
    let RuntimeLaunchRequest {
        podman,
        workspace,
        runtime,
        dev_env_mode,
        kind,
        invocation,
    } = request;
    let preparation = prepare_container_launch_for_workspace(
        podman,
        workspace,
        runtime,
        dev_env_mode,
        kind.host_client_requirement(),
        kind.existing_resource_check(),
    )?;
    let mut run_spec = runtime.run_spec(
        kind.runtime_run_mode(),
        workspace,
        &preparation.preflight.host_nix_mounts,
        &preparation.preflight.runtime_mounts,
        build_runtime_invocation(invocation, runtime, workspace, &preparation.dev_env),
    );
    apply_ssh_commit_signing_passthrough(&mut run_spec, workspace.canonical_git_root.as_ref());
    record_runtime_image_version(runtime, &preparation, &mut run_spec, kind);

    Ok(RuntimeLaunchPreparation { run_spec })
}

fn build_runtime_invocation<I>(
    invocation: RuntimeLaunchInvocation<I>,
    runtime: RuntimeKind,
    workspace: &WorkspaceIdentity,
    dev_env: &DevEnvironment,
) -> RuntimeInvocation
where
    I: FnOnce(&DevEnvironment) -> RuntimeInvocation,
{
    match invocation {
        RuntimeLaunchInvocation::Server => {
            server_runtime_command(runtime, workspace.canonical_target.as_ref(), dev_env)
        }
        RuntimeLaunchInvocation::Custom(invocation) => invocation(dev_env),
    }
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

    if let ExistingResourceCheck::RequireAbsent(existing_scope) = existing_check {
        diagnostic::info(existing_scope.diagnostic_message());
        let sessions = match existing_scope {
            ExistingResourceScope::ManagedSessions => {
                crate::session::discover_managed_sessions_for_git_root(
                    podman,
                    workspace.canonical_git_root.as_ref(),
                )?
            }
            ExistingResourceScope::AgentboxContainers => {
                crate::session::discover_sessions_for_git_root(
                    podman,
                    workspace.canonical_git_root.as_ref(),
                )?
            }
        };
        let existing = match sessions.as_slice() {
            [] => None,
            [session] => Some(session),
            _ if existing_scope == ExistingResourceScope::AgentboxContainers => {
                return Err(duplicate_agentbox_containers_error(workspace));
            }
            _ => select_single_session(&sessions, workspace)?,
        };
        if let Some(session) = existing {
            return Err(existing_session_error(podman, workspace, session));
        }
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
    kind: RuntimeLaunchKind,
) {
    if !kind.records_runtime_image_version() {
        return;
    }

    if let Some(version) = &preparation.runtime_image_version {
        run_spec
            .create_mut()
            .labels
            .insert(runtime_package_version_label(runtime), version.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_kind_selects_runtime_run_mode() {
        assert_eq!(
            RuntimeLaunchKind::ManagedServer.runtime_run_mode(),
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchKind::ReplacementServer {
                connect_after_start: false
            }
            .runtime_run_mode(),
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchKind::TransientServer.runtime_run_mode(),
            RuntimeRunMode::TransientServer
        );
        assert_eq!(
            RuntimeLaunchKind::Foreground.runtime_run_mode(),
            RuntimeRunMode::Foreground
        );
    }

    #[test]
    fn launch_kind_selects_host_client_requirement() {
        assert_eq!(
            RuntimeLaunchKind::ManagedServer.host_client_requirement(),
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchKind::TransientServer.host_client_requirement(),
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchKind::Foreground.host_client_requirement(),
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchKind::ReplacementServer {
                connect_after_start: true
            }
            .host_client_requirement(),
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchKind::ReplacementServer {
                connect_after_start: false
            }
            .host_client_requirement(),
            HostClientRequirement::NotRequired
        );
    }

    #[test]
    fn launch_kind_selects_existing_resource_check() {
        assert_eq!(
            RuntimeLaunchKind::ManagedServer.existing_resource_check(),
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchKind::TransientServer.existing_resource_check(),
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchKind::Foreground.existing_resource_check(),
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::ManagedSessions)
        );
        assert_eq!(
            RuntimeLaunchKind::ReplacementServer {
                connect_after_start: false
            }
            .existing_resource_check(),
            ExistingResourceCheck::AllowExisting
        );
    }

    #[test]
    fn launch_kind_records_runtime_image_versions_only_for_managed_lifetimes() {
        assert!(RuntimeLaunchKind::ManagedServer.records_runtime_image_version());
        assert!(
            RuntimeLaunchKind::ReplacementServer {
                connect_after_start: false
            }
            .records_runtime_image_version()
        );
        assert!(!RuntimeLaunchKind::TransientServer.records_runtime_image_version());
        assert!(!RuntimeLaunchKind::Foreground.records_runtime_image_version());
    }
}

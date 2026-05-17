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
    SessionDiscoveryQuery, duplicate_agentbox_containers_error, existing_session_error,
    select_single_session,
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

type CustomRuntimeInvocation<'a> = Box<dyn FnOnce(&DevEnvironment) -> RuntimeInvocation + 'a>;

pub(super) struct RuntimeLaunchRequest<'a> {
    podman: &'a crate::podman::Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    policy: RuntimeLaunchPolicy,
    invocation: RuntimeLaunchInvocation<'a>,
}

enum RuntimeLaunchInvocation<'a> {
    Server,
    Custom(CustomRuntimeInvocation<'a>),
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
struct RuntimeLaunchPolicy {
    run_mode: RuntimeRunMode,
    host_client: HostClientRequirement,
    existing_check: ExistingResourceCheck,
    record_runtime_image_version: bool,
}

impl RuntimeLaunchPolicy {
    fn managed_server(connect_after_start: bool) -> Self {
        Self {
            run_mode: RuntimeRunMode::ManagedSession,
            host_client: if connect_after_start {
                HostClientRequirement::Required
            } else {
                HostClientRequirement::NotRequired
            },
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::AgentboxContainers,
            ),
            record_runtime_image_version: true,
        }
    }

    fn transient_server() -> Self {
        Self {
            run_mode: RuntimeRunMode::TransientServer,
            host_client: HostClientRequirement::Required,
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::AgentboxContainers,
            ),
            record_runtime_image_version: false,
        }
    }

    fn foreground() -> Self {
        Self {
            run_mode: RuntimeRunMode::Foreground,
            host_client: HostClientRequirement::NotRequired,
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::ManagedSessions,
            ),
            record_runtime_image_version: false,
        }
    }

    fn replacement_server(connect_after_start: bool) -> Self {
        Self {
            run_mode: RuntimeRunMode::ManagedSession,
            host_client: if connect_after_start {
                HostClientRequirement::Required
            } else {
                HostClientRequirement::NotRequired
            },
            existing_check: ExistingResourceCheck::AllowExisting,
            record_runtime_image_version: true,
        }
    }
}

pub(super) fn managed_server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    connect_after_start: bool,
) -> RuntimeLaunchRequest<'a> {
    server_launch_request(
        locked,
        runtime,
        dev_env_mode,
        RuntimeLaunchPolicy::managed_server(connect_after_start),
    )
}

pub(super) fn transient_server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
) -> RuntimeLaunchRequest<'a> {
    server_launch_request(
        locked,
        runtime,
        dev_env_mode,
        RuntimeLaunchPolicy::transient_server(),
    )
}

fn server_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    policy: RuntimeLaunchPolicy,
) -> RuntimeLaunchRequest<'a> {
    let workspace = locked.workspace();
    RuntimeLaunchRequest {
        podman: locked.podman(),
        workspace,
        runtime,
        dev_env_mode,
        policy,
        invocation: RuntimeLaunchInvocation::Server,
    }
}

pub(super) fn foreground_launch_request<'a>(
    locked: &'a LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    invocation: impl FnOnce(&DevEnvironment) -> RuntimeInvocation + 'a,
) -> RuntimeLaunchRequest<'a> {
    RuntimeLaunchRequest {
        podman: locked.podman(),
        workspace: locked.workspace(),
        runtime,
        dev_env_mode,
        policy: RuntimeLaunchPolicy::foreground(),
        invocation: RuntimeLaunchInvocation::Custom(Box::new(invocation)),
    }
}

pub(super) fn replacement_server_launch_request<'a>(
    podman: &'a crate::podman::Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    connect_after_start: bool,
) -> RuntimeLaunchRequest<'a> {
    RuntimeLaunchRequest {
        podman,
        workspace,
        runtime,
        dev_env_mode,
        policy: RuntimeLaunchPolicy::replacement_server(connect_after_start),
        invocation: RuntimeLaunchInvocation::Server,
    }
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
    apply_ssh_commit_signing_passthrough(&mut run_spec, workspace.canonical_git_root.as_ref());
    record_runtime_image_version(runtime, &preparation, &mut run_spec, policy);

    Ok(RuntimeLaunchPreparation { run_spec })
}

fn build_runtime_invocation(
    invocation: RuntimeLaunchInvocation<'_>,
    runtime: RuntimeKind,
    workspace: &WorkspaceIdentity,
    dev_env: &DevEnvironment,
) -> RuntimeInvocation {
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
            ExistingResourceScope::ManagedSessions => SessionDiscoveryQuery::managed_sessions()
                .for_git_root(workspace.canonical_git_root.as_ref())
                .discover(podman)?,
            ExistingResourceScope::AgentboxContainers => {
                SessionDiscoveryQuery::agentbox_containers()
                    .for_git_root(workspace.canonical_git_root.as_ref())
                    .discover(podman)?
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

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    use super::*;

    #[test]
    fn custom_launch_invocation_receives_resolved_dev_environment() {
        let workspace = workspace();
        let invocation = RuntimeLaunchInvocation::Custom(Box::new(|dev_env| {
            RuntimeInvocation::new(
                dev_env.wrap_argv(vec!["codex".to_string(), "exec".to_string()]),
                "/workspace/demo",
            )
        }));

        let runtime_invocation = build_runtime_invocation(
            invocation,
            RuntimeKind::Codex,
            &workspace,
            &DevEnvironment::Direnv,
        );

        assert_eq!(
            runtime_invocation.argv(),
            ["direnv", "exec", ".", "codex", "exec"]
        );
        assert_eq!(
            runtime_invocation.workdir(),
            Utf8Path::new("/workspace/demo")
        );
    }

    #[test]
    fn launch_policies_select_runtime_run_mode() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).run_mode,
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).run_mode,
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().run_mode,
            RuntimeRunMode::TransientServer
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().run_mode,
            RuntimeRunMode::Foreground
        );
    }

    #[test]
    fn launch_policies_select_host_client_requirement() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).host_client,
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(true).host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().host_client,
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(true).host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).host_client,
            HostClientRequirement::NotRequired
        );
    }

    #[test]
    fn launch_policies_select_existing_resource_check() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(true).existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::ManagedSessions)
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).existing_check,
            ExistingResourceCheck::AllowExisting
        );
    }

    #[test]
    fn launch_policies_record_runtime_image_versions_only_for_managed_lifetimes() {
        assert!(RuntimeLaunchPolicy::managed_server(false).record_runtime_image_version);
        assert!(RuntimeLaunchPolicy::managed_server(true).record_runtime_image_version);
        assert!(RuntimeLaunchPolicy::replacement_server(false).record_runtime_image_version);
        assert!(!RuntimeLaunchPolicy::transient_server().record_runtime_image_version);
        assert!(!RuntimeLaunchPolicy::foreground().record_runtime_image_version);
    }

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentity {
            requested_target: "/workspace/demo".into(),
            absolute_target: "/workspace/demo".into(),
            canonical_target: "/workspace/demo".into(),
            canonical_git_root: "/workspace/demo".into(),
            digest64: "0123456789abcdef".to_string(),
            hash12: "0123456789ab".to_string(),
            container_name: "agentbox-demo".to_string(),
        }
    }
}

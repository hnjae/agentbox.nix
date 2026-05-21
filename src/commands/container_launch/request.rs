// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::commands::runtime_command::server_runtime_command;
use crate::commands::workspace_flow::LockedWorkspace;
use crate::dev_env::{DevEnvMode, DevEnvironment};
use crate::podman::Podman;
use crate::runtime::{RuntimeInvocation, RuntimeKind};
use crate::ssh_signing::GitIdentityPassthrough;
use crate::workspace::WorkspaceIdentity;

use super::policy::RuntimeLaunchPolicy;

type CustomRuntimeInvocation<'a> = Box<dyn FnOnce(&DevEnvironment) -> RuntimeInvocation + 'a>;

pub(in crate::commands) struct RuntimeLaunchRequest<'a> {
    pub(super) podman: &'a Podman,
    pub(super) workspace: &'a WorkspaceIdentity,
    pub(super) runtime: RuntimeKind,
    pub(super) dev_env_mode: DevEnvMode,
    pub(super) policy: RuntimeLaunchPolicy,
    pub(super) git_identity: GitIdentityPassthrough,
    pub(super) invocation: RuntimeLaunchInvocation<'a>,
}

pub(super) enum RuntimeLaunchInvocation<'a> {
    Server,
    Custom(CustomRuntimeInvocation<'a>),
}

pub(in crate::commands) fn managed_server_launch_request<'a>(
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

pub(in crate::commands) fn transient_server_launch_request<'a>(
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
    RuntimeLaunchRequest {
        podman: locked.podman(),
        workspace: locked.workspace(),
        runtime,
        dev_env_mode,
        policy,
        git_identity: GitIdentityPassthrough::Host,
        invocation: RuntimeLaunchInvocation::Server,
    }
}

pub(in crate::commands) fn foreground_launch_request<'a>(
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
        git_identity: GitIdentityPassthrough::CodexExec,
        invocation: RuntimeLaunchInvocation::Custom(Box::new(invocation)),
    }
}

pub(in crate::commands) fn replacement_server_launch_request<'a>(
    podman: &'a Podman,
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
        git_identity: GitIdentityPassthrough::Host,
        invocation: RuntimeLaunchInvocation::Server,
    }
}

pub(super) fn build_runtime_invocation(
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

#[cfg(test)]
mod tests {
    use camino::Utf8Path;

    use super::*;
    use crate::workspace::test_support::WorkspaceIdentityFixture;

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

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentityFixture::demo().build()
    }
}

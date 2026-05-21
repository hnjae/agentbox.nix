// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunSpec};
use crate::session::classify_create_error_or_else;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::detached_server::{
    DetachedServerContext, DetachedServerLifecycle, launch_detached_server,
};
use super::launch_policy::{CommandInterrupt, ContainerLogContext, error_with_container_logs};
use super::runtime_command::run_host_runtime_client;
use super::server_readiness::ServerEndpointContext;

pub(super) trait ManagedServerLaunchPolicy {
    fn command_name(&self) -> &'static str;

    fn launch_description(&self) -> &'static str;

    fn create_action(&self) -> &'static str;

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()>;

    fn wrap_error(&self, error: Error) -> Error {
        error
    }
}

fn launch_managed_server<P>(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &RuntimeRunSpec,
    policy: P,
) -> Result<AttachEndpoint>
where
    P: ManagedServerLaunchPolicy,
{
    launch_detached_server(
        DetachedServerContext::new(podman, workspace, runtime, run_spec),
        ManagedServerLifecycle { policy },
    )
    .map(|ready| ready.into_endpoint())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ManagedServerLaunch<'a, P> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &'a RuntimeRunSpec,
    policy: P,
    completion: ManagedServerCompletion<'a>,
}

impl<'a, P> ManagedServerLaunch<'a, P>
where
    P: ManagedServerLaunchPolicy,
{
    pub(super) fn new(
        podman: &'a Podman,
        workspace: &'a WorkspaceIdentity,
        runtime: RuntimeKind,
        run_spec: &'a RuntimeRunSpec,
        policy: P,
        completion: ManagedServerCompletion<'a>,
    ) -> Self {
        Self {
            podman,
            workspace,
            runtime,
            run_spec,
            policy,
            completion,
        }
    }

    pub(super) fn execute(self) -> Result<()> {
        let endpoint = launch_managed_server(
            self.podman,
            self.workspace,
            self.runtime,
            self.run_spec,
            self.policy,
        )?;
        finish_managed_server_launch(self.completion, self.workspace, self.runtime, endpoint)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ManagedServerCompletion<'a> {
    kind: ManagedServerCompletionKind,
    connect: bool,
    client_launch_directory: &'a Utf8Path,
    retry_target: &'a Utf8Path,
}

impl<'a> ManagedServerCompletion<'a> {
    pub(super) fn new(
        kind: ManagedServerCompletionKind,
        connect: bool,
        client_launch_directory: &'a Utf8Path,
        retry_target: &'a Utf8Path,
    ) -> Self {
        Self {
            kind,
            connect,
            client_launch_directory,
            retry_target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ManagedServerCompletionKind {
    Start,
    Restart,
}

fn finish_managed_server_launch(
    completion: ManagedServerCompletion<'_>,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    endpoint: AttachEndpoint,
) -> Result<()> {
    if completion.connect {
        crate::diagnostic::info(completion.kind.connecting_message(workspace, &endpoint));
        run_host_runtime_client(runtime, &endpoint, completion.client_launch_directory).map_err(
            |error| {
                Error::msg(completion.kind.connect_error_message(
                    workspace,
                    completion.retry_target,
                    error,
                ))
            },
        )
    } else {
        crate::diagnostic::info(completion.kind.ready_message(
            workspace,
            &endpoint,
            completion.retry_target,
        ));
        Ok(())
    }
}

impl ManagedServerCompletionKind {
    fn connecting_message(
        self,
        workspace: &WorkspaceIdentity,
        endpoint: &AttachEndpoint,
    ) -> String {
        match self {
            Self::Start => format!(
                "managed session `{}` for `{}` is ready at `{endpoint}`; connecting",
                workspace.container_name, workspace.canonical_git_root,
            ),
            Self::Restart => format!(
                "managed session `{}` for `{}` restarted and ready at `{endpoint}`; connecting",
                workspace.container_name, workspace.canonical_git_root,
            ),
        }
    }

    fn ready_message(
        self,
        workspace: &WorkspaceIdentity,
        endpoint: &AttachEndpoint,
        retry_target: &Utf8Path,
    ) -> String {
        match self {
            Self::Start => format!(
                "managed session `{}` for `{}` is ready at `{endpoint}`; use `agentbox connect {retry_target}` to connect",
                workspace.container_name, workspace.canonical_git_root,
            ),
            Self::Restart => format!(
                "managed session `{}` for `{}` restarted and ready at `{endpoint}`; use `agentbox connect {retry_target}` to connect",
                workspace.container_name, workspace.canonical_git_root,
            ),
        }
    }

    fn connect_error_message(
        self,
        workspace: &WorkspaceIdentity,
        retry_target: &Utf8Path,
        error: Error,
    ) -> String {
        match self {
            Self::Start => format!(
                "failed to connect to newly created managed session `{}` for `{}`: {error}. The session remains running; retry with `agentbox connect {}` or stop it with `agentbox stop {}`.",
                workspace.container_name, workspace.canonical_git_root, retry_target, retry_target,
            ),
            Self::Restart => format!(
                "failed to connect to restarted managed session `{}` for `{}`: {error}. The session remains running; retry with `agentbox connect {}` or stop it with `agentbox stop {}`.",
                workspace.container_name, workspace.canonical_git_root, retry_target, retry_target,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ManagedServerLifecycle<P> {
    policy: P,
}

impl<P> DetachedServerLifecycle for ManagedServerLifecycle<P>
where
    P: ManagedServerLaunchPolicy,
{
    fn command_name(&self) -> &'static str {
        self.policy.command_name()
    }

    fn launch_description(&self) -> &'static str {
        self.policy.launch_description()
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::ManagedSession
    }

    fn check_interrupted(
        &self,
        _context: DetachedServerContext<'_>,
        interrupt: &CommandInterrupt,
    ) -> Result<()> {
        self.policy.check_interrupted(interrupt)
    }

    fn run_detached_error(&self, context: DetachedServerContext<'_>, error: Error) -> Error {
        let error = classify_managed_server_create_error(
            context.podman(),
            context.workspace(),
            context.run_spec(),
            self.policy.create_action(),
            error,
        );
        self.policy.wrap_error(error)
    }

    fn readiness_error(&self, context: DetachedServerContext<'_>, error: Error) -> Error {
        let error = error_with_container_logs(
            context.podman(),
            context.workspace(),
            ContainerLogContext::ManagedSession,
            error,
        );
        self.policy.wrap_error(error)
    }
}

fn classify_managed_server_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    run_spec: &RuntimeRunSpec,
    action: &'static str,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        action,
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, run_spec.create(), wrapped, |error| {
        error_with_container_logs(
            podman,
            workspace,
            ContainerLogContext::ManagedSession,
            error,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::test_support::WorkspaceIdentityFixture;

    #[test]
    fn start_completion_messages_preserve_existing_wording() {
        let workspace = workspace();
        let endpoint = endpoint();
        let retry_target = workspace.requested_target.as_ref();

        assert_eq!(
            ManagedServerCompletionKind::Start.connecting_message(&workspace, &endpoint),
            "managed session `agentbox-demo` for `/workspace/demo` is ready at `http://127.0.0.1:4096`; connecting",
        );
        assert_eq!(
            ManagedServerCompletionKind::Start.ready_message(&workspace, &endpoint, retry_target),
            "managed session `agentbox-demo` for `/workspace/demo` is ready at `http://127.0.0.1:4096`; use `agentbox connect /workspace/demo` to connect",
        );
        assert_eq!(
            ManagedServerCompletionKind::Start.connect_error_message(
                &workspace,
                retry_target,
                Error::msg("client failed")
            ),
            "failed to connect to newly created managed session `agentbox-demo` for `/workspace/demo`: client failed. The session remains running; retry with `agentbox connect /workspace/demo` or stop it with `agentbox stop /workspace/demo`.",
        );
    }

    #[test]
    fn restart_completion_messages_preserve_existing_wording() {
        let workspace = workspace();
        let endpoint = endpoint();
        let retry_target = workspace.canonical_target.as_ref();

        assert_eq!(
            ManagedServerCompletionKind::Restart.connecting_message(&workspace, &endpoint),
            "managed session `agentbox-demo` for `/workspace/demo` restarted and ready at `http://127.0.0.1:4096`; connecting",
        );
        assert_eq!(
            ManagedServerCompletionKind::Restart.ready_message(&workspace, &endpoint, retry_target),
            "managed session `agentbox-demo` for `/workspace/demo` restarted and ready at `http://127.0.0.1:4096`; use `agentbox connect /workspace/demo` to connect",
        );
        assert_eq!(
            ManagedServerCompletionKind::Restart.connect_error_message(
                &workspace,
                retry_target,
                Error::msg("client failed")
            ),
            "failed to connect to restarted managed session `agentbox-demo` for `/workspace/demo`: client failed. The session remains running; retry with `agentbox connect /workspace/demo` or stop it with `agentbox stop /workspace/demo`.",
        );
    }

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentityFixture::demo().build()
    }

    fn endpoint() -> AttachEndpoint {
        AttachEndpoint {
            scheme: "http".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: 4096,
        }
    }
}

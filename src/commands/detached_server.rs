// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::diagnostic;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunSpec};
use crate::workspace::WorkspaceIdentity;

use super::launch_policy::CommandInterrupt;
use super::server_readiness::{
    ServerEndpointContext, ServerEndpointWait, wait_for_server_endpoint,
};

pub(super) trait DetachedServerLifecycle {
    fn command_name(&self) -> &'static str;

    fn launch_description(&self) -> &'static str;

    fn readiness_context(&self) -> ServerEndpointContext;

    fn check_interrupted(
        &self,
        context: DetachedServerContext<'_>,
        interrupt: &CommandInterrupt,
    ) -> Result<()>;

    fn run_detached_error(
        &self,
        context: DetachedServerContext<'_>,
        error: crate::Error,
    ) -> crate::Error;

    fn readiness_error(
        &self,
        context: DetachedServerContext<'_>,
        error: crate::Error,
    ) -> crate::Error;
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DetachedServerContext<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &'a RuntimeRunSpec,
}

impl<'a> DetachedServerContext<'a> {
    pub(super) fn new(
        podman: &'a Podman,
        workspace: &'a WorkspaceIdentity,
        runtime: RuntimeKind,
        run_spec: &'a RuntimeRunSpec,
    ) -> Self {
        Self {
            podman,
            workspace,
            runtime,
            run_spec,
        }
    }

    pub(super) fn podman(self) -> &'a Podman {
        self.podman
    }

    pub(super) fn workspace(self) -> &'a WorkspaceIdentity {
        self.workspace
    }

    pub(super) fn runtime(self) -> RuntimeKind {
        self.runtime
    }

    pub(super) fn run_spec(self) -> &'a RuntimeRunSpec {
        self.run_spec
    }
}

pub(super) struct ReadyDetachedServer {
    endpoint: AttachEndpoint,
    _interrupt: CommandInterrupt,
}

impl ReadyDetachedServer {
    pub(super) fn endpoint(&self) -> &AttachEndpoint {
        &self.endpoint
    }

    pub(super) fn into_endpoint(self) -> AttachEndpoint {
        let Self {
            endpoint,
            _interrupt,
        } = self;
        endpoint
    }
}

pub(super) fn launch_detached_server<L>(
    context: DetachedServerContext<'_>,
    lifecycle: L,
) -> Result<ReadyDetachedServer>
where
    L: DetachedServerLifecycle,
{
    let interrupt = CommandInterrupt::install(lifecycle.command_name())?;

    diagnostic::info(format!(
        "starting {} `{}` for `{}`",
        lifecycle.launch_description(),
        context.workspace().container_name,
        context.runtime()
    ));
    if let Err(error) = context
        .podman()
        .run_detached(&context.workspace().container_name, context.run_spec())
    {
        lifecycle.check_interrupted(context, &interrupt)?;
        return Err(lifecycle.run_detached_error(context, error));
    }
    lifecycle.check_interrupted(context, &interrupt)?;

    diagnostic::info(format!(
        "waiting for `{}` runtime server",
        context.runtime()
    ));
    let endpoint = match wait_for_server_endpoint(
        context.podman(),
        context.workspace(),
        context.runtime(),
        lifecycle.readiness_context(),
        || interrupt.interrupted(),
    ) {
        Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
        Ok(ServerEndpointWait::Interrupted) => {
            lifecycle.check_interrupted(context, &interrupt)?;
            return Err(crate::Error::msg(
                "runtime server readiness wait was interrupted",
            ));
        }
        Err(error) => {
            return Err(lifecycle.readiness_error(context, error));
        }
    };
    lifecycle.check_interrupted(context, &interrupt)?;

    Ok(ReadyDetachedServer {
        endpoint,
        _interrupt: interrupt,
    })
}

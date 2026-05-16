// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()>;

    fn run_detached_error(&self, error: crate::Error) -> crate::Error;

    fn readiness_error(&self, error: crate::Error) -> crate::Error;
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
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &RuntimeRunSpec,
    lifecycle: L,
) -> Result<ReadyDetachedServer>
where
    L: DetachedServerLifecycle,
{
    let interrupt = CommandInterrupt::install(lifecycle.command_name())?;

    diagnostic::info(format!(
        "starting {} `{}` for `{}`",
        lifecycle.launch_description(),
        workspace.container_name,
        runtime
    ));
    if let Err(error) = podman.run_detached(&workspace.container_name, run_spec) {
        lifecycle.check_interrupted(&interrupt)?;
        return Err(lifecycle.run_detached_error(error));
    }
    lifecycle.check_interrupted(&interrupt)?;

    diagnostic::info(format!("waiting for `{runtime}` runtime server"));
    let endpoint = match wait_for_server_endpoint(
        podman,
        workspace,
        runtime,
        lifecycle.readiness_context(),
        || interrupt.interrupted(),
    ) {
        Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
        Ok(ServerEndpointWait::Interrupted) => {
            lifecycle.check_interrupted(&interrupt)?;
            return Err(crate::Error::msg(
                "runtime server readiness wait was interrupted",
            ));
        }
        Err(error) => {
            return Err(lifecycle.readiness_error(error));
        }
    };
    lifecycle.check_interrupted(&interrupt)?;

    Ok(ReadyDetachedServer {
        endpoint,
        _interrupt: interrupt,
    })
}

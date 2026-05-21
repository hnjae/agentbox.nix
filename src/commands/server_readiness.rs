// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use crate::Result;
use crate::podman::Podman;
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::workspace::WorkspaceIdentity;

mod endpoint;
#[cfg(test)]
mod test_support;
mod waiter;

pub(super) use endpoint::ServerEndpointContext;
use waiter::ServerEndpointWaiter;

const SERVER_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub(super) fn wait_for_server_endpoint(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    context: ServerEndpointContext,
    interrupted: impl Fn() -> bool,
) -> Result<ServerEndpointWait> {
    ServerEndpointWaiter::production().wait(podman, workspace, runtime, context, interrupted)
}

#[derive(Debug)]
pub(super) enum ServerEndpointWait {
    Ready(AttachEndpoint),
    Interrupted,
}

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::{Duration, Instant};

use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::{AttachEndpoint, HostRuntimeHealthProbe, RuntimeHealthProbe, RuntimeKind};
use crate::session::discover_attach_endpoint_from_inspect;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

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

#[derive(Debug, Clone)]
struct ServerEndpointWaiter<P> {
    probe: P,
    timeout: Duration,
    poll_interval: Duration,
}

impl ServerEndpointWaiter<HostRuntimeHealthProbe> {
    fn production() -> Self {
        Self {
            probe: HostRuntimeHealthProbe,
            timeout: SERVER_READINESS_TIMEOUT,
            poll_interval: SERVER_READINESS_POLL_INTERVAL,
        }
    }
}

impl<P> ServerEndpointWaiter<P>
where
    P: RuntimeHealthProbe,
{
    fn wait(
        &self,
        podman: &Podman,
        workspace: &WorkspaceIdentity,
        runtime: RuntimeKind,
        context: ServerEndpointContext,
        interrupted: impl Fn() -> bool,
    ) -> Result<ServerEndpointWait> {
        let deadline = Instant::now() + self.timeout;
        let mut last_error = None::<String>;

        loop {
            if interrupted() {
                return Ok(ServerEndpointWait::Interrupted);
            }

            if Instant::now() >= deadline {
                let last_error = last_error
                    .as_deref()
                    .unwrap_or("no inspect data was available");
                return Err(Error::msg(format!(
                    "runtime server for {} `{}` in `{}` did not become reachable: {last_error}",
                    context.description(),
                    workspace.container_name,
                    workspace.canonical_git_root,
                )));
            }

            match podman.inspect_one(&workspace.container_name) {
                Ok(inspect) => {
                    match inspect_server_endpoint(
                        workspace,
                        runtime,
                        context,
                        inspect,
                        &self.probe,
                    )? {
                        ServerEndpointState::Ready(endpoint) => {
                            return Ok(ServerEndpointWait::Ready(endpoint));
                        }
                        ServerEndpointState::Pending(error) => last_error = Some(error),
                    }
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                }
            }

            std::thread::sleep(self.poll_interval);
        }
    }
}

pub(super) enum ServerEndpointWait {
    Ready(AttachEndpoint),
    Interrupted,
}

enum ServerEndpointState {
    Ready(AttachEndpoint),
    Pending(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerEndpointContext {
    ManagedSession,
    TransientRunContainer,
}

impl ServerEndpointContext {
    fn description(self) -> &'static str {
        match self {
            Self::ManagedSession => "managed session",
            Self::TransientRunContainer => "transient run container",
        }
    }
}

fn inspect_server_endpoint<P>(
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    context: ServerEndpointContext,
    inspect: PodmanContainerInspect,
    probe: &P,
) -> Result<ServerEndpointState>
where
    P: RuntimeHealthProbe,
{
    if !inspect.state.running {
        return Err(Error::msg(format!(
            "{} `{}` for `{}` exited before the `{}` runtime server became reachable; status: {}, exit code: {}",
            context.description(),
            workspace.container_name,
            workspace.canonical_git_root,
            runtime.as_str(),
            inspect.state.status,
            inspect.state.exit_code,
        )));
    }

    let endpoint = match context {
        ServerEndpointContext::ManagedSession => discover_attach_endpoint_from_inspect(&inspect),
        ServerEndpointContext::TransientRunContainer => {
            discover_attach_endpoint_from_runtime_inspect(runtime, &inspect)
        }
    };

    match endpoint {
        Ok(endpoint) => {
            let health = probe.check(runtime, &endpoint);
            if health.is_healthy() {
                Ok(ServerEndpointState::Ready(endpoint))
            } else {
                tracing::debug!(
                    endpoint = %endpoint,
                    reason = health.reason(),
                    "runtime endpoint probe is not ready"
                );
                Ok(ServerEndpointState::Pending(format!(
                    "endpoint `{endpoint}` is not reachable yet"
                )))
            }
        }
        Err(error) => Ok(ServerEndpointState::Pending(error.to_string())),
    }
}

fn discover_attach_endpoint_from_runtime_inspect(
    runtime: RuntimeKind,
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let attach = runtime.attach_spec();
    let port_key = format!("{}/tcp", attach.container_port);
    let published_port = inspect
        .network_settings
        .published_tcp_host_port(attach.container_port)?
        .ok_or_else(|| {
            Error::msg(format!(
                "transient run container has no published attach port for `{port_key}`"
            ))
        })?;

    Ok(AttachEndpoint {
        scheme: attach.scheme.to_string(),
        host_ip: published_port.host_ip,
        host_port: published_port.host_port,
    })
}

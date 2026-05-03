// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::{AttachEndpoint, RuntimeAdapter};
use crate::session::discover_attach_endpoint_from_inspect;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

const SERVER_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) fn wait_for_server_endpoint(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeAdapter,
) -> Result<AttachEndpoint> {
    ServerEndpointWaiter::production().wait(podman, workspace, runtime)
}

#[derive(Debug, Clone)]
struct ServerEndpointWaiter<P> {
    probe: P,
    timeout: Duration,
    poll_interval: Duration,
}

impl ServerEndpointWaiter<HostTcpEndpointProbe> {
    fn production() -> Self {
        Self {
            probe: HostTcpEndpointProbe,
            timeout: SERVER_READINESS_TIMEOUT,
            poll_interval: SERVER_READINESS_POLL_INTERVAL,
        }
    }
}

impl<P> ServerEndpointWaiter<P>
where
    P: EndpointProbe,
{
    fn wait(
        &self,
        podman: &Podman,
        workspace: &WorkspaceIdentity,
        runtime: RuntimeAdapter,
    ) -> Result<AttachEndpoint> {
        let deadline = Instant::now() + self.timeout;
        let mut last_error = None::<String>;

        loop {
            if Instant::now() >= deadline {
                let last_error = last_error
                    .as_deref()
                    .unwrap_or("no inspect data was available");
                return Err(Error::msg(format!(
                    "runtime server for managed session `{}` in `{}` did not become reachable: {last_error}",
                    workspace.container_name, workspace.canonical_git_root,
                )));
            }

            match podman.inspect_one(&workspace.container_name) {
                Ok(inspect) => {
                    match inspect_server_endpoint(workspace, runtime, inspect, &self.probe)? {
                        ServerEndpointState::Ready(endpoint) => return Ok(endpoint),
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

enum ServerEndpointState {
    Ready(AttachEndpoint),
    Pending(String),
}

fn inspect_server_endpoint<P>(
    workspace: &WorkspaceIdentity,
    runtime: RuntimeAdapter,
    inspect: PodmanContainerInspect,
    probe: &P,
) -> Result<ServerEndpointState>
where
    P: EndpointProbe,
{
    if !inspect.state.running {
        return Err(Error::msg(format!(
            "container `{}` for `{}` exited before the `{}` runtime server became reachable; status: {}, exit code: {}",
            workspace.container_name,
            workspace.canonical_git_root,
            runtime.name(),
            inspect.state.status,
            inspect.state.exit_code,
        )));
    }

    match discover_attach_endpoint_from_inspect(&inspect) {
        Ok(endpoint) if probe.is_reachable(&endpoint) => Ok(ServerEndpointState::Ready(endpoint)),
        Ok(endpoint) => Ok(ServerEndpointState::Pending(format!(
            "endpoint `{endpoint}` is not reachable yet"
        ))),
        Err(error) => Ok(ServerEndpointState::Pending(error.to_string())),
    }
}

trait EndpointProbe {
    fn is_reachable(&self, endpoint: &AttachEndpoint) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct HostTcpEndpointProbe;

impl EndpointProbe for HostTcpEndpointProbe {
    fn is_reachable(&self, endpoint: &AttachEndpoint) -> bool {
        if std::env::var_os("AGENTBOX_TEST_FIXTURES").is_some() {
            return true;
        }

        let Ok(addresses) = (endpoint.host_ip.as_str(), endpoint.host_port).to_socket_addrs()
        else {
            return false;
        };

        addresses
            .into_iter()
            .any(|address| TcpStream::connect_timeout(&address, ENDPOINT_CONNECT_TIMEOUT).is_ok())
    }
}

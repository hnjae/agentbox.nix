// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::session::discover_attach_endpoint_from_inspect;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

const SERVER_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) fn wait_for_server_endpoint(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
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
        runtime: RuntimeKind,
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
    runtime: RuntimeKind,
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
            runtime.as_str(),
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
        let Ok(addresses) = (endpoint.host_ip.as_str(), endpoint.host_port).to_socket_addrs()
        else {
            return false;
        };

        addresses.into_iter().any(|address| {
            TcpStream::connect_timeout(&address, ENDPOINT_CONNECT_TIMEOUT)
                .is_ok_and(|mut stream| endpoint_connection_is_ready(endpoint, &mut stream))
        })
    }
}

fn endpoint_connection_is_ready(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> bool {
    match endpoint.scheme.as_str() {
        "http" => http_endpoint_is_ready(endpoint, stream),
        "ws" => ws_endpoint_is_ready(endpoint, stream),
        _ => true,
    }
}

fn http_endpoint_is_ready(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> bool {
    http_get_response_prefix(endpoint, stream, "/", 8)
        .is_some_and(|prefix| prefix.starts_with(b"HTTP/1."))
}

fn ws_endpoint_is_ready(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> bool {
    http_get_response_prefix(endpoint, stream, "/readyz", 13).is_some_and(|prefix| {
        prefix.len() >= 13
            && prefix.starts_with(b"HTTP/1.")
            && prefix[8] == b' '
            && &prefix[9..12] == b"200"
            && matches!(prefix[12], b' ' | b'\r')
    })
}

fn http_get_response_prefix(
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
    path: &str,
    prefix_len: usize,
) -> Option<Vec<u8>> {
    let _ = stream.set_read_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        endpoint.host_ip, endpoint.host_port,
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return None;
    }

    let mut response_prefix = Vec::with_capacity(prefix_len);
    while response_prefix.len() < prefix_len {
        let mut buffer = [0_u8; 12];
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => response_prefix.extend_from_slice(&buffer[..bytes_read]),
            Err(_) => return None,
        }
    }
    response_prefix.truncate(prefix_len);
    Some(response_prefix)
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    #[test]
    fn http_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_reachable(&endpoint));
        server.join().unwrap();
    }

    #[test]
    fn http_probe_accepts_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });

        assert!(HostTcpEndpointProbe.is_reachable(&endpoint));
        server.join().unwrap();
    }

    #[test]
    fn ws_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_ws_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_reachable(&endpoint));
        server.join().unwrap();
    }

    #[test]
    fn ws_probe_accepts_readyz_http_200() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_ws_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let bytes_read = stream.read(&mut request).unwrap();
            assert!(request[..bytes_read].starts_with(b"GET /readyz HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });

        assert!(HostTcpEndpointProbe.is_reachable(&endpoint));
        server.join().unwrap();
    }

    #[test]
    fn ws_probe_rejects_non_200_readyz_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_ws_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_reachable(&endpoint));
        server.join().unwrap();
    }

    fn local_http_endpoint(listener: &TcpListener) -> AttachEndpoint {
        AttachEndpoint {
            scheme: "http".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: listener.local_addr().unwrap().port(),
        }
    }

    fn local_ws_endpoint(listener: &TcpListener) -> AttachEndpoint {
        AttachEndpoint {
            scheme: "ws".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: listener.local_addr().unwrap().port(),
        }
    }
}

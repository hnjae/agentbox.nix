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
const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;
const OPENCODE_HEALTH_PATH: &str = "/global/health";
const CODEX_READY_PATH: &str = "/readyz";

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
        Ok(endpoint) if probe.is_ready(runtime, &endpoint) => {
            Ok(ServerEndpointState::Ready(endpoint))
        }
        Ok(endpoint) => Ok(ServerEndpointState::Pending(format!(
            "endpoint `{endpoint}` is not reachable yet"
        ))),
        Err(error) => Ok(ServerEndpointState::Pending(error.to_string())),
    }
}

trait EndpointProbe {
    fn is_ready(&self, runtime: RuntimeKind, endpoint: &AttachEndpoint) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct HostTcpEndpointProbe;

impl EndpointProbe for HostTcpEndpointProbe {
    fn is_ready(&self, runtime: RuntimeKind, endpoint: &AttachEndpoint) -> bool {
        let Ok(addresses) = (endpoint.host_ip.as_str(), endpoint.host_port).to_socket_addrs()
        else {
            return false;
        };

        addresses.into_iter().any(|address| {
            TcpStream::connect_timeout(&address, ENDPOINT_CONNECT_TIMEOUT).is_ok_and(
                |mut stream| endpoint_connection_is_ready(runtime, endpoint, &mut stream),
            )
        })
    }
}

fn endpoint_connection_is_ready(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
) -> bool {
    match runtime {
        RuntimeKind::Opencode => opencode_endpoint_is_ready(endpoint, stream),
        RuntimeKind::Codex => codex_endpoint_is_ready(endpoint, stream),
    }
}

fn opencode_endpoint_is_ready(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> bool {
    let Some(response) = http_get_response(endpoint, stream, OPENCODE_HEALTH_PATH) else {
        return false;
    };
    if response.status_code != 200 {
        return false;
    }
    serde_json::from_slice::<OpencodeHealthResponse>(&response.body)
        .is_ok_and(|health| health.healthy)
}

fn codex_endpoint_is_ready(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> bool {
    http_get_response(endpoint, stream, CODEX_READY_PATH)
        .is_some_and(|response| response.status_code == 200)
}

#[derive(serde::Deserialize)]
struct OpencodeHealthResponse {
    healthy: bool,
}

struct HttpResponse {
    status_code: u16,
    body: Vec<u8>,
}

fn http_get_response(
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
    path: &str,
) -> Option<HttpResponse> {
    let _ = stream.set_read_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        endpoint.host_ip, endpoint.host_port,
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return None;
    }

    let mut response = Vec::new();
    let body_start = loop {
        if let Some(body_start) = http_body_start(&response) {
            break body_start;
        }
        if response.len() >= MAX_HTTP_RESPONSE_BYTES {
            return None;
        }
        let mut buffer = [0_u8; 512];
        match stream.read(&mut buffer) {
            Ok(0) => return None,
            Ok(bytes_read) => response.extend_from_slice(&buffer[..bytes_read]),
            Err(_) => return None,
        }
    };

    let (status_code, content_length) = parse_http_response_headers(&response[..body_start])?;
    if let Some(content_length) = content_length {
        let response_len = body_start.checked_add(content_length)?;
        if response_len > MAX_HTTP_RESPONSE_BYTES {
            return None;
        }
        while response.len() < response_len {
            let mut buffer = [0_u8; 512];
            match stream.read(&mut buffer) {
                Ok(0) => return None,
                Ok(bytes_read) => response.extend_from_slice(&buffer[..bytes_read]),
                Err(_) => return None,
            }
        }
        response.truncate(response_len);
    } else {
        while response.len() < MAX_HTTP_RESPONSE_BYTES {
            let mut buffer = [0_u8; 512];
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes_read) => response.extend_from_slice(&buffer[..bytes_read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => return None,
            }
        }
    }

    Some(HttpResponse {
        status_code,
        body: response[body_start..].to_vec(),
    })
}

fn http_body_start(response: &[u8]) -> Option<usize> {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_http_response_headers(headers: &[u8]) -> Option<(u16, Option<usize>)> {
    let headers = std::str::from_utf8(headers).ok()?;
    let mut lines = headers.split("\r\n");
    let status_line = lines.next()?;
    let mut status_parts = status_line.split_whitespace();
    let http_version = status_parts.next()?;
    if !http_version.starts_with("HTTP/1.") {
        return None;
    }
    let status_code = status_parts.next()?.parse().ok()?;
    let mut content_length = None;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.trim().parse().ok()?);
        }
    }

    Some((status_code, content_length))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    #[test]
    fn opencode_probe_accepts_global_health_http_200_with_healthy_true() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let bytes_read = stream.read(&mut request).unwrap();
            assert!(request[..bytes_read].starts_with(b"GET /global/health HTTP/1.1"));
            stream
                .write_all(&http_response(
                    "200 OK",
                    r#"{"healthy":true,"version":"0.0.0-test"}"#,
                ))
                .unwrap();
        });

        assert!(HostTcpEndpointProbe.is_ready(RuntimeKind::Opencode, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn opencode_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Opencode, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn opencode_probe_rejects_non_200_global_health_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(&http_response(
                    "503 Service Unavailable",
                    r#"{"healthy":true}"#,
                ))
                .unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Opencode, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn opencode_probe_rejects_malformed_global_health_json() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(&http_response("200 OK", "not-json"))
                .unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Opencode, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn opencode_probe_rejects_unhealthy_global_health_json() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 128];
            let _ = stream.read(&mut request);
            stream
                .write_all(&http_response("200 OK", r#"{"healthy":false}"#))
                .unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Opencode, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn codex_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_ws_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Codex, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn codex_probe_accepts_readyz_http_200() {
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

        assert!(HostTcpEndpointProbe.is_ready(RuntimeKind::Codex, &endpoint));
        server.join().unwrap();
    }

    #[test]
    fn codex_probe_rejects_non_200_readyz_response() {
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

        assert!(!HostTcpEndpointProbe.is_ready(RuntimeKind::Codex, &endpoint));
        server.join().unwrap();
    }

    fn http_response(status: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
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

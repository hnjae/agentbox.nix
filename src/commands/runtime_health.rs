// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::runtime::{AttachEndpoint, RuntimeKind};

const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;
const OPENCODE_HEALTH_PATH: &str = "/global/health";
const CODEX_READY_PATH: &str = "/readyz";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeHealth {
    healthy: bool,
    reason: String,
}

impl RuntimeHealth {
    fn healthy() -> Self {
        Self {
            healthy: true,
            reason: "ok".to_string(),
        }
    }

    fn unhealthy(reason: impl Into<String>) -> Self {
        Self {
            healthy: false,
            reason: reason.into(),
        }
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

pub(crate) trait RuntimeHealthProbe {
    fn check(&self, runtime: RuntimeKind, endpoint: &AttachEndpoint) -> RuntimeHealth;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HostRuntimeHealthProbe;

impl RuntimeHealthProbe for HostRuntimeHealthProbe {
    fn check(&self, runtime: RuntimeKind, endpoint: &AttachEndpoint) -> RuntimeHealth {
        let Ok(addresses) = (endpoint.host_ip.as_str(), endpoint.host_port).to_socket_addrs()
        else {
            return RuntimeHealth::unhealthy("unreachable");
        };

        let mut last_unhealthy = None;
        for address in addresses {
            let Ok(mut stream) = TcpStream::connect_timeout(&address, ENDPOINT_CONNECT_TIMEOUT)
            else {
                continue;
            };

            let health = endpoint_connection_health(runtime, endpoint, &mut stream);
            if health.is_healthy() {
                return health;
            }
            last_unhealthy = Some(health);
        }

        last_unhealthy.unwrap_or_else(|| RuntimeHealth::unhealthy("unreachable"))
    }
}

fn endpoint_connection_health(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
) -> RuntimeHealth {
    match runtime {
        RuntimeKind::Opencode => opencode_endpoint_health(endpoint, stream),
        RuntimeKind::Codex => codex_endpoint_health(endpoint, stream),
    }
}

fn opencode_endpoint_health(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> RuntimeHealth {
    let Some(response) = http_get_response(endpoint, stream, OPENCODE_HEALTH_PATH) else {
        return RuntimeHealth::unhealthy("unreachable");
    };
    if response.status_code != 200 {
        return RuntimeHealth::unhealthy(format!("HTTP {}", response.status_code));
    }

    match serde_json::from_slice::<OpencodeHealthResponse>(&response.body) {
        Ok(health) if health.healthy => RuntimeHealth::healthy(),
        Ok(_) => RuntimeHealth::unhealthy("healthy=false"),
        Err(_) => RuntimeHealth::unhealthy("malformed JSON"),
    }
}

fn codex_endpoint_health(endpoint: &AttachEndpoint, stream: &mut TcpStream) -> RuntimeHealth {
    let Some(response) = http_get_response(endpoint, stream, CODEX_READY_PATH) else {
        return RuntimeHealth::unhealthy("unreachable");
    };
    if response.status_code == 200 {
        RuntimeHealth::healthy()
    } else {
        RuntimeHealth::unhealthy(format!("HTTP {}", response.status_code))
    }
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Opencode, &endpoint),
            RuntimeHealth::healthy()
        );
        server.join().unwrap();
    }

    #[test]
    fn opencode_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_http_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Opencode, &endpoint),
            RuntimeHealth::unhealthy("unreachable")
        );
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Opencode, &endpoint),
            RuntimeHealth::unhealthy("HTTP 503")
        );
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Opencode, &endpoint),
            RuntimeHealth::unhealthy("malformed JSON")
        );
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Opencode, &endpoint),
            RuntimeHealth::unhealthy("healthy=false")
        );
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Codex, &endpoint),
            RuntimeHealth::healthy()
        );
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

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Codex, &endpoint),
            RuntimeHealth::unhealthy("HTTP 503")
        );
        server.join().unwrap();
    }

    #[test]
    fn codex_probe_rejects_tcp_accept_without_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = local_ws_endpoint(&listener);
        let server = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });

        assert_eq!(
            HostRuntimeHealthProbe.check(RuntimeKind::Codex, &endpoint),
            RuntimeHealth::unhealthy("unreachable")
        );
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

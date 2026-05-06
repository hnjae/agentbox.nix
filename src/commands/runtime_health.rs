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

use crate::runtime::{
    AttachEndpoint, RuntimeHealthCheck, RuntimeHealthResponsePolicy, RuntimeKind,
};

const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;

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

            let health = endpoint_connection_health(runtime.health_check(), endpoint, &mut stream);
            if health.is_healthy() {
                return health;
            }
            last_unhealthy = Some(health);
        }

        last_unhealthy.unwrap_or_else(|| RuntimeHealth::unhealthy("unreachable"))
    }
}

fn endpoint_connection_health(
    health_check: RuntimeHealthCheck,
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
) -> RuntimeHealth {
    let Some(response) = http_get_response(endpoint, stream, health_check.path) else {
        return RuntimeHealth::unhealthy("unreachable");
    };

    health_response_status(response, health_check.response_policy)
}

fn health_response_status(
    response: HttpResponse,
    response_policy: RuntimeHealthResponsePolicy,
) -> RuntimeHealth {
    if response.status_code != 200 {
        return RuntimeHealth::unhealthy(format!("HTTP {}", response.status_code));
    }

    match response_policy {
        RuntimeHealthResponsePolicy::HttpOk => RuntimeHealth::healthy(),
        RuntimeHealthResponsePolicy::JsonHealthyFlag => {
            match serde_json::from_slice::<HealthyFlagResponse>(&response.body) {
                Ok(health) if health.healthy => RuntimeHealth::healthy(),
                Ok(_) => RuntimeHealth::unhealthy("healthy=false"),
                Err(_) => RuntimeHealth::unhealthy("malformed JSON"),
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct HealthyFlagResponse {
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

    write_http_get_request(endpoint, stream, path)?;

    let mut response = Vec::new();
    let body_start = read_until_http_body(stream, &mut response)?;
    let (status_code, content_length) = parse_http_response_headers(&response[..body_start])?;

    match content_length {
        Some(content_length) => {
            let response_len = body_start.checked_add(content_length)?;
            if response_len > MAX_HTTP_RESPONSE_BYTES {
                return None;
            }
            read_declared_response_body(stream, &mut response, response_len)?;
        }
        None => read_undeclared_response_body(stream, &mut response)?,
    }

    Some(HttpResponse {
        status_code,
        body: response[body_start..].to_vec(),
    })
}

fn write_http_get_request(
    endpoint: &AttachEndpoint,
    stream: &mut TcpStream,
    path: &str,
) -> Option<()> {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        endpoint.host_ip, endpoint.host_port,
    );
    stream.write_all(request.as_bytes()).ok()
}

fn read_until_http_body(stream: &mut TcpStream, response: &mut Vec<u8>) -> Option<usize> {
    loop {
        if let Some(body_start) = http_body_start(response) {
            return Some(body_start);
        }
        if response.len() >= MAX_HTTP_RESPONSE_BYTES {
            return None;
        }

        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout | HttpRead::Error => return None,
        }
    }
}

fn read_declared_response_body(
    stream: &mut TcpStream,
    response: &mut Vec<u8>,
    response_len: usize,
) -> Option<()> {
    while response.len() < response_len {
        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout | HttpRead::Error => return None,
        }
    }

    response.truncate(response_len);
    Some(())
}

fn read_undeclared_response_body(stream: &mut TcpStream, response: &mut Vec<u8>) -> Option<()> {
    while response.len() < MAX_HTTP_RESPONSE_BYTES {
        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout => break,
            HttpRead::Error => return None,
        }
    }

    Some(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpRead {
    Data,
    End,
    Timeout,
    Error,
}

fn read_http_chunk(stream: &mut TcpStream, response: &mut Vec<u8>) -> HttpRead {
    let mut buffer = [0_u8; 512];
    match stream.read(&mut buffer) {
        Ok(0) => HttpRead::End,
        Ok(bytes_read) => {
            response.extend_from_slice(&buffer[..bytes_read]);
            HttpRead::Data
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            HttpRead::Timeout
        }
        Err(_) => HttpRead::Error,
    }
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
        assert_probe_result(
            RuntimeKind::Opencode,
            http_response("200 OK", r#"{"healthy":true,"version":"0.0.0-test"}"#),
            RuntimeHealth::healthy(),
        );
    }

    #[test]
    fn opencode_probe_rejects_tcp_accept_without_http_response() {
        assert_probe_result(
            RuntimeKind::Opencode,
            ProbeResponse::CloseWithoutResponse,
            RuntimeHealth::unhealthy("unreachable"),
        );
    }

    #[test]
    fn opencode_probe_rejects_non_200_global_health_response() {
        assert_probe_result(
            RuntimeKind::Opencode,
            http_response("503 Service Unavailable", r#"{"healthy":true}"#),
            RuntimeHealth::unhealthy("HTTP 503"),
        );
    }

    #[test]
    fn opencode_probe_rejects_malformed_global_health_json() {
        assert_probe_result(
            RuntimeKind::Opencode,
            http_response("200 OK", "not-json"),
            RuntimeHealth::unhealthy("malformed JSON"),
        );
    }

    #[test]
    fn opencode_probe_rejects_unhealthy_global_health_json() {
        assert_probe_result(
            RuntimeKind::Opencode,
            http_response("200 OK", r#"{"healthy":false}"#),
            RuntimeHealth::unhealthy("healthy=false"),
        );
    }

    #[test]
    fn codex_probe_accepts_readyz_http_200() {
        assert_probe_result(
            RuntimeKind::Codex,
            http_response("200 OK", ""),
            RuntimeHealth::healthy(),
        );
    }

    #[test]
    fn codex_probe_rejects_non_200_readyz_response() {
        assert_probe_result(
            RuntimeKind::Codex,
            http_response("503 Service Unavailable", ""),
            RuntimeHealth::unhealthy("HTTP 503"),
        );
    }

    #[test]
    fn codex_probe_rejects_tcp_accept_without_http_response() {
        assert_probe_result(
            RuntimeKind::Codex,
            ProbeResponse::CloseWithoutResponse,
            RuntimeHealth::unhealthy("unreachable"),
        );
    }

    fn assert_probe_result(runtime: RuntimeKind, response: ProbeResponse, expected: RuntimeHealth) {
        let server = ProbeServer::start(runtime, response);
        assert_eq!(
            HostRuntimeHealthProbe.check(runtime, server.endpoint()),
            expected
        );
        server.join();
    }

    struct ProbeServer {
        endpoint: AttachEndpoint,
        handle: thread::JoinHandle<()>,
    }

    impl ProbeServer {
        fn start(runtime: RuntimeKind, response: ProbeResponse) -> Self {
            let (scheme, expected_path) = runtime_probe_contract(runtime);
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let endpoint = AttachEndpoint {
                scheme: scheme.to_string(),
                host_ip: "127.0.0.1".to_string(),
                host_port: listener.local_addr().unwrap().port(),
            };
            let handle = thread::spawn(move || serve_probe(listener, expected_path, response));

            Self { endpoint, handle }
        }

        fn endpoint(&self) -> &AttachEndpoint {
            &self.endpoint
        }

        fn join(self) {
            self.handle.join().unwrap();
        }
    }

    enum ProbeResponse {
        Bytes(Vec<u8>),
        CloseWithoutResponse,
    }

    fn serve_probe(listener: TcpListener, expected_path: &'static str, response: ProbeResponse) {
        let (mut stream, _) = listener.accept().unwrap();

        if let ProbeResponse::Bytes(response) = response {
            let mut request = [0_u8; 128];
            let bytes_read = stream.read(&mut request).unwrap();
            let expected_request = format!("GET {expected_path} HTTP/1.1");
            assert!(request[..bytes_read].starts_with(expected_request.as_bytes()));
            stream.write_all(&response).unwrap();
        }
    }

    fn runtime_probe_contract(runtime: RuntimeKind) -> (&'static str, &'static str) {
        match runtime {
            RuntimeKind::Opencode => ("http", "/global/health"),
            RuntimeKind::Codex => ("ws", "/readyz"),
        }
    }

    fn http_response(status: &str, body: &str) -> ProbeResponse {
        ProbeResponse::Bytes(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .into_bytes(),
        )
    }
}

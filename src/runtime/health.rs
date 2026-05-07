// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::RuntimeKind;
use super::http_probe::{self, HttpResponse};
use super::spec::{AttachEndpoint, RuntimeHealthCheck, RuntimeHealthResponsePolicy};

const ENDPOINT_CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeHealth {
    Healthy,
    Unhealthy { reason: String },
}

impl RuntimeHealth {
    fn healthy() -> Self {
        Self::Healthy
    }

    pub(crate) fn unhealthy(reason: impl Into<String>) -> Self {
        Self::Unhealthy {
            reason: reason.into(),
        }
    }

    pub(crate) fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    pub(crate) fn status_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unhealthy { .. } => "unhealthy",
        }
    }

    pub(crate) fn reason(&self) -> &str {
        match self {
            Self::Healthy => "ok",
            Self::Unhealthy { reason } => reason,
        }
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
    let _ = stream.set_read_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(ENDPOINT_CONNECT_TIMEOUT));

    let Some(response) = http_probe::get_response(endpoint, stream, health_check.path) else {
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
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
        (runtime.attach_spec().scheme, runtime.health_check().path)
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

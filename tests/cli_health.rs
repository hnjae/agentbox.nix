// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use agentbox::runtime::RuntimeKind;

#[path = "support/mod.rs"]
mod support;

use support::{
    CliHarness as Harness, opencode_workspace_inspect_fixture, ps_fixture,
    running_workspace_inspect_fixture_with_host_port, workspace_ps_entry,
};

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[test]
fn health_reports_running_opencode_session_as_healthy() {
    let fixture = support::temp_workspace("opencode");
    let workspace = &fixture.workspace;
    let endpoint = HealthEndpoint::opencode_healthy();
    let port = endpoint.port();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            port,
        ),
    );

    let output = harness.agentbox_output(&["health"]);
    endpoint.wait();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(workspace.canonical_git_root.as_str()));
    assert!(stdout.contains("opencode"));
    assert!(stdout.contains("healthy"));
    assert!(stdout.contains("ok"));
    assert!(stdout.contains(&format!("http://127.0.0.1:{port}")));
    assert!(stdout.contains(&workspace.container_name));
    assert_no_box_drawing_borders(&stdout);
}

#[test]
fn health_reports_running_codex_session_as_healthy() {
    let fixture = support::temp_workspace("codex");
    let workspace = &fixture.workspace;
    let endpoint = HealthEndpoint::codex_ready();
    let port = endpoint.port();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            RuntimeKind::Codex.default_image(),
            RuntimeKind::Codex,
            port,
        ),
    );

    let output = harness.agentbox_output(&["health"]);
    endpoint.wait();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(workspace.canonical_git_root.as_str()));
    assert!(stdout.contains("codex"));
    assert!(stdout.contains("healthy"));
    assert!(stdout.contains("ok"));
    assert!(stdout.contains(&format!("ws://127.0.0.1:{port}")));
    assert!(stdout.contains(&workspace.container_name));
}

#[test]
fn health_reports_unhealthy_opencode_without_failing() {
    let fixture = support::temp_workspace("unhealthy");
    let workspace = &fixture.workspace;
    let endpoint = HealthEndpoint::opencode_unhealthy();
    let port = endpoint.port();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![workspace_ps_entry(
        "running-id",
        workspace,
    )]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture_with_host_port(
            workspace,
            RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            port,
        ),
    );

    let output = harness.agentbox_output(&["health"]);
    endpoint.wait();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("unhealthy"));
    assert!(stdout.contains("healthy=false"));
    assert!(stdout.contains(&workspace.container_name));
}

#[test]
fn health_json_reports_healthy_and_unhealthy_rows_without_failing() {
    let healthy_fixture = support::temp_workspace("healthy");
    let unhealthy_fixture = support::temp_workspace("unhealthy");
    let healthy = &healthy_fixture.workspace;
    let unhealthy = &unhealthy_fixture.workspace;
    let healthy_endpoint = HealthEndpoint::opencode_healthy();
    let unhealthy_endpoint = HealthEndpoint::opencode_unhealthy();
    let healthy_port = healthy_endpoint.port();
    let unhealthy_port = unhealthy_endpoint.port();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("healthy-id", healthy),
        workspace_ps_entry("unhealthy-id", unhealthy),
    ]));
    harness.write_inspect(
        "healthy-id",
        &running_workspace_inspect_fixture_with_host_port(
            healthy,
            RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            healthy_port,
        ),
    );
    harness.write_inspect(
        "unhealthy-id",
        &running_workspace_inspect_fixture_with_host_port(
            unhealthy,
            RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            unhealthy_port,
        ),
    );

    let output = harness.agentbox_output(&["health", "--output=json"]);
    healthy_endpoint.wait();
    unhealthy_endpoint.wait();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let roots = rows
        .iter()
        .map(|row| row["canonical_git_root"].as_str().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(rows.len(), 2);
    assert!(roots.windows(2).all(|window| window[0] <= window[1]));
    assert_eq!(stdout.matches('\n').count(), 1);

    let healthy_row = rows
        .iter()
        .find(|row| row["container_name"] == healthy.container_name)
        .unwrap();
    assert_eq!(healthy_row["runtime"], "opencode");
    assert_eq!(healthy_row["health"], "healthy");
    assert_eq!(healthy_row["reason"], "ok");
    assert_eq!(
        healthy_row["endpoint"],
        format!("http://127.0.0.1:{healthy_port}")
    );

    let unhealthy_row = rows
        .iter()
        .find(|row| row["container_name"] == unhealthy.container_name)
        .unwrap();
    assert_eq!(unhealthy_row["health"], "unhealthy");
    assert_eq!(unhealthy_row["reason"], "healthy=false");
    assert_eq!(
        unhealthy_row["endpoint"],
        format!("http://127.0.0.1:{unhealthy_port}")
    );
}

#[test]
fn health_filters_non_running_session_statuses() {
    let running_fixture = support::temp_workspace("running");
    let stopped_fixture = support::temp_workspace("stopped");
    let failed_fixture = support::temp_workspace("failed");
    let orphan_fixture = support::temp_workspace("orphan");
    let running = &running_fixture.workspace;
    let stopped = &stopped_fixture.workspace;
    let failed = &failed_fixture.workspace;
    let orphan = &orphan_fixture.workspace;
    let endpoint = HealthEndpoint::opencode_healthy();
    let port = endpoint.port();
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(vec![
        workspace_ps_entry("running-id", running),
        workspace_ps_entry("stopped-id", stopped),
        workspace_ps_entry("failed-id", failed),
        workspace_ps_entry("orphan-id", orphan),
    ]));
    harness.write_inspect(
        "running-id",
        &running_workspace_inspect_fixture_with_host_port(
            running,
            RuntimeKind::Opencode.default_image(),
            RuntimeKind::Opencode,
            port,
        ),
    );
    harness.write_inspect(
        "stopped-id",
        &opencode_workspace_inspect_fixture(stopped, false, true),
    );
    harness.write_inspect(
        "failed-id",
        &opencode_workspace_inspect_fixture(failed, true, false),
    );
    harness.write_inspect(
        "orphan-id",
        &opencode_workspace_inspect_fixture(orphan, true, true),
    );
    std::fs::remove_dir_all(orphan.canonical_git_root.as_std_path()).unwrap();

    let output = harness.agentbox_output(&["health"]);
    endpoint.wait();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(&running.container_name));
    assert!(!stdout.contains(&stopped.container_name));
    assert!(!stdout.contains(&failed.container_name));
    assert!(!stdout.contains(&orphan.container_name));
}

#[test]
fn health_with_no_running_sessions_prints_header_only_table() {
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));

    let output = harness.agentbox_output(&["health"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("canonical git root"));
    assert!(stdout.contains("runtime"));
    assert!(stdout.contains("health"));
    assert!(stdout.contains("reason"));
    assert!(stdout.contains("endpoint"));
    assert!(stdout.contains("container name"));
    assert!(!stdout.contains("opencode"));
    assert!(!stdout.contains("codex"));
    assert_no_box_drawing_borders(&stdout);
}

#[test]
fn health_json_with_no_running_sessions_prints_empty_array() {
    let harness = Harness::new();
    harness.write_ps(&ps_fixture(Vec::new()));

    let output = harness.agentbox_output(&["health", "--output=json"]);

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "[]\n");
}

fn assert_no_box_drawing_borders(table: &str) {
    let border = table
        .chars()
        .find(|character| ('\u{2500}'..='\u{257f}').contains(character));
    assert!(border.is_none(), "table contains a border: {table}");
}

struct HealthEndpoint {
    port: u16,
    handle: Option<JoinHandle<()>>,
}

impl HealthEndpoint {
    fn opencode_healthy() -> Self {
        Self::start(
            "/global/health",
            http_response("200 OK", r#"{"healthy":true,"version":"0.0.0-test"}"#),
        )
    }

    fn opencode_unhealthy() -> Self {
        Self::start(
            "/global/health",
            http_response("200 OK", r#"{"healthy":false}"#),
        )
    }

    fn codex_ready() -> Self {
        Self::start(
            "/readyz",
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_string(),
        )
    }

    fn start(expected_path: &'static str, response: String) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || serve_one_probe(listener, expected_path, response));

        Self {
            port,
            handle: Some(handle),
        }
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn wait(mut self) {
        self.join();
    }

    fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

impl Drop for HealthEndpoint {
    fn drop(&mut self) {
        self.join();
    }
}

fn serve_one_probe(listener: TcpListener, expected_path: &'static str, response: String) {
    let deadline = Instant::now() + ACCEPT_TIMEOUT;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(ACCEPT_TIMEOUT));
                let _ = stream.set_write_timeout(Some(ACCEPT_TIMEOUT));
                let mut request = [0_u8; 256];
                let bytes_read = stream.read(&mut request).unwrap();
                let expected_request = format!("GET {expected_path} HTTP/1.1");
                assert!(request[..bytes_read].starts_with(expected_request.as_bytes()));
                stream.write_all(response.as_bytes()).unwrap();
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return,
        }
    }
}

fn http_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

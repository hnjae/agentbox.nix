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

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct ReadyEndpoint {
    port: u16,
    handle: Option<JoinHandle<()>>,
}

impl ReadyEndpoint {
    pub fn start(runtime: RuntimeKind) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || serve_one_ready_probe(listener, runtime));

        Self {
            port,
            handle: Some(handle),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn wait(mut self) {
        self.join();
    }

    fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

impl Drop for ReadyEndpoint {
    fn drop(&mut self) {
        self.join();
    }
}

fn serve_one_ready_probe(listener: TcpListener, runtime: RuntimeKind) {
    let deadline = Instant::now() + ACCEPT_TIMEOUT;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(ACCEPT_TIMEOUT));
                let _ = stream.set_write_timeout(Some(ACCEPT_TIMEOUT));
                let mut request = [0_u8; 128];
                let bytes_read = stream.read(&mut request).unwrap();
                match runtime {
                    RuntimeKind::Opencode => {
                        assert!(request[..bytes_read].starts_with(b"GET /global/health HTTP/1.1"));
                        let body = r#"{"healthy":true,"version":"0.0.0-test"}"#;
                        stream
                            .write_all(http_response("200 OK", body).as_bytes())
                            .unwrap();
                    }
                    RuntimeKind::Codex => {
                        assert!(request[..bytes_read].starts_with(b"GET /readyz HTTP/1.1"));
                        stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                            .unwrap();
                    }
                }
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

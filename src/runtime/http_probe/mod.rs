// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{Read, Write};

use super::AttachEndpoint;

mod response;

pub(crate) use response::HttpResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpProbeError {
    RequestWriteFailed,
    ResponseHeadersUnavailable,
    InvalidResponseHeaders,
    ResponseTooLarge,
    ResponseBodyIncomplete,
    ResponseReadFailed,
}

pub(crate) fn get_response(
    endpoint: &AttachEndpoint,
    stream: &mut impl ReadWrite,
    path: &str,
) -> Result<HttpResponse, HttpProbeError> {
    write_http_get_request(endpoint, stream, path)?;
    response::read_response(stream)
}

pub(crate) trait ReadWrite: Read + Write {}

impl<T> ReadWrite for T where T: Read + Write {}

fn write_http_get_request(
    endpoint: &AttachEndpoint,
    stream: &mut impl Write,
    path: &str,
) -> Result<(), HttpProbeError> {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        endpoint.host_ip, endpoint.host_port,
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| HttpProbeError::RequestWriteFailed)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};

    use super::*;

    #[test]
    fn get_response_writes_request_with_host_header() {
        let mut stream = TestStream::with_reads([b"HTTP/1.1 204 No Content\r\n\r\n".as_slice()]);

        let response = get_response(&endpoint(), &mut stream, "/health").unwrap();

        assert_eq!(response.status_code, 204);
        assert_eq!(
            stream.writes,
            b"GET /health HTTP/1.1\r\nHost: 127.0.0.1:49152\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn get_response_rejects_request_write_failure() {
        let mut stream = TestStream::failing_writes();

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::RequestWriteFailed
        );
    }

    fn endpoint() -> AttachEndpoint {
        AttachEndpoint {
            scheme: "http".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: 49152,
        }
    }

    struct TestStream {
        reads: VecDeque<Vec<u8>>,
        writes: Vec<u8>,
        fail_writes: bool,
    }

    impl TestStream {
        fn with_reads<const N: usize>(reads: [&[u8]; N]) -> Self {
            Self {
                reads: reads.iter().map(|read| read.to_vec()).collect(),
                writes: Vec::new(),
                fail_writes: false,
            }
        }

        fn failing_writes() -> Self {
            Self {
                fail_writes: true,
                ..Self::with_reads([])
            }
        }
    }

    impl Read for TestStream {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let Some(mut next) = self.reads.pop_front() else {
                return Ok(0);
            };
            let bytes_read = next.len().min(buffer.len());
            buffer[..bytes_read].copy_from_slice(&next[..bytes_read]);

            if bytes_read < next.len() {
                next.drain(..bytes_read);
                self.reads.push_front(next);
            }

            Ok(bytes_read)
        }
    }

    impl Write for TestStream {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.fail_writes {
                return Err(io::Error::other("write failed"));
            }

            self.writes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}

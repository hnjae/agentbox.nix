// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{Read, Write};

use super::AttachEndpoint;

const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpResponse {
    pub(crate) status_code: u16,
    pub(crate) body: Vec<u8>,
}

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

    let mut response = Vec::new();
    let body_start = read_until_http_body(stream, &mut response)?;
    let (status_code, content_length) = parse_http_response_headers(&response[..body_start])?;

    match content_length {
        Some(content_length) => {
            let response_len = body_start
                .checked_add(content_length)
                .ok_or(HttpProbeError::ResponseTooLarge)?;
            if response_len > MAX_HTTP_RESPONSE_BYTES {
                return Err(HttpProbeError::ResponseTooLarge);
            }
            read_declared_response_body(stream, &mut response, response_len)?;
        }
        None => read_undeclared_response_body(stream, &mut response)?,
    }

    Ok(HttpResponse {
        status_code,
        body: response[body_start..].to_vec(),
    })
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

fn read_until_http_body(
    stream: &mut impl Read,
    response: &mut Vec<u8>,
) -> Result<usize, HttpProbeError> {
    loop {
        if let Some(body_start) = http_body_start(response) {
            return Ok(body_start);
        }
        if response.len() >= MAX_HTTP_RESPONSE_BYTES {
            return Err(HttpProbeError::ResponseTooLarge);
        }

        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout => {
                return Err(HttpProbeError::ResponseHeadersUnavailable);
            }
            HttpRead::Error => return Err(HttpProbeError::ResponseReadFailed),
        }
    }
}

fn read_declared_response_body(
    stream: &mut impl Read,
    response: &mut Vec<u8>,
    response_len: usize,
) -> Result<(), HttpProbeError> {
    while response.len() < response_len {
        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout => {
                return Err(HttpProbeError::ResponseBodyIncomplete);
            }
            HttpRead::Error => return Err(HttpProbeError::ResponseReadFailed),
        }
    }

    response.truncate(response_len);
    Ok(())
}

fn read_undeclared_response_body(
    stream: &mut impl Read,
    response: &mut Vec<u8>,
) -> Result<(), HttpProbeError> {
    while response.len() < MAX_HTTP_RESPONSE_BYTES {
        match read_http_chunk(stream, response) {
            HttpRead::Data => {}
            HttpRead::End | HttpRead::Timeout => break,
            HttpRead::Error => return Err(HttpProbeError::ResponseReadFailed),
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpRead {
    Data,
    End,
    Timeout,
    Error,
}

fn read_http_chunk(stream: &mut impl Read, response: &mut Vec<u8>) -> HttpRead {
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

fn parse_http_response_headers(headers: &[u8]) -> Result<(u16, Option<usize>), HttpProbeError> {
    let headers =
        std::str::from_utf8(headers).map_err(|_| HttpProbeError::InvalidResponseHeaders)?;
    let mut lines = headers.split("\r\n");
    let status_line = lines.next().ok_or(HttpProbeError::InvalidResponseHeaders)?;
    let mut status_parts = status_line.split_whitespace();
    let http_version = status_parts
        .next()
        .ok_or(HttpProbeError::InvalidResponseHeaders)?;
    if !http_version.starts_with("HTTP/1.") {
        return Err(HttpProbeError::InvalidResponseHeaders);
    }
    let status_code = status_parts
        .next()
        .ok_or(HttpProbeError::InvalidResponseHeaders)?
        .parse()
        .map_err(|_| HttpProbeError::InvalidResponseHeaders)?;
    let mut content_length = None;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| HttpProbeError::InvalidResponseHeaders)?,
            );
        }
    }

    Ok((status_code, content_length))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;

    use super::*;

    #[test]
    fn get_response_reads_declared_body_across_chunks() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello".as_slice(),
            b" world".as_slice(),
        ]);

        let response = get_response(&endpoint(), &mut stream, "/readyz").unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn get_response_reads_undeclared_body_until_eof() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\n\r\nhello".as_slice(),
            b" world".as_slice(),
        ]);

        let response = get_response(&endpoint(), &mut stream, "/readyz").unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn get_response_rejects_oversized_declared_body() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_HTTP_RESPONSE_BYTES + 1
        );
        let mut stream = TestStream::with_reads([response.as_bytes()]);

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::ResponseTooLarge
        );
    }

    #[test]
    fn get_response_rejects_missing_header_terminator() {
        let mut stream = TestStream::with_reads([b"HTTP/1.1 200 OK".as_slice()]);

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::ResponseHeadersUnavailable
        );
    }

    #[test]
    fn get_response_rejects_invalid_headers() {
        let mut stream = TestStream::with_reads([b"not-http\r\n\r\n".as_slice()]);

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::InvalidResponseHeaders
        );
    }

    #[test]
    fn get_response_rejects_incomplete_declared_body() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort".as_slice(),
        ]);

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::ResponseBodyIncomplete
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

    #[test]
    fn get_response_rejects_response_read_failure() {
        let mut stream = TestStream::failing_reads();

        assert_eq!(
            get_response(&endpoint(), &mut stream, "/readyz").unwrap_err(),
            HttpProbeError::ResponseReadFailed
        );
    }

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
        fail_reads: bool,
        fail_writes: bool,
    }

    impl TestStream {
        fn with_reads<const N: usize>(reads: [&[u8]; N]) -> Self {
            Self {
                reads: reads.iter().map(|read| read.to_vec()).collect(),
                writes: Vec::new(),
                fail_reads: false,
                fail_writes: false,
            }
        }

        fn failing_reads() -> Self {
            Self {
                fail_reads: true,
                ..Self::with_reads([])
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
            if self.fail_reads {
                return Err(io::Error::other("read failed"));
            }

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

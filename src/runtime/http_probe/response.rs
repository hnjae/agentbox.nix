// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::Read;

use super::HttpProbeError;

const MAX_HTTP_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpResponse {
    pub(crate) status_code: u16,
    pub(crate) body: Vec<u8>,
}

pub(super) fn read_response(stream: &mut impl Read) -> Result<HttpResponse, HttpProbeError> {
    HttpResponseReader::new(stream).read()
}

struct HttpResponseReader<'a, R> {
    stream: &'a mut R,
    response: Vec<u8>,
}

impl<'a, R> HttpResponseReader<'a, R>
where
    R: Read,
{
    fn new(stream: &'a mut R) -> Self {
        Self {
            stream,
            response: Vec::new(),
        }
    }

    fn read(mut self) -> Result<HttpResponse, HttpProbeError> {
        let body_start = self.read_headers()?;
        let (status_code, content_length) =
            parse_http_response_headers(&self.response[..body_start])?;

        match content_length {
            Some(content_length) => self.read_declared_body(body_start, content_length)?,
            None => self.read_undeclared_body()?,
        }

        Ok(HttpResponse {
            status_code,
            body: self.response[body_start..].to_vec(),
        })
    }

    fn read_headers(&mut self) -> Result<usize, HttpProbeError> {
        loop {
            if let Some(body_start) = http_body_start(&self.response) {
                return Ok(body_start);
            }

            match self.read_chunk()? {
                HttpRead::Data => {}
                HttpRead::End | HttpRead::Timeout => {
                    return Err(HttpProbeError::ResponseHeadersUnavailable);
                }
            }
        }
    }

    fn read_declared_body(
        &mut self,
        body_start: usize,
        content_length: usize,
    ) -> Result<(), HttpProbeError> {
        let response_len = body_start
            .checked_add(content_length)
            .ok_or(HttpProbeError::ResponseTooLarge)?;
        if response_len > MAX_HTTP_RESPONSE_BYTES {
            return Err(HttpProbeError::ResponseTooLarge);
        }

        while self.response.len() < response_len {
            match self.read_chunk()? {
                HttpRead::Data => {}
                HttpRead::End | HttpRead::Timeout => {
                    return Err(HttpProbeError::ResponseBodyIncomplete);
                }
            }
        }

        self.response.truncate(response_len);
        Ok(())
    }

    fn read_undeclared_body(&mut self) -> Result<(), HttpProbeError> {
        loop {
            match self.read_chunk()? {
                HttpRead::Data => {}
                HttpRead::End | HttpRead::Timeout => return Ok(()),
            }
        }
    }

    fn read_chunk(&mut self) -> Result<HttpRead, HttpProbeError> {
        let mut buffer = [0_u8; 512];
        match self.stream.read(&mut buffer) {
            Ok(0) => Ok(HttpRead::End),
            Ok(bytes_read) => {
                let next_len = self
                    .response
                    .len()
                    .checked_add(bytes_read)
                    .ok_or(HttpProbeError::ResponseTooLarge)?;
                if next_len > MAX_HTTP_RESPONSE_BYTES {
                    return Err(HttpProbeError::ResponseTooLarge);
                }

                self.response.extend_from_slice(&buffer[..bytes_read]);
                Ok(HttpRead::Data)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(HttpRead::Timeout)
            }
            Err(_) => Err(HttpProbeError::ResponseReadFailed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpRead {
    Data,
    End,
    Timeout,
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
    fn reads_declared_body_across_chunks() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello".as_slice(),
            b" world".as_slice(),
        ]);

        let response = read_response(&mut stream).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn reads_undeclared_body_until_eof() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\n\r\nhello".as_slice(),
            b" world".as_slice(),
        ]);

        let response = read_response(&mut stream).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn rejects_oversized_declared_body() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_HTTP_RESPONSE_BYTES + 1
        );
        let mut stream = TestStream::with_reads([response.as_bytes()]);

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::ResponseTooLarge
        );
    }

    #[test]
    fn rejects_oversized_undeclared_body() {
        let mut body = vec![b'a'; MAX_HTTP_RESPONSE_BYTES];
        body.extend_from_slice(b"b");
        let mut response = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        response.extend(body);
        let mut stream = TestStream::with_reads([response.as_slice()]);

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::ResponseTooLarge
        );
    }

    #[test]
    fn rejects_missing_header_terminator() {
        let mut stream = TestStream::with_reads([b"HTTP/1.1 200 OK".as_slice()]);

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::ResponseHeadersUnavailable
        );
    }

    #[test]
    fn rejects_invalid_headers() {
        let mut stream = TestStream::with_reads([b"not-http\r\n\r\n".as_slice()]);

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::InvalidResponseHeaders
        );
    }

    #[test]
    fn rejects_incomplete_declared_body() {
        let mut stream = TestStream::with_reads([
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort".as_slice(),
        ]);

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::ResponseBodyIncomplete
        );
    }

    #[test]
    fn rejects_response_read_failure() {
        let mut stream = TestStream::failing_reads();

        assert_eq!(
            read_response(&mut stream).unwrap_err(),
            HttpProbeError::ResponseReadFailed
        );
    }

    struct TestStream {
        reads: VecDeque<Vec<u8>>,
        fail_reads: bool,
    }

    impl TestStream {
        fn with_reads<const N: usize>(reads: [&[u8]; N]) -> Self {
            Self {
                reads: reads.iter().map(|read| read.to_vec()).collect(),
                fail_reads: false,
            }
        }

        fn failing_reads() -> Self {
            Self {
                fail_reads: true,
                ..Self::with_reads([])
            }
        }
    }

    impl io::Read for TestStream {
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
}

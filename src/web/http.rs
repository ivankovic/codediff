/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
//! The HTTP/1.1 subset `codediff-web` speaks, over tokio's `TcpStream`.
//!
//! Hand-written rather than a server crate, and that is a deliberate trade, not an oversight.
//! What the page needs is exactly this: a handful of `GET`s for static assets and `POST`s carrying
//! a small JSON body, from one browser on the same machine, each answered with one response and a
//! closed connection. That is a request line, a header block, and a `Content-Length` body -
//! perhaps two hundred lines with tests. Every server crate that would do it for us brings its own
//! dependency graph (`axum` roughly thirty crates, `tiny_http` a handful), and every crate in
//! `Cargo.lock` is a line in packaging/gentoo's generated `CRATES=` block and an entry in its
//! `LICENSE` enumeration, whether or not the feature that needs it is enabled. `tokio` is already
//! there for the TUI. See `SPECS.md`'s decision log for the alternatives weighed.
//!
//! What is deliberately *not* here, because nothing sends it: chunked request bodies (a browser's
//! `fetch` with a string body always sends `Content-Length`), keep-alive (every response says
//! `Connection: close`, which every browser honours), pipelining, and anything but HTTP/1.x.
//! Requests outside that subset are rejected with a 4xx rather than misread.

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Request line plus headers may not exceed this. Real requests from a browser are a few hundred
/// bytes; anything near this bound is not one.
pub const MAX_HEAD_BYTES: usize = 64 * 1024;

/// The largest body accepted. The API's bodies are file paths, option sets and a hex colour or
/// two; the bound only has to stop an unbounded read, not accommodate anything.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path only - a query string, if any, is dropped at parse time. Every endpoint here takes
    /// its parameters in a JSON body, so there is nothing to read from one.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header named `name`, compared case-insensitively as HTTP requires.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug)]
pub enum HttpError {
    /// Not HTTP/1.x as this module understands it - which status to answer with is the caller's
    /// call, the message says what was wrong.
    Malformed(&'static str),
    /// Head or body over its bound.
    TooLarge,
    Io(std::io::Error),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Malformed(what) => write!(f, "malformed request: {what}"),
            HttpError::TooLarge => write!(f, "request too large"),
            HttpError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl From<std::io::Error> for HttpError {
    fn from(err: std::io::Error) -> Self {
        HttpError::Io(err)
    }
}

/// A parsed request head: everything before the blank line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

/// Parses a request head into its method, path and headers. Header names keep their case
/// (lookups through [`Request::header`] ignore it); values are trimmed of surrounding
/// whitespace, as the grammar allows.
pub fn parse_head(head: &str) -> Result<Head, HttpError> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts
        .next()
        .filter(|m| !m.is_empty())
        .ok_or(HttpError::Malformed("empty request line"))?;
    let target = parts
        .next()
        .ok_or(HttpError::Malformed("request line has no target"))?;
    let version = parts
        .next()
        .ok_or(HttpError::Malformed("request line has no version"))?;
    if !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err(HttpError::Malformed("not an HTTP/1.x request line"));
    }
    if !target.starts_with('/') {
        return Err(HttpError::Malformed("target is not an absolute path"));
    }
    let path = target.split('?').next().unwrap_or(target).to_string();

    let mut headers = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or(HttpError::Malformed("header line has no colon"))?;
        if name.is_empty() || name.contains(' ') {
            return Err(HttpError::Malformed("bad header name"));
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    Ok(Head {
        method: method.to_string(),
        path,
        headers,
    })
}

/// Reads one request. `Ok(None)` means the peer closed the connection before sending anything,
/// which a browser does routinely (speculative preconnects) and which is not an error.
pub async fn read_request<R>(reader: &mut R) -> Result<Option<Request>, HttpError>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut head = Vec::new();
    loop {
        let before = head.len();
        let read = reader.read_until(b'\n', &mut head).await?;
        if read == 0 {
            if head.is_empty() {
                return Ok(None);
            }
            return Err(HttpError::Malformed("connection closed mid-head"));
        }
        if head.len() > MAX_HEAD_BYTES {
            return Err(HttpError::TooLarge);
        }
        // The blank line ending the head is `\r\n` on its own; a bare `\n` is tolerated the way
        // most servers do.
        let line = &head[before..];
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    let head = std::str::from_utf8(&head).map_err(|_| HttpError::Malformed("head is not UTF-8"))?;
    // Lenient about a bare `\n` in the head too, for the same reason as above.
    let normalized = head.replace("\r\n", "\n").replace('\n', "\r\n");
    let head = parse_head(normalized.trim_end())?;

    let mut request = Request {
        method: head.method,
        path: head.path,
        headers: head.headers,
        body: Vec::new(),
    };
    if request
        .header("Transfer-Encoding")
        .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return Err(HttpError::Malformed(
            "chunked request bodies are not supported",
        ));
    }
    let length = match request.header("Content-Length") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| HttpError::Malformed("bad Content-Length"))?,
        None => 0,
    };
    if length > MAX_BODY_BYTES {
        return Err(HttpError::TooLarge);
    }
    if length > 0 {
        let mut body = vec![0; length];
        reader.read_exact(&mut body).await?;
        request.body = body;
    }
    Ok(Some(request))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json<T: Serialize>(status: u16, value: &T) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|err| {
            // A serialization failure here is a bug in a payload type, not a runtime condition;
            // surface it to the page rather than answering with nothing.
            serde_json::to_vec(&serde_json::json!({ "error": err.to_string() }))
                .expect("a one-field object serializes")
        });
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    /// `{"error": message}` - the one error shape the page knows how to show.
    pub fn error(status: u16, message: impl Into<String>) -> Self {
        Self::json(status, &serde_json::json!({ "error": message.into() }))
    }

    pub fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    pub fn asset(content_type: &'static str, body: &'static str) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            _ => "Unknown",
        }
    }

    /// The bytes on the wire. `Connection: close` because every request gets its own connection
    /// (see the module comment); `no-store` because the page is generated with a per-run token
    /// and the API's answers describe files that may change under it; `nosniff` so nothing here
    /// is ever reinterpreted as a different type than it was served as.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\
             Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }

    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.to_bytes()).await?;
        writer.flush().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    async fn read(bytes: &[u8]) -> Result<Option<Request>, HttpError> {
        let mut reader = BufReader::new(bytes);
        read_request(&mut reader).await
    }

    #[tokio::test]
    async fn a_get_with_headers_and_no_body_parses() {
        let request =
            read(b"GET /app.js?v=1 HTTP/1.1\r\nHost: 127.0.0.1:8080\r\nX-Thing:  spaced \r\n\r\n")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/app.js", "the query string is dropped");
        assert_eq!(request.header("host"), Some("127.0.0.1:8080"));
        assert_eq!(request.header("x-thing"), Some("spaced"));
        assert!(request.body.is_empty());
    }

    #[tokio::test]
    async fn a_post_reads_exactly_content_length_bytes() {
        let request = read(b"POST /api/diff HTTP/1.1\r\nContent-Length: 5\r\n\r\nhelloTRAILING")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.body, b"hello");
    }

    #[tokio::test]
    async fn a_closed_connection_before_any_bytes_is_not_an_error() {
        assert!(read(b"").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_head_cut_off_before_the_blank_line_is_malformed() {
        assert!(matches!(
            read(b"GET / HTTP/1.1\r\nHost: x").await,
            Err(HttpError::Malformed(_))
        ));
    }

    #[tokio::test]
    async fn bare_newlines_are_tolerated() {
        let request = read(b"GET / HTTP/1.1\nHost: x\n\n").await.unwrap().unwrap();
        assert_eq!(request.header("Host"), Some("x"));
    }

    #[tokio::test]
    async fn a_body_over_the_bound_is_refused_before_it_is_read() {
        let head = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        assert!(matches!(
            read(head.as_bytes()).await,
            Err(HttpError::TooLarge)
        ));
    }

    #[tokio::test]
    async fn an_oversized_head_is_refused() {
        let mut head = b"GET / HTTP/1.1\r\n".to_vec();
        while head.len() <= MAX_HEAD_BYTES {
            head.extend_from_slice(
                b"X-Pad: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n",
            );
        }
        assert!(matches!(read(&head).await, Err(HttpError::TooLarge)));
    }

    #[tokio::test]
    async fn chunked_bodies_are_rejected_rather_than_misread() {
        assert!(matches!(
            read(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n")
                .await,
            Err(HttpError::Malformed(_))
        ));
    }

    #[test]
    fn parse_head_rejects_the_shapes_it_does_not_understand() {
        for head in [
            "",
            "GET",
            "GET /",
            "GET / SPDY/3",
            "GET / HTTP/1.1 extra",
            "GET http://example.com/ HTTP/1.1",
            "GET / HTTP/1.1\r\nno colon here",
            "GET / HTTP/1.1\r\nBad Name: x",
        ] {
            assert!(parse_head(head).is_err(), "{head:?} should be rejected");
        }
    }

    #[test]
    fn a_response_serializes_with_the_headers_the_page_relies_on() {
        let response = Response::text(404, "nope");
        let bytes = String::from_utf8(response.to_bytes()).unwrap();
        assert!(bytes.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(bytes.contains("Content-Length: 4\r\n"));
        assert!(bytes.contains("Connection: close\r\n"));
        assert!(bytes.contains("Cache-Control: no-store\r\n"));
        assert!(bytes.ends_with("\r\n\r\nnope"));
    }

    #[test]
    fn json_errors_have_the_one_shape_the_page_shows() {
        let response = Response::error(400, "bad");
        assert_eq!(response.body, br#"{"error":"bad"}"#);
        assert!(response.content_type.starts_with("application/json"));
    }
}

//! Minimal blocking HTTP/1.1 client for plain `http://` admin binds.
//!
//! Admin listeners are private; TLS is not required. Avoids pulling a TLS
//! stack into the ops tool.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use thiserror::Error;

/// HTTP client error.
#[derive(Debug, Error)]
pub enum HttpError {
    /// Bad URL shape.
    #[error("url: {0}")]
    Url(String),
    /// I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Non-success status.
    #[error("HTTP {status}: {body}")]
    Status {
        /// Status code.
        status: u16,
        /// Response body (truncated).
        body: String,
    },
    /// Response could not be parsed.
    #[error("response: {0}")]
    Response(String),
}

/// Parsed absolute `http://host:port/path` URL.
#[derive(Clone, Debug)]
pub struct HttpUrl {
    /// Host to connect to.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Path + query (starts with `/`).
    pub path: String,
}

impl HttpUrl {
    /// Parse `http://host[:port][/path…]`.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError::Url`] when the scheme is not `http` or the host is missing.
    pub fn parse(raw: &str) -> Result<Self, HttpError> {
        let rest = raw
            .strip_prefix("http://")
            .ok_or_else(|| HttpError::Url("only http:// is supported".into()))?;
        let (authority, path) = match rest.split_once('/') {
            Some((a, p)) => (a, format!("/{p}")),
            None => (rest, "/".into()),
        };
        if authority.is_empty() {
            return Err(HttpError::Url("missing host".into()));
        }
        let (host, port) = if let Some((h, p)) = authority.rsplit_once(':') {
            let port: u16 = p
                .parse()
                .map_err(|_| HttpError::Url(format!("bad port `{p}`")))?;
            (h.to_owned(), port)
        } else {
            (authority.to_owned(), 80)
        };
        Ok(Self { host, port, path })
    }

    /// Join a relative path onto this base (base path is ignored; only host/port).
    #[must_use]
    pub fn with_path(&self, path: &str) -> Self {
        let path = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        Self {
            host: self.host.clone(),
            port: self.port,
            path,
        }
    }

    /// `Host` header value.
    #[must_use]
    pub fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Auth material attached to every request.
#[derive(Clone, Debug, Default)]
pub struct Auth {
    /// Optional `Authorization: Bearer …`.
    pub bearer: Option<String>,
    /// Session cookie value for `room_admin_sid`.
    pub session: Option<String>,
    /// Pending MFA cookie value for `room_admin_pending`.
    pub pending: Option<String>,
    /// CSRF token for mutating calls.
    pub csrf: Option<String>,
}

/// One HTTP response.
#[derive(Clone, Debug)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Raw headers (lowercased names).
    pub headers: Vec<(String, String)>,
    /// Body bytes.
    pub body: Vec<u8>,
}

impl Response {
    /// Body as UTF-8 lossy string.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// First `Set-Cookie` name=value pairs we care about.
    #[must_use]
    pub fn set_cookies(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, value) in &self.headers {
            if name != "set-cookie" {
                continue;
            }
            if let Some((nv, _)) = value.split_once(';') {
                if let Some((n, v)) = nv.split_once('=') {
                    out.push((n.trim().to_owned(), v.trim().to_owned()));
                }
            }
        }
        out
    }
}

/// Perform an HTTP/1.1 request and return the full response.
///
/// # Errors
///
/// Connection, parse, or transport failures.
pub fn request(
    url: &HttpUrl,
    method: &str,
    auth: &Auth,
    content_type: Option<&str>,
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let mut stream = TcpStream::connect((url.host.as_str(), url.port))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: application/json, text/plain, */*\r\nUser-Agent: room-admin-tui/0.1\r\n",
        path = url.path,
        host = url.host_header(),
    );
    if let Some(token) = &auth.bearer {
        req.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    let mut cookies = Vec::new();
    if let Some(sid) = &auth.session {
        cookies.push(format!("room_admin_sid={sid}"));
    }
    if let Some(pending) = &auth.pending {
        cookies.push(format!("room_admin_pending={pending}"));
    }
    if !cookies.is_empty() {
        req.push_str(&format!("Cookie: {}\r\n", cookies.join("; ")));
    }
    if let Some(csrf) = &auth.csrf {
        req.push_str(&format!("X-CSRF-Token: {csrf}\r\n"));
    }
    if let Some(ct) = content_type {
        req.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    let body = body.unwrap_or(&[]);
    req.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));

    stream.write_all(req.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(body)?;
    }

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    parse_response(&buf)
}

fn parse_response(raw: &[u8]) -> Result<Response, HttpError> {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| HttpError::Response("missing header terminator".into()))?;
    let head = std::str::from_utf8(&raw[..sep])
        .map_err(|_| HttpError::Response("headers are not utf-8".into()))?;
    let body = raw[sep + 4..].to_vec();
    let mut lines = head.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| HttpError::Response("empty status line".into()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| HttpError::Response(format!("bad status line `{status_line}`")))?;
    let mut headers = Vec::new();
    for line in lines {
        if let Some((n, v)) = line.split_once(':') {
            headers.push((n.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    // Honour Content-Length when present (trim trailing bytes from keep-alive leftovers).
    if let Some((_, cl)) = headers.iter().find(|(n, _)| n == "content-length") {
        if let Ok(n) = cl.parse::<usize>() {
            let mut body = body;
            if body.len() > n {
                body.truncate(n);
            }
            return Ok(Response {
                status,
                headers,
                body,
            });
        }
    }
    Ok(Response {
        status,
        headers,
        body,
    })
}

/// GET JSON; errors on non-2xx.
///
/// # Errors
///
/// Transport or HTTP status failures.
pub fn get_json<T: serde::de::DeserializeOwned>(
    url: &HttpUrl,
    auth: &Auth,
) -> Result<T, HttpError> {
    let res = request(url, "GET", auth, None, None, Duration::from_secs(5))?;
    if !(200..300).contains(&res.status) {
        return Err(HttpError::Status {
            status: res.status,
            body: truncate(&res.text(), 240),
        });
    }
    serde_json::from_slice(&res.body).map_err(|e| HttpError::Response(e.to_string()))
}

/// GET text body; errors on non-2xx.
///
/// # Errors
///
/// Transport or HTTP status failures.
pub fn get_text(url: &HttpUrl, auth: &Auth) -> Result<String, HttpError> {
    let res = request(url, "GET", auth, None, None, Duration::from_secs(5))?;
    if !(200..300).contains(&res.status) {
        return Err(HttpError::Status {
            status: res.status,
            body: truncate(&res.text(), 240),
        });
    }
    Ok(res.text())
}

/// POST JSON body; returns raw response (caller inspects status / cookies).
///
/// # Errors
///
/// Transport failures only.
pub fn post_json(
    url: &HttpUrl,
    auth: &Auth,
    body: &serde_json::Value,
) -> Result<Response, HttpError> {
    let bytes = serde_json::to_vec(body).map_err(|e| HttpError::Response(e.to_string()))?;
    request(
        url,
        "POST",
        auth,
        Some("application/json"),
        Some(&bytes),
        Duration::from_secs(5),
    )
}

/// POST with empty body; errors on non-2xx (204 ok).
///
/// # Errors
///
/// Transport or HTTP status failures.
pub fn post_empty(url: &HttpUrl, auth: &Auth) -> Result<(), HttpError> {
    let res = request(url, "POST", auth, None, Some(&[]), Duration::from_secs(5))?;
    if !(200..300).contains(&res.status) {
        return Err(HttpError::Status {
            status: res.status,
            body: truncate(&res.text(), 240),
        });
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let mut t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        t.push('…');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_urls() {
        let Ok(u) = HttpUrl::parse("http://127.0.0.1:8766/admin") else {
            panic!("url");
        };
        assert_eq!(u.host, "127.0.0.1");
        assert_eq!(u.port, 8766);
        assert_eq!(u.path, "/admin");
        let u2 = u.with_path("/admin/api/v1/summary");
        assert_eq!(u2.path, "/admin/api/v1/summary");
    }

    #[test]
    fn parses_response_with_content_length() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhelloEXTRA";
        let Ok(res) = parse_response(raw) else {
            panic!("parse");
        };
        assert_eq!(res.status, 200);
        assert_eq!(res.body, b"hello");
    }
}

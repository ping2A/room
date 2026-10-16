//! Admin MFA (password + TOTP) and bearer fallback.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_server::{AuthStore, RoomRegistry, ServerConfig, bind_ephemeral};

async fn http(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> (u16, String, Vec<String>) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(c) = cookie {
        req.push_str(&format!("Cookie: {c}\r\n"));
    }
    if let Some(t) = csrf {
        req.push_str(&format!("X-CSRF-Token: {t}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf);
    let status = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let cookies: Vec<String> = text
        .lines()
        .take_while(|l| !l.is_empty())
        .filter(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .map(|l| l["set-cookie:".len()..].trim().to_owned())
        .collect();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body, cookies)
}

fn cookie_jar(set: &[String]) -> String {
    set.iter()
        .filter_map(|c| c.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

#[tokio::test]
async fn mfa_totp_session_and_csrf() {
    let dir = std::env::temp_dir().join(format!("room-admin-mfa-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let auth = AuthStore::open(&dir, "localhost", "http://127.0.0.1:9", false).unwrap();
    auth.bootstrap_user("alice", "correct horse battery staple")
        .unwrap();

    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        data_dir: dir.to_string_lossy().into(),
        admin_rp_id: "localhost".into(),
        admin_rp_origin: "http://127.0.0.1:9".into(),
        ..ServerConfig::default()
    });
    // Ensure the ephemeral server loads the same admins.json.
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();

    let (st, body, _) = http(addr, "GET", "/admin/api/v1/summary", "", None, None).await;
    assert_eq!(st, 401, "{body}");

    let (st, body, cookies) = http(
        addr,
        "POST",
        "/admin/api/v1/auth/login",
        r#"{"username":"alice","password":"correct horse battery staple"}"#,
        None,
        None,
    )
    .await;
    assert_eq!(st, 200, "{body}");
    assert!(body.contains("\"via\":\"password\""), "{body}");
    let csrf = serde_json::from_str::<serde_json::Value>(&body).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    let session = cookie_jar(&cookies);
    assert!(session.contains("room_admin_sid"), "{session}");

    let (st, body, _) = http(
        addr,
        "GET",
        "/admin/api/v1/summary",
        "",
        Some(&session),
        None,
    )
    .await;
    assert_eq!(st, 200, "{body}");
    assert!(body.contains("\"rooms\":"), "{body}");

    // Mutating without CSRF must fail.
    let (st, _, _) = http(
        addr,
        "POST",
        "/admin/api/v1/maintenance",
        r#"{"enabled":false}"#,
        Some(&session),
        None,
    )
    .await;
    assert_eq!(st, 403);

    let (st, _, _) = http(
        addr,
        "POST",
        "/admin/api/v1/maintenance",
        r#"{"enabled":false}"#,
        Some(&session),
        Some(&csrf),
    )
    .await;
    assert_eq!(st, 204);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn bearer_still_works_without_users() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        admin_token: "secret".into(),
        ..ServerConfig::default()
    });
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let (st, body, _) = http(addr, "GET", "/admin/api/v1/summary", "", None, None).await;
    assert_eq!(st, 401, "{body}");

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let req = "GET /admin/api/v1/summary HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer secret\r\nConnection: close\r\n\r\n";
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("200"), "{text}");
    assert!(text.contains("\"rooms\":0"), "{text}");
}

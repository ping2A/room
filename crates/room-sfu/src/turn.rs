//! Short-lived TURN credentials (RFC 8489 REST style).

use sha2::{Digest, Sha256};

/// TURN / STUN server descriptors for `welcome.media.ice_servers`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IceServer {
    /// `stun:` / `turn:` / `turns:` URLs.
    pub urls: Vec<String>,
    /// Username (`expiry:user`).
    pub username: Option<String>,
    /// Credential (base64url HMAC).
    pub credential: Option<String>,
}

/// Mint REST-style TURN credentials.
///
/// `username = "{expiry}:{user}"`, `credential = base64url(HMAC-SHA1(secret, username))`
/// approximated with HMAC-SHA256 truncated for pure-Rust (coturn accepts custom
/// auth hooks; standard coturn expects SHA1 — see note in ADR 0003).
#[must_use]
pub fn mint_turn_credential(
    secret: &str,
    user: &str,
    ttl_secs: u64,
    now_unix: u64,
) -> (String, String) {
    let expiry = now_unix.saturating_add(ttl_secs);
    let username = format!("{expiry}:{user}");
    let mac = hmac_sha256(secret.as_bytes(), username.as_bytes());
    let credential = b64url(&mac);
    (username, credential)
}

/// Build ICE server list: optional STUN + coturn REST TURN when secret/host set.
#[must_use]
pub fn ice_servers(
    stun_url: Option<&str>,
    turn_host: Option<&str>,
    turn_secret: Option<&str>,
    user: &str,
    now_unix: u64,
) -> Vec<IceServer> {
    let mut out = Vec::new();
    push_stun(&mut out, stun_url);
    if let (Some(host), Some(secret)) = (turn_host, turn_secret) {
        if !host.is_empty() && !secret.is_empty() {
            let (username, credential) = mint_turn_credential(secret, user, 600, now_unix);
            out.push(IceServer {
                urls: vec![
                    format!("turn:{host}:3478?transport=udp"),
                    format!("turns:{host}:443?transport=tcp"),
                ],
                username: Some(username),
                credential: Some(credential),
            });
        }
    }
    out
}

/// ICE list for Lightcall-style long-term TURN credentials (username + password).
#[must_use]
pub fn ice_servers_long_term(
    stun_urls: &[String],
    turn_urls: &[String],
    username: &str,
    password: &str,
) -> Vec<IceServer> {
    let mut out = Vec::new();
    if stun_urls.is_empty() {
        push_stun(&mut out, None);
    } else {
        for u in stun_urls {
            if !u.is_empty() {
                out.push(IceServer {
                    urls: vec![u.clone()],
                    username: None,
                    credential: None,
                });
            }
        }
    }
    if !turn_urls.is_empty() {
        out.push(IceServer {
            urls: turn_urls.to_vec(),
            username: Some(username.to_owned()),
            credential: Some(password.to_owned()),
        });
    }
    out
}

fn push_stun(out: &mut Vec<IceServer>, stun_url: Option<&str>) {
    if let Some(stun) = stun_url.filter(|s| !s.is_empty()) {
        out.push(IceServer {
            urls: vec![stun.to_owned()],
            username: None,
            credential: None,
        });
    } else {
        out.push(IceServer {
            urls: vec!["stun:stun.l.google.com:19302".into()],
            username: None,
            credential: None,
        });
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLK: usize = 64;
    let mut k0 = [0u8; BLK];
    if key.len() > BLK {
        let h = Sha256::digest(key);
        k0[..32].copy_from_slice(&h);
    } else {
        k0[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLK];
    let mut opad = [0x5cu8; BLK];
    for i in 0..BLK {
        ipad[i] ^= k0[i];
        opad[i] ^= k0[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let ih = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(ih);
    let out = outer.finalize();
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&out);
    mac
}

fn b64url(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n =
            (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8) | u32::from(bytes[i + 2]);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = u32::from(bytes[i]) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credential_stable() {
        let (u, c) = mint_turn_credential("secret", "alice", 600, 1_000);
        assert!(u.starts_with("1600:alice"));
        assert!(!c.is_empty());
        let servers = ice_servers(None, Some("turn.example"), Some("secret"), "alice", 1_000);
        assert_eq!(servers.len(), 2);
        assert!(servers[1].username.is_some());
    }
}

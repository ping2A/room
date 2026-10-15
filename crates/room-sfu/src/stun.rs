//! Minimal RFC 5389 STUN server (Binding only).
//!
//! Copied from Lightcall `lightcall-stun` for Room WebRTC NAT discovery
//! (`docs/22` §8). Pure Rust — no C/C++.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

const MAGIC_COOKIE: u32 = 0x2112_A442;
const MSG_BINDING_REQUEST: u16 = 0x0001;
const MSG_BINDING_RESPONSE: u16 = 0x0101;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const FAMILY_IPV4: u8 = 0x01;
const STUN_HEADER_LEN: usize = 20;

/// Run a STUN server until the task is cancelled / process exits.
///
/// # Errors
///
/// Bind failures.
pub async fn run(bind_addr: SocketAddr) -> std::io::Result<()> {
    let socket = UdpSocket::bind(bind_addr).await?;
    info!(%bind_addr, "STUN listening (Lightcall binding server)");
    let socket = Arc::new(socket);
    loop {
        let mut buf = [0u8; 512];
        let (n, peer) = match socket.recv_from(&mut buf).await {
            Ok(x) => x,
            Err(e) => {
                warn!("STUN recv error: {e}");
                continue;
            }
        };
        let socket = Arc::clone(&socket);
        let buf = buf[..n].to_vec();
        tokio::spawn(async move {
            if let Some(response) = handle_binding_request(&buf, peer) {
                match socket.send_to(&response, peer).await {
                    Ok(_) => debug!("STUN: client from {peer}"),
                    Err(e) => warn!("STUN: send to {peer} failed: {e}"),
                }
            }
        });
    }
}

fn handle_binding_request(req: &[u8], client_addr: SocketAddr) -> Option<Vec<u8>> {
    if req.len() < STUN_HEADER_LEN {
        return None;
    }
    let msg_type = u16::from_be_bytes([req[0], req[1]]);
    if msg_type != MSG_BINDING_REQUEST {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([req[2], req[3]]));
    let cookie = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
    if cookie != MAGIC_COOKIE {
        return None;
    }
    let transaction_id = &req[8..20];
    if req.len() < STUN_HEADER_LEN + length {
        return None;
    }
    build_binding_response(transaction_id, client_addr)
}

fn build_binding_response(transaction_id: &[u8], peer: SocketAddr) -> Option<Vec<u8>> {
    let (port_xor, addr_bytes) = match peer {
        SocketAddr::V4(v4) => {
            let port = peer.port();
            let port_xor = (port ^ (MAGIC_COOKIE >> 16) as u16).to_be_bytes();
            let ip = v4.ip().octets();
            let ip_u32 = u32::from_be_bytes(ip) ^ MAGIC_COOKIE;
            (port_xor, ip_u32.to_be_bytes())
        }
        SocketAddr::V6(_) => return None,
    };
    let attr_value_len = 4 + 2 + addr_bytes.len();
    let attr_len = 2 + 2 + attr_value_len;
    let mut out = Vec::with_capacity(STUN_HEADER_LEN + attr_len);
    out.extend_from_slice(&MSG_BINDING_RESPONSE.to_be_bytes());
    out.extend_from_slice(&(attr_len as u16).to_be_bytes());
    out.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    out.extend_from_slice(transaction_id);
    out.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    out.extend_from_slice(&(attr_value_len as u16).to_be_bytes());
    out.push(0);
    out.push(FAMILY_IPV4);
    out.extend_from_slice(&port_xor);
    out.extend_from_slice(&addr_bytes);
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn binding_response_format() {
        let tid = [1u8; 12];
        let peer = SocketAddr::from((Ipv4Addr::new(192, 168, 1, 100), 54321));
        let resp = build_binding_response(&tid, peer).unwrap();
        assert!(resp.len() >= STUN_HEADER_LEN + 8);
        assert_eq!(resp[0..2], MSG_BINDING_RESPONSE.to_be_bytes());
        assert_eq!(resp[4..8], MAGIC_COOKIE.to_be_bytes());
        assert_eq!(resp[8..20], tid);
    }
}

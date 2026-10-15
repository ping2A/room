//! Built-in TURN relay (copied from Lightcall `turn_relay.rs`).
//!
//! Uses the pure-Rust `turn` crate (webrtc-rs). Long-term credentials via
//! `generate_auth_key`; per-client entries live in a shared [`DashMap`].

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{debug, info, warn};
use turn::Error as TurnError;
use turn::allocation::AllocationInfo;
use turn::auth::{AuthHandler, generate_auth_key};
use turn::relay::relay_static::RelayAddressGeneratorStatic;
use turn::server::Server;
use turn::server::config::{ConnConfig, ServerConfig};
use webrtc_util::vnet::net::Net;

/// TURN relay configuration.
#[derive(Clone, Debug)]
pub struct TurnRelayConfig {
    /// UDP bind (e.g. `0.0.0.0:3479`).
    pub bind_addr: SocketAddr,
    /// Public IP advertised in relay candidates.
    pub relay_public_ip: IpAddr,
    /// TURN realm (default `room`).
    pub realm: String,
    /// Shared username/password (dev / external static).
    pub auth_static: Option<(String, String)>,
    /// Per-client auth keys (`username` → `generate_auth_key` output).
    pub auth_dynamic_registry: Option<Arc<DashMap<String, Vec<u8>>>>,
}

/// Mint a long-term TURN credential and insert it into `registry`.
///
/// Returns `(username, password)` for `RTCPeerConnection` `iceServers`.
#[must_use]
pub fn issue_turn_user(
    registry: &DashMap<String, Vec<u8>>,
    realm: &str,
    username: String,
    password: String,
) -> (String, String) {
    let key = generate_auth_key(&username, realm, &password);
    registry.insert(username.clone(), key);
    (username, password)
}

/// Remove a previously issued username.
pub fn revoke_turn_user(registry: &DashMap<String, Vec<u8>>, username: &str) {
    registry.remove(username);
}

#[derive(Clone)]
struct StaticAuthHandler {
    realm: String,
    keys: HashMap<String, Vec<u8>>,
}

impl StaticAuthHandler {
    fn new(username: String, realm: String, password: String) -> Self {
        let mut keys = HashMap::new();
        let key = generate_auth_key(&username, &realm, &password);
        keys.insert(username, key);
        Self { realm, keys }
    }
}

impl AuthHandler for StaticAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
    ) -> Result<Vec<u8>, TurnError> {
        if realm != self.realm {
            warn!(
                "TURN: auth rejected from {src_addr} (realm mismatch got={realm} want={})",
                self.realm
            );
            return Err(TurnError::ErrNoSuchUser);
        }
        match self.keys.get(username) {
            Some(key) => Ok(key.clone()),
            None => Err(TurnError::ErrNoSuchUser),
        }
    }
}

#[derive(Clone)]
struct DynamicAuthHandler {
    realm: String,
    registry: Arc<DashMap<String, Vec<u8>>>,
}

impl AuthHandler for DynamicAuthHandler {
    fn auth_handle(
        &self,
        username: &str,
        realm: &str,
        src_addr: SocketAddr,
    ) -> Result<Vec<u8>, TurnError> {
        if realm != self.realm {
            warn!("TURN: auth rejected from {src_addr} (realm mismatch)");
            return Err(TurnError::ErrNoSuchUser);
        }
        match self.registry.get(username) {
            Some(key) => {
                debug!("TURN: client from {src_addr}");
                Ok(key.clone())
            }
            None => Err(TurnError::ErrNoSuchUser),
        }
    }
}

/// Run the TURN relay until the process exits.
///
/// # Errors
///
/// Bind / TURN server construction failures.
pub async fn run_turn_relay(cfg: TurnRelayConfig) -> Result<(), TurnError> {
    let conn = Arc::new(UdpSocket::bind(cfg.bind_addr).await?);
    info!(%cfg.bind_addr, "TURN listening (Lightcall relay)");

    let (alloc_tx, mut alloc_rx) = mpsc::channel::<AllocationInfo>(1024);
    tokio::spawn(async move {
        while let Some(info) = alloc_rx.recv().await {
            debug!(
                "TURN: allocation closed user='{}' relay={}",
                info.username, info.relay_addr
            );
        }
    });

    let auth_handler: Arc<dyn AuthHandler + Send + Sync> =
        if let Some(registry) = cfg.auth_dynamic_registry {
            Arc::new(DynamicAuthHandler {
                realm: cfg.realm.clone(),
                registry,
            })
        } else if let Some((username, password)) = cfg.auth_static {
            Arc::new(StaticAuthHandler::new(
                username,
                cfg.realm.clone(),
                password,
            ))
        } else {
            panic!("TURN: auth_static or auth_dynamic_registry required");
        };

    let _server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: cfg.relay_public_ip,
                address: "0.0.0.0".to_string(),
                net: Arc::new(Net::new(None)),
            }),
        }],
        realm: cfg.realm.clone(),
        auth_handler,
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: Some(alloc_tx),
    })
    .await?;

    std::future::pending::<()>().await;
    Ok(())
}

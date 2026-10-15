//! Runtime configuration.

use dashmap::DashMap;
use std::sync::Arc;

/// Lightcall-style built-in TURN credential registry (long-term auth).
#[derive(Clone)]
pub struct TurnAuth {
    /// `username` → TURN auth key bytes.
    pub registry: Arc<DashMap<String, Vec<u8>>>,
    /// STUN URLs advertised in `welcome.media`.
    pub stun_urls: Vec<String>,
    /// TURN URLs advertised in `welcome.media`.
    pub turn_urls: Vec<String>,
    /// TURN realm.
    pub realm: String,
}

impl std::fmt::Debug for TurnAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnAuth")
            .field("stun_urls", &self.stun_urls)
            .field("turn_urls", &self.turn_urls)
            .field("realm", &self.realm)
            .field("issued", &self.registry.len())
            .finish()
    }
}

/// Server listen / capacity settings.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Bind address (`host:port`).
    pub bind: String,
    /// Admin UI/API bind (`host:port`). Empty = serve `/admin` on [`Self::bind`].
    pub admin_bind: String,
    /// Browser game client origin for lobby deep-links (e.g. `http://127.0.0.1:8080/play`).
    pub play_base: String,
    /// Directory of built `room-web` assets served at `/play/` (empty = off).
    pub web_root: String,
    /// Stable id for this process (multi-node routing / tickets).
    pub node_id: String,
    /// Max rooms on this node.
    pub max_rooms: usize,
    /// Default max players for a new room.
    pub default_max_players: u8,
    /// Max WebSocket message size (bytes).
    pub max_message_size: usize,
    /// Outbound queue capacity per player.
    pub player_out_capacity: usize,
    /// Room command channel capacity.
    pub room_cmd_capacity: usize,
    /// Jitter grace past a tic's 35 Hz slot before a late seat is filled (v2).
    pub stall_fill_ms: u64,
    /// Secret used to mint join tickets (HMAC key material).
    pub ticket_secret: String,
    /// Allowed `Origin` values for WebSocket upgrade; empty = allow any (dev).
    pub allowed_origins: Vec<String>,
    /// Require a minted join ticket for v2 hello (M2+). Legacy FTRL ignores this.
    pub require_tickets: bool,
    /// Message token-bucket burst.
    pub msg_burst: f64,
    /// Message token-bucket refill per second.
    pub msg_refill_per_sec: f64,
    /// Inbound bytes token-bucket burst (docs/22 §5: 16 KiB).
    pub byte_burst: f64,
    /// Inbound bytes refill per second.
    pub byte_refill_per_sec: f64,
    /// Chat message burst (docs/22 §5: 5).
    pub chat_burst: f64,
    /// Chat messages refill per second (docs/22 §5: 1).
    pub chat_refill_per_sec: f64,
    /// Max concurrent WebSocket connections per client IP (0 = unlimited).
    pub max_conn_per_ip: usize,
    /// Bearer token for `/admin/*` (empty = allow all — local/dev only).
    pub admin_token: String,
    /// Directory for persisted audit.jsonl + bans.json (empty = memory only).
    pub data_dir: String,
    /// STUN URL for `welcome.media` (empty = Google public STUN, or built-in when `stun_bind` set).
    pub stun_url: String,
    /// TURN host (no scheme); empty disables coturn REST credentials.
    pub turn_host: String,
    /// Coturn REST shared secret for short-lived TURN creds.
    pub turn_secret: String,
    /// Bind for built-in Lightcall STUN (e.g. `0.0.0.0:3478`); empty = off.
    pub stun_bind: String,
    /// Bind for built-in Lightcall TURN (e.g. `0.0.0.0:3479`); empty = off.
    pub turn_bind: String,
    /// Public IP for TURN relay candidates (required when `turn_bind` is `0.0.0.0`).
    pub turn_public_ip: String,
    /// Hostname/IP used in advertised `stun:` / `turn:` URLs.
    pub turn_public_host: String,
    /// TURN realm for built-in relay (default `room`).
    pub turn_realm: String,
    /// Populated when built-in TURN is enabled (`feature = builtin-turn`).
    pub turn_auth: Option<TurnAuth>,
    /// UDP bind for in-process SFU (`host:port`); empty = signalling-only.
    pub sfu_udp: String,
    /// Default: allow creating rooms with the optional headless verifier.
    pub enable_verifier: bool,
    /// Cap on concurrent verified rooms on this node.
    pub max_verified_rooms: usize,
    /// Seconds between drain notice and forced close (admin drain).
    pub drain_grace_secs: u64,
    /// IWAD path for the optional Headless verifier (empty = verifier cannot start).
    pub iwad_path: String,
    /// Seconds to wait for the first client frame (hello) before close.
    pub handshake_timeout_secs: u64,
    /// Seconds without any frame before idle close (clients should ping).
    pub idle_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8765".into(),
            admin_bind: String::new(),
            play_base: "http://127.0.0.1:8080".into(),
            web_root: String::new(),
            node_id: "local".into(),
            max_rooms: 5_000,
            default_max_players: 2,
            // WebRTC SDP + control JSON; keep in sync with room_proto::MAX_CONTROL_BYTES.
            max_message_size: 65_536,
            player_out_capacity: 64,
            room_cmd_capacity: 256,
            stall_fill_ms: 30,
            ticket_secret: "room-dev-ticket-secret".into(),
            allowed_origins: Vec::new(),
            require_tickets: false,
            msg_burst: 120.0,
            msg_refill_per_sec: 90.0,
            byte_burst: 16_384.0,
            byte_refill_per_sec: 16_384.0,
            chat_burst: 5.0,
            chat_refill_per_sec: 1.0,
            max_conn_per_ip: 16,
            admin_token: String::new(),
            data_dir: String::new(),
            stun_url: String::new(),
            turn_host: String::new(),
            turn_secret: String::new(),
            stun_bind: String::new(),
            turn_bind: String::new(),
            turn_public_ip: String::new(),
            turn_public_host: String::new(),
            turn_realm: "room".into(),
            turn_auth: None,
            sfu_udp: String::new(),
            enable_verifier: false,
            max_verified_rooms: 200,
            drain_grace_secs: 60,
            iwad_path: String::new(),
            handshake_timeout_secs: 5,
            idle_timeout_secs: 30,
        }
    }
}

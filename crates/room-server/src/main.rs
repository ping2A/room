//! `room-server` binary — multi-room WebSocket game host.

use clap::Parser;
use room_server::{RoomRegistry, ServerConfig, TurnAuth, serve};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "room-server", about = "Room multi-room web coop server")]
struct Args {
    /// Bind address (`host:port`).
    #[arg(long, default_value = "127.0.0.1:8765")]
    bind: String,
    /// Admin UI/API bind (`host:port`). Empty = serve `/admin` on `--bind`.
    #[arg(long, default_value = "")]
    admin_bind: String,
    /// Browser game client base URL for lobby Play / Watch links.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    play_base: String,
    /// Serve a built `room-web` directory at `/play/` (same-origin joins).
    #[arg(long, default_value = "")]
    web_root: String,
    /// Max concurrent rooms.
    #[arg(long, default_value_t = 5_000)]
    max_rooms: usize,
    /// Secret used to mint HMAC join tickets.
    #[arg(long, default_value = "room-dev-ticket-secret")]
    ticket_secret: String,
    /// Allowed browser Origins (repeatable). Empty = allow any (dev).
    #[arg(long = "origin")]
    origins: Vec<String>,
    /// Require minted join tickets for v2 hello (reject bare room ids).
    #[arg(long, default_value_t = false)]
    require_tickets: bool,
    /// Jitter grace (ms) past a tic's 35 Hz slot before a late seat's input is filled (v2).
    #[arg(long, default_value_t = 30)]
    stall_fill_ms: u64,
    /// Max concurrent WebSocket connections per client IP (0 = unlimited).
    #[arg(long, default_value_t = 16)]
    max_conn_per_ip: usize,
    /// Bearer token required for `/admin/*` (empty = open, for local/dev).
    #[arg(long, default_value = "")]
    admin_token: String,
    /// Persist audit.jsonl + bans.json under this directory (empty = memory only).
    #[arg(long, default_value = "")]
    data_dir: String,
    /// STUN URL for WebRTC (empty = built-in / Google public STUN).
    #[arg(long, default_value = "")]
    stun_url: String,
    /// External coturn host (e.g. turn.example.com); empty disables REST creds.
    #[arg(long, default_value = "")]
    turn_host: String,
    /// Coturn REST shared secret for short-lived TURN credentials.
    #[arg(long, default_value = "")]
    turn_secret: String,
    /// Bind built-in Lightcall STUN (e.g. `0.0.0.0:3478`); empty = off.
    #[arg(long, default_value = "")]
    stun_bind: String,
    /// Bind built-in Lightcall TURN (e.g. `0.0.0.0:3479`); empty = off.
    #[arg(long, default_value = "")]
    turn_bind: String,
    /// Public IP for TURN relay candidates (`127.0.0.1` for local).
    #[arg(long, default_value = "")]
    turn_public_ip: String,
    /// Host advertised in `stun:` / `turn:` URLs (defaults to public IP / bind host).
    #[arg(long, default_value = "")]
    turn_public_host: String,
    /// TURN realm for built-in relay.
    #[arg(long, default_value = "room")]
    turn_realm: String,
    /// Stable node id for multi-node `room_location` routing.
    #[arg(long, default_value = "local")]
    node_id: String,
    /// UDP bind for in-process SFU (requires `--features rtp`); empty = off.
    #[arg(long, default_value = "")]
    sfu_udp: String,
    /// Allow rooms to opt into the headless verifier (`enable_verifier` on create).
    #[arg(long, default_value_t = false)]
    enable_verifier: bool,
    /// Cap on concurrent verified rooms.
    #[arg(long, default_value_t = 200)]
    max_verified_rooms: usize,
    /// Seconds after drain notice before rooms are force-closed.
    #[arg(long, default_value_t = 60)]
    drain_grace_secs: u64,
    /// IWAD file for Headless verifier (requires `--features verifier`).
    #[arg(long, default_value = "")]
    iwad: String,
    /// Seconds to wait for the first hello frame.
    #[arg(long, default_value_t = 5)]
    handshake_timeout_secs: u64,
    /// Seconds without any frame before idle disconnect.
    #[arg(long, default_value_t = 30)]
    idle_timeout_secs: u64,
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("room_server=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    // When serving room-web from this process, default Play links to same-origin `/play`.
    let play_base = if !args.web_root.is_empty()
        && (args.play_base.is_empty() || args.play_base == "http://127.0.0.1:8080")
    {
        let host = args.bind.replace("0.0.0.0", "127.0.0.1");
        format!("http://{host}/play")
    } else {
        args.play_base
    };
    let turn_public_host = if args.turn_public_host.is_empty() {
        if !args.turn_public_ip.is_empty() {
            args.turn_public_ip.clone()
        } else {
            args.bind
                .split(':')
                .next()
                .unwrap_or("127.0.0.1")
                .replace("0.0.0.0", "127.0.0.1")
        }
    } else {
        args.turn_public_host.clone()
    };
    let stun_urls = if !args.stun_url.is_empty() {
        vec![args.stun_url.clone()]
    } else if !args.stun_bind.is_empty() {
        let port = args.stun_bind.split(':').next_back().unwrap_or("3478");
        vec![format!("stun:{turn_public_host}:{port}")]
    } else {
        Vec::new()
    };
    let turn_urls = if !args.turn_bind.is_empty() {
        let port = args.turn_bind.split(':').next_back().unwrap_or("3479");
        vec![format!("turn:{turn_public_host}:{port}?transport=udp")]
    } else {
        Vec::new()
    };
    let turn_auth = if !args.turn_bind.is_empty() {
        #[cfg(feature = "builtin-turn")]
        {
            Some(TurnAuth {
                registry: Arc::new(dashmap::DashMap::new()),
                stun_urls: stun_urls.clone(),
                turn_urls: turn_urls.clone(),
                realm: args.turn_realm.clone(),
            })
        }
        #[cfg(not(feature = "builtin-turn"))]
        {
            eprintln!("room-server: --turn-bind requires --features builtin-turn");
            None
        }
    } else {
        None
    };
    let cfg = ServerConfig {
        bind: args.bind,
        admin_bind: args.admin_bind,
        play_base,
        web_root: args.web_root,
        node_id: args.node_id,
        max_rooms: args.max_rooms,
        ticket_secret: args.ticket_secret,
        allowed_origins: args.origins,
        require_tickets: args.require_tickets,
        stall_fill_ms: args.stall_fill_ms,
        max_conn_per_ip: args.max_conn_per_ip,
        admin_token: args.admin_token,
        data_dir: args.data_dir,
        stun_url: args.stun_url,
        turn_host: args.turn_host,
        turn_secret: args.turn_secret,
        stun_bind: args.stun_bind,
        turn_bind: args.turn_bind,
        turn_public_ip: args.turn_public_ip,
        turn_public_host,
        turn_realm: args.turn_realm,
        turn_auth,
        sfu_udp: args.sfu_udp,
        enable_verifier: args.enable_verifier,
        max_verified_rooms: args.max_verified_rooms,
        drain_grace_secs: args.drain_grace_secs,
        iwad_path: args.iwad,
        handshake_timeout_secs: args.handshake_timeout_secs,
        idle_timeout_secs: args.idle_timeout_secs,
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    if let Err(e) = serve(registry).await {
        eprintln!("room-server: {e}");
        std::process::exit(1);
    }
}

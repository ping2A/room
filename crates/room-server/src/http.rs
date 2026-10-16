//! HTTP API + WebSocket gateways.

use crate::admin::{admin_routes, prometheus_text};
use crate::admin_auth::AuthStore;
use crate::limits::{TokenBucket, ViolationWindow};
use crate::lobby::{list_public_rooms, lobby_routes};
use crate::registry::{RegistryError, RoomRegistry};
use crate::room::{CreateRoomRequest, CreateRoomResponse, LeaveReason, RoomCmd, RoomHandle};
use crate::security::security_headers_middleware;
use crate::state::AdminStore;
use crate::state::AppState;
use axum::extract::ConnectInfo;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::serve::ListenerExt;
use axum::{Json, Router};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use room_proto::{parse_binary, parse_control};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path as FsPath;
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

/// `POST /api/rooms/{id}/join` body.
#[derive(Clone, Debug, Deserialize)]
pub struct JoinRoomRequest {
    /// Display name.
    pub name: String,
}

/// Join response with a one-time ticket.
#[derive(Clone, Debug, Serialize)]
pub struct JoinRoomResponse {
    /// Opaque join ticket for v2 hello.
    pub ticket: String,
    /// v2 WebSocket path.
    pub ws_v2: String,
    /// Ticket expiry (unix seconds).
    pub exp: u64,
    /// Node that owns the room (multi-node routing).
    pub node_id: String,
}

/// Bind and serve forever.
///
/// # Errors
///
/// Bind / serve failures.
pub async fn serve(registry: RoomRegistry) -> Result<(), std::io::Error> {
    let bind = registry.config().bind.clone();
    let admin_bind = registry.config().admin_bind.clone();
    let admin = if registry.config().data_dir.is_empty() {
        AdminStore::default()
    } else {
        AdminStore::open(&registry.config().data_dir)?
    };
    registry.set_admin(admin.clone());
    let auth = build_auth_store(registry.config())?;
    warn_insecure_defaults(registry.config(), &auth);
    #[cfg(feature = "rtp")]
    if !registry.config().sfu_udp.is_empty() {
        let addr: SocketAddr = registry
            .config()
            .sfu_udp
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        match room_sfu::UdpSfu::bind(addr).await {
            Ok(sfu) => {
                let sfu = std::sync::Arc::new(sfu);
                info!(%addr, "room-sfu UDP up");
                tokio::spawn(sfu.run());
            }
            Err(e) => warn!(?e, "room-sfu UDP bind failed"),
        }
    }
    #[cfg(not(feature = "rtp"))]
    if !registry.config().sfu_udp.is_empty() {
        warn!("--sfu-udp set but room-server built without feature rtp");
    }

    // Lightcall STUN (always available in room-sfu).
    if !registry.config().stun_bind.is_empty() {
        let addr: SocketAddr = registry
            .config()
            .stun_bind
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        tokio::spawn(async move {
            if let Err(e) = room_sfu::run_stun(addr).await {
                warn!(?e, %addr, "STUN server exited");
            }
        });
        info!(%addr, "Lightcall STUN up");
    }

    #[cfg(feature = "builtin-turn")]
    if !registry.config().turn_bind.is_empty() {
        let addr: SocketAddr = registry
            .config()
            .turn_bind
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let public_ip: std::net::IpAddr = {
            let raw = if registry.config().turn_public_ip.is_empty() {
                "127.0.0.1"
            } else {
                registry.config().turn_public_ip.as_str()
            };
            raw.parse()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
        };
        let Some(auth) = registry.config().turn_auth.clone() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "--turn-bind set but turn_auth missing",
            ));
        };
        let realm = auth.realm.clone();
        let registry_keys = auth.registry.clone();
        tokio::spawn(async move {
            let cfg = room_sfu::TurnRelayConfig {
                bind_addr: addr,
                relay_public_ip: public_ip,
                realm,
                auth_static: None,
                auth_dynamic_registry: Some(registry_keys),
            };
            if let Err(e) = room_sfu::run_turn_relay(cfg).await {
                warn!(?e, %addr, "TURN relay exited");
            }
        });
        info!(%addr, %public_ip, "Lightcall TURN up");
    }
    #[cfg(not(feature = "builtin-turn"))]
    if !registry.config().turn_bind.is_empty() {
        warn!("--turn-bind set but room-server built without feature builtin-turn");
    }
    let state = AppState {
        registry,
        admin,
        auth,
    };
    let listener = tokio::net::TcpListener::bind(&bind).await?.tap_io(|tcp| {
        if let Err(e) = tcp.set_nodelay(true) {
            warn!(?e, "TCP_NODELAY failed");
        }
    });
    info!("room-server listening on http://{bind}");

    if !admin_bind.is_empty() {
        let admin_listener = tokio::net::TcpListener::bind(&admin_bind)
            .await?
            .tap_io(|tcp| {
                if let Err(e) = tcp.set_nodelay(true) {
                    warn!(?e, "TCP_NODELAY failed");
                }
            });
        info!("room-server admin listening on http://{admin_bind} (UI /admin)");
        let public = public_router(state.clone());
        let admin_app = admin_router(state);
        let public_serve = axum::serve(
            listener,
            public.into_make_service_with_connect_info::<SocketAddr>(),
        );
        let admin_serve = axum::serve(
            admin_listener,
            admin_app.into_make_service_with_connect_info::<SocketAddr>(),
        );
        tokio::select! {
            r = public_serve => r?,
            r = admin_serve => r?,
        }
        return Ok(());
    }

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

fn apply_http_layers(router: Router<AppState>, state: AppState) -> Router {
    router
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Public game/API router (no admin plane).
pub fn public_router(state: AppState) -> Router {
    let web_root = state.registry.config().web_root.clone();
    apply_http_layers(
        with_web_root(
            Router::new()
                .route("/healthz", get(|| async { "ok" }))
                .route("/readyz", get(readyz))
                .route("/metrics", get(metrics))
                .route("/api/rooms", get(list_public_rooms).post(create_room))
                .route("/api/rooms/{room_id}/join", post(join_room))
                .route("/api/mods", get(list_mods))
                .route("/api/mods/{file}", get(get_mod_file))
                .route("/ws/{room_id}", get(ws_legacy))
                .route("/v2/rooms/{room_id}/ws", get(ws_v2))
                .merge(lobby_routes()),
            &web_root,
        ),
        state,
    )
}

/// Admin UI/API router (docs/22 §7 separate listener).
pub fn admin_router(state: AppState) -> Router {
    apply_http_layers(
        Router::new()
            .route("/healthz", get(|| async { "ok" }))
            // Prometheus text also on admin bind so the SPA can scrape without the game port.
            .route("/metrics", get(metrics))
            .merge(admin_routes()),
        state,
    )
}

/// Combined router (game + admin on one bind). Used by tests and when `--admin-bind` is empty.
pub fn router(state: AppState) -> Router {
    let web_root = state.registry.config().web_root.clone();
    apply_http_layers(
        with_web_root(
            Router::new()
                .route("/healthz", get(|| async { "ok" }))
                .route("/readyz", get(readyz))
                .route("/metrics", get(metrics))
                .route("/api/rooms", get(list_public_rooms).post(create_room))
                .route("/api/rooms/{room_id}/join", post(join_room))
                .route("/api/mods", get(list_mods))
                .route("/api/mods/{file}", get(get_mod_file))
                .route("/ws/{room_id}", get(ws_legacy))
                .route("/v2/rooms/{room_id}/ws", get(ws_v2))
                .merge(lobby_routes())
                .merge(admin_routes()),
            &web_root,
        ),
        state,
    )
}

fn with_web_root(router: Router<AppState>, web_root: &str) -> Router<AppState> {
    if web_root.is_empty() {
        return router;
    }
    let root = FsPath::new(web_root);
    if !root.is_dir() {
        warn!(%web_root, "web_root is not a directory; /play disabled");
        return router;
    }
    let index = root.join("index.html");
    let service = ServeDir::new(root).append_index_html_on_directories(true);
    info!(%web_root, "serving room-web at /play/");
    // Prefer explicit index for `/play` and `/play/`.
    if index.is_file() {
        router
            .route_service("/play", ServeFile::new(&index))
            .nest_service("/play/", service)
    } else {
        router.nest_service("/play/", service)
    }
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    if state.registry.is_draining() {
        (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response()
    } else {
        (StatusCode::OK, "ok").into_response()
    }
}

async fn metrics(State(state): State<AppState>) -> String {
    prometheus_text(&state.registry)
}

fn origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        // Non-browser clients omit Origin; allow when allowlist is set only if
        // empty-origin is not required — browsers always send it.
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    allowed.iter().any(|a| a == origin)
}

fn build_auth_store(cfg: &crate::config::ServerConfig) -> Result<AuthStore, std::io::Error> {
    let auth = if cfg.data_dir.is_empty() {
        AuthStore::memory(&cfg.admin_rp_id, &cfg.admin_rp_origin)
    } else {
        AuthStore::open(
            &cfg.data_dir,
            &cfg.admin_rp_id,
            &cfg.admin_rp_origin,
            cfg.admin_secure_cookie,
        )?
    };
    if !cfg.admin_bootstrap.is_empty() {
        if let Some((user, pass)) = cfg.admin_bootstrap.split_once(':') {
            if let Err(e) = auth.bootstrap_user(user, pass) {
                warn!(%e, "admin bootstrap skipped");
            }
        } else {
            warn!("--admin-bootstrap must be user:password");
        }
    }
    Ok(auth)
}

fn warn_insecure_defaults(cfg: &crate::config::ServerConfig, auth: &AuthStore) {
    if auth.has_users() {
        info!(
            users = auth.user_count(),
            rp = %cfg.admin_rp_id,
            origin = %cfg.admin_rp_origin,
            "admin MFA enabled (password + TOTP / WebAuthn)"
        );
    } else if cfg.admin_token.is_empty() {
        warn!(
            "room-server: no admin users and --admin-token is empty — admin API is open (dev only)"
        );
    } else {
        warn!(
            "room-server: admin uses bearer token only — set --admin-bootstrap or enroll MFA users for production"
        );
    }
    if cfg.ticket_secret == "room-dev-ticket-secret" || cfg.ticket_secret.len() < 16 {
        warn!(
            "room-server: weak/default --ticket-secret — set a long random secret for production"
        );
    }
    if cfg.allowed_origins.is_empty() {
        warn!("room-server: no --origin set — any browser Origin is accepted");
    }
    if !cfg.require_tickets {
        warn!(
            "room-server: --require-tickets is off — clients may spoof display names without a join ticket"
        );
    }
}

async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomRequest>,
) -> Result<Json<CreateRoomResponse>, StatusCode> {
    state
        .registry
        .create_room(req)
        .map(Json)
        .map_err(|e| match e {
            RegistryError::Full | RegistryError::Draining | RegistryError::VerifierFull => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            RegistryError::UnknownMod => StatusCode::BAD_REQUEST,
            RegistryError::ModNotAllowed => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        })
}

/// `GET /api/mods` — mods rooms can be created with.
async fn list_mods(State(state): State<AppState>) -> Json<Vec<crate::mods::ModListing>> {
    Json(state.registry.config().mods.listing())
}

/// `GET /api/mods/{name}.wad` — mod file for clients (public, cacheable; CORS
/// open so a client served from another origin can fetch it).
async fn get_mod_file(
    State(state): State<AppState>,
    Path(file): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let name = file.strip_suffix(".wad").ok_or(StatusCode::NOT_FOUND)?;
    let entry = state
        .registry
        .config()
        .mods
        .get(name)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_owned()),
            (header::CACHE_CONTROL, "public, max-age=300".to_owned()),
            (header::ETAG, format!("\"{}\"", entry.info.fingerprint)),
        ],
        entry.bytes.clone(),
    ))
}

async fn join_room(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<JoinRoomRequest>,
) -> Result<Json<JoinRoomResponse>, StatusCode> {
    let Some(name) = crate::names::sanitize_display_name(&req.name) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let ip = addr.ip().to_string();
    if let Some(hit) = state.admin.ban_hit(&crate::bans::BanSubject {
        ip: &ip,
        name: &name,
        ..Default::default()
    }) {
        warn!(ban = %hit.id, scope = %hit.scope, "join refused (banned)");
        return Err(StatusCode::FORBIDDEN);
    }
    // Multi-node: refuse join when the room lives elsewhere (caller should redirect).
    if let Some(owner) = state.registry.locate_room(&room_id) {
        if owner != state.registry.config().node_id {
            return Err(StatusCode::MISDIRECTED_REQUEST);
        }
    }
    let (ticket, claims) =
        state
            .registry
            .issue_join_ticket(&room_id, &name)
            .map_err(|e| match e {
                RegistryError::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            })?;
    let node = state.registry.config().node_id.clone();
    Ok(Json(JoinRoomResponse {
        ticket,
        ws_v2: format!("/v2/rooms/{room_id}/ws"),
        exp: claims.exp,
        node_id: node,
    }))
}

async fn ws_legacy(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !origin_allowed(&headers, &state.registry.config().allowed_origins) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let ip = addr.ip().to_string();
    if state
        .admin
        .ban_hit(&crate::bans::BanSubject {
            ip: &ip,
            ..Default::default()
        })
        .is_some()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.registry.try_acquire_ip(addr.ip()) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some(room) = state.registry.get(&room_id) else {
        state.registry.release_ip(addr.ip());
        return StatusCode::NOT_FOUND.into_response();
    };
    let conn = state.registry.alloc_conn();
    let max_msg = state.registry.config().max_message_size;
    let out_cap = state.registry.config().player_out_capacity;
    let burst = state.registry.config().msg_burst;
    let refill = state.registry.config().msg_refill_per_sec;
    let registry = state.registry.clone();
    ws.max_message_size(max_msg)
        .max_frame_size(max_msg)
        .on_upgrade(move |socket| async move {
            legacy_client(socket, room, conn, out_cap, burst, refill, addr).await;
            registry.release_ip(addr.ip());
        })
}

async fn ws_v2(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !origin_allowed(&headers, &state.registry.config().allowed_origins) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let ip = addr.ip().to_string();
    if state
        .admin
        .ban_hit(&crate::bans::BanSubject {
            ip: &ip,
            ..Default::default()
        })
        .is_some()
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.registry.try_acquire_ip(addr.ip()) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Some(room) = state.registry.get(&room_id) else {
        state.registry.release_ip(addr.ip());
        return StatusCode::NOT_FOUND.into_response();
    };
    let conn = state.registry.alloc_conn();
    let max_msg = state.registry.config().max_message_size;
    let out_cap = state.registry.config().player_out_capacity;
    let burst = state.registry.config().msg_burst;
    let refill = state.registry.config().msg_refill_per_sec;
    let byte_burst = state.registry.config().byte_burst;
    let byte_refill = state.registry.config().byte_refill_per_sec;
    let handshake_secs = state.registry.config().handshake_timeout_secs;
    let idle_secs = state.registry.config().idle_timeout_secs;
    let registry = state.registry.clone();
    ws.max_message_size(max_msg)
        .max_frame_size(max_msg)
        .on_upgrade(move |socket| async move {
            v2_client(
                socket,
                room,
                conn,
                out_cap,
                burst,
                refill,
                byte_burst,
                byte_refill,
                handshake_secs,
                idle_secs,
                registry.clone(),
                addr,
            )
            .await;
            registry.release_ip(addr.ip());
        })
}

async fn legacy_client(
    socket: WebSocket,
    room: RoomHandle,
    conn: crate::room::ConnId,
    out_cap: usize,
    msg_burst: f64,
    msg_refill: f64,
    remote: SocketAddr,
) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Bytes>(out_cap);
    if room
        .tx
        .send(RoomCmd::LegacyJoin {
            conn,
            tx: tx.clone(),
            remote,
        })
        .await
        .is_err()
    {
        return;
    }

    let writer = tokio::spawn(async move {
        while let Some(bytes) = rx.recv().await {
            if sink.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
    });

    let mut bucket = TokenBucket::new(msg_burst, msg_refill);
    let mut violations = ViolationWindow::default();
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(data) => {
                if !bucket.try_take() {
                    if violations.hit() {
                        break;
                    }
                    continue;
                }
                if room
                    .tx
                    .send(RoomCmd::LegacyFrame { conn, data })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = room
        .tx
        .send(RoomCmd::Leave {
            conn,
            reason: LeaveReason::Disconnect,
        })
        .await;
    writer.abort();
}

#[allow(clippy::too_many_arguments)]
async fn v2_client(
    socket: WebSocket,
    room: RoomHandle,
    conn: crate::room::ConnId,
    out_cap: usize,
    msg_burst: f64,
    msg_refill: f64,
    byte_burst: f64,
    byte_refill: f64,
    handshake_secs: u64,
    idle_secs: u64,
    registry: RoomRegistry,
    remote: SocketAddr,
) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Bytes>(out_cap);
    if room
        .tx
        .send(RoomCmd::V2Join {
            conn,
            tx: tx.clone(),
            remote,
        })
        .await
        .is_err()
    {
        return;
    }

    let writer = tokio::spawn(async move {
        while let Some(bytes) = rx.recv().await {
            // Control is UTF-8 text; binary game frames stay binary.
            let msg = if looks_like_json(&bytes) {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(s) => Message::Text(s.into()),
                    Err(e) => Message::Binary(e.into_bytes().into()),
                }
            } else {
                Message::Binary(bytes)
            };
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut bucket = TokenBucket::new(msg_burst, msg_refill);
    let mut bytes_bucket = TokenBucket::new(byte_burst, byte_refill);
    // Clients send exactly one cmd per server tic (35 Hz, self-clocked by
    // TIC_FRAME); burst covers the input lead plus a reconnect catch-up.
    let mut tic_bucket = TokenBucket::new(70.0, 40.0);
    let mut violations = ViolationWindow::default();
    let mut saw_msg = false;
    let handshake = std::time::Duration::from_secs(handshake_secs.max(1));
    let idle = std::time::Duration::from_secs(idle_secs.max(1));
    let mut leave_reason = LeaveReason::Disconnect;
    loop {
        let limit = if saw_msg { idle } else { handshake };
        let next = tokio::time::timeout(limit, stream.next()).await;
        let msg = match next {
            Ok(Some(Ok(m))) => m,
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => {
                leave_reason = LeaveReason::Timeout;
                break;
            }
        };
        saw_msg = true;
        match msg {
            Message::Binary(data) => {
                registry.note_ws_in(data.len());
                // TIC_CMD has its own budget so RTC/ICE chatter cannot starve
                // lockstep input (dropped cmds stall every seat).
                let is_tic_cmd = data.len() >= 5
                    && data[..4] == *room_proto::FTR2_MAGIC
                    && data[4] == room_proto::MSG_TIC_CMD;
                let allowed = if is_tic_cmd {
                    tic_bucket.try_take()
                } else {
                    bucket.try_take() && bytes_bucket.try_take_n(data.len() as f64)
                };
                if !allowed {
                    registry.note_violation();
                    if violations.hit() {
                        break;
                    }
                    continue;
                }
                match parse_binary(&data) {
                    Ok(parsed) => {
                        if room
                            .tx
                            .send(RoomCmd::V2Binary { conn, msg: parsed })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(?e, "bad v2 binary");
                        registry.note_violation();
                        if violations.hit() {
                            break;
                        }
                    }
                }
            }
            Message::Text(text) => {
                registry.note_ws_in(text.len());
                if !bucket.try_take() || !bytes_bucket.try_take_n(text.len() as f64) {
                    registry.note_violation();
                    if violations.hit() {
                        break;
                    }
                    continue;
                }
                match parse_control(&text) {
                    Ok(parsed) => {
                        if room
                            .tx
                            .send(RoomCmd::V2Control { conn, msg: parsed })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(?e, "bad v2 control");
                        registry.note_violation();
                        if violations.hit() {
                            break;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = room
        .tx
        .send(RoomCmd::Leave {
            conn,
            reason: leave_reason,
        })
        .await;
    writer.abort();
}

fn looks_like_json(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'{'))
}

/// Local bind helper for tests.
pub async fn bind_ephemeral(
    registry: RoomRegistry,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>), std::io::Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let listener = listener.tap_io(|tcp| {
        let _ = tcp.set_nodelay(true);
    });
    // Share the registry's admin store so bans added via HTTP hit WS hello checks.
    let admin = registry.admin();
    let auth = build_auth_store(registry.config())?;
    let app = router(AppState {
        registry: registry.clone(),
        admin,
        auth,
    });
    let handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok((addr, handle))
}

//! Public lobby SPA + room directory (docs/22 player-facing surface).

use crate::state::AppState;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::atomic::Ordering;

const LOBBY_HTML: &str = include_str!("lobby_ui.html");
const LOBBY_CSS: &str = include_str!("lobby_ui.css");
const LOBBY_JS: &str = include_str!("lobby_ui.js");

/// Public lobby routes (game bind).
pub fn lobby_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(lobby_spa))
        .route("/lobby", get(lobby_spa))
        .route("/lobby/", get(lobby_spa))
        .route("/lobby/assets/app.css", get(lobby_css))
        .route("/lobby/assets/app.js", get(lobby_js))
        .route("/api/lobby", get(lobby_config))
        .route("/api/leaderboard", get(leaderboard))
        .route("/api/br/leaderboard", get(br_leaderboard))
        .route("/api/quick-play/br", axum::routing::post(quick_play_br))
}

/// `GET /api/leaderboard` — top 10 players (no auth).
async fn leaderboard(State(state): State<AppState>) -> Json<Vec<crate::leaderboard::PlayerStats>> {
    Json(state.registry.leaderboard().top(10))
}

#[derive(serde::Deserialize, Default)]
struct BrLeaderboardQuery {
    #[serde(default = "default_br_limit")]
    limit: usize,
}

fn default_br_limit() -> usize {
    25
}

/// `GET /api/br/leaderboard` — Battle Royale ratings (no auth).
async fn br_leaderboard(
    State(state): State<AppState>,
    Query(q): Query<BrLeaderboardQuery>,
) -> Json<Vec<crate::br_stats::BrPlayerStats>> {
    let n = q.limit.clamp(1, 100);
    Json(state.registry.br_stats().top(crate::br_stats::MODE_BR, n))
}

#[derive(serde::Deserialize)]
struct QuickPlayBrRequest {
    name: String,
}

/// `POST /api/quick-play/br` — create or join the 8-seat BR queue (`S-BR4`).
async fn quick_play_br(
    State(state): State<AppState>,
    Json(req): Json<QuickPlayBrRequest>,
) -> Result<Json<crate::http::JoinRoomResponse>, StatusCode> {
    let Some(name) = crate::names::sanitize_display_name(&req.name) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let (ticket, claims) = state.registry.quick_play_br(&name).map_err(|e| match e {
        crate::registry::RegistryError::UnknownMod
        | crate::registry::RegistryError::ModNotAllowed => StatusCode::SERVICE_UNAVAILABLE,
        crate::registry::RegistryError::Full | crate::registry::RegistryError::Draining => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;
    let room_id = claims.room_id.clone();
    Ok(Json(crate::http::JoinRoomResponse {
        ticket,
        ws_v2: format!("/v2/rooms/{room_id}/ws"),
        exp: claims.exp,
        node_id: state.registry.config().node_id.clone(),
    }))
}

async fn lobby_spa() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(LOBBY_HTML),
    )
}

async fn lobby_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        LOBBY_CSS,
    )
}

async fn lobby_js() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        LOBBY_JS,
    )
}

#[derive(Serialize)]
struct LobbyConfig {
    play_base: String,
    node_id: String,
    draining: bool,
    /// Default match length for new rooms (minutes).
    room_time_limit_mins: u16,
}

async fn lobby_config(State(state): State<AppState>) -> Json<LobbyConfig> {
    let cfg = state.registry.config();
    Json(LobbyConfig {
        play_base: cfg.play_base.clone(),
        node_id: cfg.node_id.clone(),
        draining: state.registry.is_draining(),
        room_time_limit_mins: cfg.room_time_limit_mins,
    })
}

/// Public room card for the lobby directory.
#[derive(Serialize)]
pub struct PublicRoom {
    /// Room id.
    pub id: String,
    /// Start map lump.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Seat capacity.
    pub max_players: u8,
    /// Live (non-away) seats.
    pub occupied: usize,
    /// Lockstep has started.
    pub started: bool,
    /// Owning node id.
    pub node_id: String,
    /// Always `public` for listed rooms.
    pub visibility: String,
    /// Gameplay mod (name + title), if any.
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub game_mod: Option<room_proto::RoomModRef>,
    /// Content packs (e.g. `voxel`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// Match time limit (minutes).
    pub time_limit: u16,
    /// `doom` or `duke3d`.
    pub game: String,
    /// Relative v2 WebSocket path.
    pub ws_v2: String,
    /// Relative legacy WebSocket path.
    pub ws_legacy: String,
}

/// `GET /api/rooms` — list public rooms (no auth).
pub async fn list_public_rooms(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicRoom>>, StatusCode> {
    let ids = state.registry.room_ids();
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(h) = state.registry.get(&id) else {
            continue;
        };
        if h.settings.visibility != "public" {
            continue;
        }
        out.push(PublicRoom {
            id: id.clone(),
            map: h.settings.map.clone(),
            skill: h.settings.skill,
            max_players: h.settings.max_players,
            occupied: h.occupied.load(Ordering::Relaxed),
            started: h.started.load(Ordering::Relaxed),
            node_id: state.registry.locate_room(&id).unwrap_or_default(),
            visibility: h.settings.visibility.clone(),
            game_mod: h.settings.game_mod.clone(),
            packs: h.settings.packs.clone(),
            time_limit: h.settings.time_limit,
            game: h.settings.game.clone(),
            ws_v2: format!("/v2/rooms/{id}/ws"),
            ws_legacy: format!("/ws/{id}"),
        });
    }
    out.sort_by(|a, b| a.map.cmp(&b.map).then(a.id.cmp(&b.id)));
    Ok(Json(out))
}

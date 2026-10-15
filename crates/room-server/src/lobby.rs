//! Public lobby SPA + room directory (docs/22 player-facing surface).

use crate::state::AppState;
use axum::extract::State;
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
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        LOBBY_JS,
    )
}

#[derive(Serialize)]
struct LobbyConfig {
    play_base: String,
    node_id: String,
    draining: bool,
}

async fn lobby_config(State(state): State<AppState>) -> Json<LobbyConfig> {
    Json(LobbyConfig {
        play_base: state.registry.config().play_base.clone(),
        node_id: state.registry.config().node_id.clone(),
        draining: state.registry.is_draining(),
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
            ws_v2: format!("/v2/rooms/{id}/ws"),
            ws_legacy: format!("/ws/{id}"),
        });
    }
    out.sort_by(|a, b| a.map.cmp(&b.map).then(a.id.cmp(&b.id)));
    Ok(Json(out))
}

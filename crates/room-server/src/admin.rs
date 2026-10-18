//! Admin API + embedded SPA (docs/22 §7).

use crate::admin_auth::{AdminIdentity, auth_routes};
use crate::registry::RoomRegistry;
use crate::room::{AdminCmd, CreateRoomRequest, CreateRoomResponse, RoomCmd, RoomInspect};
use crate::state::{AppState, AuditEntry, Ban};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

const ADMIN_HTML: &str = include_str!("admin_ui.html");
const ADMIN_CSS: &str = include_str!("admin_ui.css");
const ADMIN_JS: &str = include_str!("admin_ui.js");

/// Cluster summary for the dashboard.
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    /// Live rooms.
    pub rooms: usize,
    /// Node id (single-node for now).
    pub node_id: String,
    /// Graceful drain in progress.
    pub draining: bool,
    /// Rooms with verifier enabled.
    pub verified_rooms: usize,
}

/// Admin routes sharing [`AppState`].
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(admin_spa))
        .route("/admin/", get(admin_spa))
        .route("/admin/assets/app.css", get(admin_css))
        .route("/admin/assets/app.js", get(admin_js))
        .merge(auth_routes())
        .route("/admin/api/v1/summary", get(summary))
        .route(
            "/admin/api/v1/rooms",
            get(list_rooms).post(create_room_admin),
        )
        .route("/admin/api/v1/rooms/{id}", get(inspect_room))
        .route("/admin/api/v1/rooms/{id}/close", post(close_room))
        .route("/admin/api/v1/rooms/{id}/notice", post(notice_room))
        .route(
            "/admin/api/v1/rooms/{id}/seats/{seat}/kick",
            post(kick_seat),
        )
        .route("/admin/api/v1/rooms/{id}/seats/{seat}/ban", post(ban_seat))
        .route(
            "/admin/api/v1/rooms/{id}/seats/{seat}/mute",
            post(mute_seat),
        )
        .route("/admin/api/v1/audit", get(audit_list))
        .route("/admin/api/v1/bans", get(list_bans).post(add_ban))
        .route("/admin/api/v1/bans/{id}", delete(delete_ban))
        .route("/admin/api/v1/mods", get(list_mods))
        .route("/admin/api/v1/mods/{name}", post(set_mod_allowed))
        .route("/admin/api/v1/maintenance", post(maintenance))
        .route("/admin/api/v1/locations", get(list_locations))
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<AdminIdentity, StatusCode> {
    authorize_method(state, headers, &Method::GET)
}

fn authorize_mut(state: &AppState, headers: &HeaderMap) -> Result<AdminIdentity, StatusCode> {
    authorize_method(state, headers, &Method::POST)
}

fn authorize_method(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
) -> Result<AdminIdentity, StatusCode> {
    state.auth.authorize(
        headers,
        method,
        state.registry.config().admin_token.as_str(),
    )
}

async fn admin_spa() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(ADMIN_HTML),
    )
}

async fn admin_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        ADMIN_CSS,
    )
}

async fn admin_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        ADMIN_JS,
    )
}

async fn summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Summary>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    Ok(Json(Summary {
        rooms: state.registry.len(),
        node_id: state.registry.config().node_id.clone(),
        draining: state.registry.is_draining(),
        verified_rooms: state.registry.verified_room_count(),
    }))
}

#[derive(Deserialize)]
struct MaintenanceBody {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    message: String,
}

async fn maintenance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MaintenanceBody>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    if body.enabled {
        state.registry.begin_drain(&body.message);
        state
            .admin
            .audit(&actor.actor, "drain", "node", &body.message);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_mods(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::mods::AdminModRow>>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    Ok(Json(state.registry.config().mods.admin_listing()))
}

#[derive(Deserialize)]
struct ModAllowBody {
    allowed: bool,
}

async fn set_mod_allowed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<ModAllowBody>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let mods = &state.registry.config().mods;
    if mods.get(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    mods.set_allowed(&name, body.allowed)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let action = if body.allowed {
        "allow_mod"
    } else {
        "disallow_mod"
    };
    state.admin.audit(&actor.actor, action, &name, "");
    Ok(StatusCode::NO_CONTENT)
}

async fn list_locations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    let rows = state
        .registry
        .room_locations()
        .into_iter()
        .map(|(room_id, node_id)| serde_json::json!({ "room_id": room_id, "node_id": node_id }))
        .collect();
    Ok(Json(rows))
}

async fn list_rooms(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    let ids = state.registry.room_ids();
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(h) = state.registry.get(&id) {
            out.push(serde_json::json!({
                "id": id,
                "map": h.settings.map,
                "skill": h.settings.skill,
                "max_players": h.settings.max_players,
                "rng_seed": h.rng_seed,
                "enable_verifier": h.settings.enable_verifier,
                "mod": h.settings.game_mod.as_ref().map(|m| m.name.clone()),
                "occupied": h.occupied.load(std::sync::atomic::Ordering::Relaxed),
                "started": h.started.load(std::sync::atomic::Ordering::Relaxed),
                "node_id": state.registry.locate_room(&id).unwrap_or_default(),
            }));
        }
    }
    Ok(Json(out))
}

async fn inspect_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RoomInspect>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let (tx, rx) = oneshot::channel();
    room.tx
        .send(RoomCmd::Admin(AdminCmd::Inspect { reply: tx }))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let snap = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(snap))
}

async fn create_room_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateRoomRequest>,
) -> Result<Json<CreateRoomResponse>, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let map = body.map.clone();
    let created = state.registry.create_room(body).map_err(|e| match e {
        crate::registry::RegistryError::Full => StatusCode::INSUFFICIENT_STORAGE,
        crate::registry::RegistryError::Draining => StatusCode::SERVICE_UNAVAILABLE,
        crate::registry::RegistryError::VerifierFull
        | crate::registry::RegistryError::UnknownMod
        | crate::registry::RegistryError::UnknownGame
        | crate::registry::RegistryError::ModNotAllowed => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;
    state
        .admin
        .audit(&actor.actor, "create_room", &created.id, &map);
    Ok(Json(created))
}

async fn close_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let _ = room
        .tx
        .send(RoomCmd::Admin(AdminCmd::Close {
            reason: "admin close".into(),
        }))
        .await;
    state.registry.remove(&id);
    state
        .admin
        .audit(&actor.actor, "close_room", &id, "admin close");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct NoticeBody {
    text: String,
}

async fn notice_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<NoticeBody>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let text = body.text.chars().take(500).collect::<String>();
    room.tx
        .send(RoomCmd::Admin(AdminCmd::Notice { text: text.clone() }))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    state.admin.audit(&actor.actor, "notice", &id, &text);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct KickBody {
    #[serde(default)]
    reason: String,
}

async fn kick_seat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, seat)): Path<(String, u8)>,
    Json(body): Json<KickBody>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let reason = if body.reason.is_empty() {
        "kicked".into()
    } else {
        body.reason
    };
    room.tx
        .send(RoomCmd::Admin(AdminCmd::Kick {
            seat,
            reason: reason.clone(),
        }))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    state
        .admin
        .audit(&actor.actor, "kick", &format!("{id}/{seat}"), &reason);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct BanSeatBody {
    /// Ban scopes to apply from the seat row (`ip`, `name`, `fingerprint`, …).
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    reason: String,
    /// Also kick the seat after banning (default true).
    #[serde(default = "default_true")]
    kick: bool,
    /// Expiry unix seconds (`0` = permanent).
    #[serde(default)]
    exp: u64,
}

fn default_true() -> bool {
    true
}

async fn ban_seat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, seat)): Path<(String, u8)>,
    Json(body): Json<BanSeatBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let (tx, rx) = oneshot::channel();
    room.tx
        .send(RoomCmd::Admin(AdminCmd::Inspect { reply: tx }))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let inspect = rx.await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let Some(row) = inspect.seats.iter().find(|s| s.seat == seat) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let reason = if body.reason.is_empty() {
        format!("banned from seat {seat}")
    } else {
        body.reason
    };
    let scopes = if body.scopes.is_empty() {
        vec!["ip".into(), "name".into()]
    } else {
        body.scopes
    };
    let mut created = Vec::new();
    for scope_raw in scopes {
        let scope = crate::bans::normalize_scope(&scope_raw);
        let value = match scope.as_str() {
            "ip" if row.remote_ip != "—" && !row.remote_ip.is_empty() => row.remote_ip.clone(),
            "name" if !row.name.is_empty() => row.name.clone(),
            "name_regex" if !row.name.is_empty() => row.name.clone(),
            "fingerprint" if row.iwad_fingerprint != "—" && !row.iwad_fingerprint.is_empty() => {
                row.iwad_fingerprint.clone()
            }
            "ticket_id" if row.ticket_id != "—" && !row.ticket_id.is_empty() => {
                row.ticket_id.clone()
            }
            "turn_user" if !row.turn_username.is_empty() => row.turn_username.clone(),
            "resume_token" if !row.resume_token.is_empty() => row.resume_token.clone(),
            "auth_key" => {
                // Prefer ticket id, then TURN user, then fingerprint.
                if row.ticket_id != "—" && !row.ticket_id.is_empty() {
                    row.ticket_id.clone()
                } else if !row.turn_username.is_empty() {
                    row.turn_username.clone()
                } else if row.iwad_fingerprint != "—" && !row.iwad_fingerprint.is_empty() {
                    row.iwad_fingerprint.clone()
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        if crate::bans::validate_ban(&scope, &value).is_err() {
            continue;
        }
        let ban_id = crate::util::new_room_id().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let ban = Ban {
            id: ban_id.clone(),
            scope: scope.clone(),
            value: value.clone(),
            reason: reason.clone(),
            exp: body.exp,
        };
        state.admin.add_ban(ban);
        state.admin.audit(
            &actor.actor,
            "add_ban",
            &ban_id,
            &format!("{scope}={value}"),
        );
        created.push(serde_json::json!({ "id": ban_id, "scope": scope, "value": value }));
    }
    if body.kick {
        room.tx
            .send(RoomCmd::Admin(AdminCmd::Kick {
                seat,
                reason: reason.clone(),
            }))
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        state
            .admin
            .audit(&actor.actor, "kick", &format!("{id}/{seat}"), &reason);
    }
    Ok(Json(
        serde_json::json!({ "bans": created, "kicked": body.kick }),
    ))
}

#[derive(Deserialize)]
struct MuteBody {
    #[serde(default)]
    audio: bool,
    #[serde(default)]
    video: bool,
    #[serde(default)]
    chat: bool,
}

async fn mute_seat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, seat)): Path<(String, u8)>,
    Json(body): Json<MuteBody>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let Some(room) = state.registry.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    room.tx
        .send(RoomCmd::Admin(AdminCmd::Mute {
            seat,
            audio: body.audio,
            video: body.video,
            chat: body.chat,
        }))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    state.admin.audit(
        &actor.actor,
        "mute",
        &format!("{id}/{seat}"),
        &format!("a={} v={} c={}", body.audio, body.video, body.chat),
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn audit_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AuditEntry>>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    Ok(Json(state.admin.audit_list()))
}

async fn list_bans(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Ban>>, StatusCode> {
    let _actor = authorize(&state, &headers)?;
    Ok(Json(state.admin.bans()))
}

#[derive(Deserialize)]
struct AddBanBody {
    scope: String,
    value: String,
    reason: String,
    #[serde(default)]
    exp: u64,
}

async fn add_ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AddBanBody>,
) -> Result<Json<Ban>, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    let scope = crate::bans::normalize_scope(&body.scope);
    let value = body.value.trim().to_owned();
    crate::bans::validate_ban(&scope, &value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let id = crate::util::new_room_id().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ban = Ban {
        id: id.clone(),
        scope,
        value,
        reason: body.reason,
        exp: body.exp,
    };
    state.admin.add_ban(ban.clone());
    state.admin.audit(&actor.actor, "add_ban", &id, &ban.reason);
    Ok(Json(ban))
}

async fn delete_ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let actor = authorize_mut(&state, &headers)?;
    if !state.admin.remove_ban(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    state.admin.audit(&actor.actor, "delete_ban", &id, "");
    Ok(StatusCode::NO_CONTENT)
}

/// Helper for metrics text.
#[must_use]
pub fn prometheus_text(registry: &RoomRegistry) -> String {
    use std::sync::atomic::Ordering;
    format!(
        "\
# HELP room_server_rooms Live rooms
# TYPE room_server_rooms gauge
room_server_rooms {}
# HELP room_server_draining Node drain flag (1=draining)
# TYPE room_server_draining gauge
room_server_draining {}
# HELP room_server_verified_rooms Verifier-enabled rooms
# TYPE room_server_verified_rooms gauge
room_server_verified_rooms {}
# HELP room_server_ws_msgs_total Inbound WebSocket messages
# TYPE room_server_ws_msgs_total counter
room_server_ws_msgs_total {}
# HELP room_server_ws_bytes_total Inbound WebSocket bytes
# TYPE room_server_ws_bytes_total counter
room_server_ws_bytes_total {}
# HELP room_server_violations_total Rate/size violations
# TYPE room_server_violations_total counter
room_server_violations_total {}
# HELP room_server_tic_frames_total TIC_FRAME fan-outs
# TYPE room_server_tic_frames_total counter
room_server_tic_frames_total {}
# HELP room_server_relay_latency_us First TicCmd → TIC_FRAME (server-side)
# TYPE room_server_relay_latency_us histogram
room_server_relay_latency_us_bucket{{le=\"100\"}} {}
room_server_relay_latency_us_bucket{{le=\"300\"}} {}
room_server_relay_latency_us_bucket{{le=\"1000\"}} {}
room_server_relay_latency_us_bucket{{le=\"2000\"}} {}
room_server_relay_latency_us_bucket{{le=\"5000\"}} {}
room_server_relay_latency_us_bucket{{le=\"+Inf\"}} {}
room_server_relay_latency_us_sum {}
room_server_relay_latency_us_count {}
",
        registry.len(),
        u8::from(registry.is_draining()),
        registry.verified_room_count(),
        registry.metric_ws_msgs_in.load(Ordering::Relaxed),
        registry.metric_ws_bytes_in.load(Ordering::Relaxed),
        registry.metric_violations.load(Ordering::Relaxed),
        registry.metric_tic_frames.load(Ordering::Relaxed),
        registry.metric_relay_us_buckets[0].load(Ordering::Relaxed),
        cumulative(registry, 1),
        cumulative(registry, 2),
        cumulative(registry, 3),
        cumulative(registry, 4),
        cumulative(registry, 5),
        registry.metric_relay_us_sum.load(Ordering::Relaxed),
        registry.metric_relay_us_count.load(Ordering::Relaxed),
    )
}

fn cumulative(registry: &RoomRegistry, upto: usize) -> u64 {
    use std::sync::atomic::Ordering;
    registry.metric_relay_us_buckets[..=upto]
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .sum()
}

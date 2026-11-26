//! Typed wrappers around `/admin/api/v1` and `/metrics`.

use crate::http::{self, Auth, HttpError, HttpUrl};
use serde::Deserialize;
use serde_json::json;

/// Cluster summary from `GET /admin/api/v1/summary`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Summary {
    /// Live rooms.
    pub rooms: usize,
    /// Node id.
    #[serde(default)]
    pub node_id: String,
    /// Drain in progress.
    #[serde(default)]
    pub draining: bool,
    /// Verifier-enabled rooms.
    #[serde(default)]
    pub verified_rooms: usize,
}

/// One row from `GET /admin/api/v1/rooms`.
#[derive(Clone, Debug, Deserialize)]
pub struct RoomRow {
    /// Room id.
    pub id: String,
    /// Start map.
    #[serde(default)]
    pub map: String,
    /// Skill.
    #[serde(default)]
    pub skill: u8,
    /// Capacity.
    #[serde(default)]
    pub max_players: u8,
    /// Occupied seats.
    #[serde(default)]
    pub occupied: usize,
    /// Lockstep started.
    #[serde(default)]
    pub started: bool,
    /// Gameplay mod.
    #[serde(default, rename = "mod")]
    pub game_mod: Option<String>,
    /// Content packs.
    #[serde(default)]
    pub packs: Vec<String>,
    /// Verifier flag.
    #[serde(default)]
    pub enable_verifier: bool,
    /// Owning node.
    #[serde(default)]
    pub node_id: String,
}

/// Seat pose for the minimap strip.
#[derive(Clone, Debug, Deserialize)]
pub struct PoseInspect {
    /// Map X.
    pub x: i32,
    /// Map Y.
    pub y: i32,
    /// Yaw degrees.
    pub angle: i16,
}

/// One seat from room inspect.
#[derive(Clone, Debug, Deserialize)]
pub struct SeatInspect {
    /// Seat index.
    pub seat: u8,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Lifecycle state.
    #[serde(default)]
    pub state: String,
    /// Remote IP.
    #[serde(default)]
    pub remote_ip: String,
    /// Ticket id.
    #[serde(default)]
    pub ticket_id: String,
    /// IWAD fingerprint.
    #[serde(default)]
    pub iwad_fingerprint: String,
    /// Mute flags.
    #[serde(default)]
    pub muted_audio: bool,
    /// Video mute.
    #[serde(default)]
    pub muted_video: bool,
    /// Chat mute.
    #[serde(default)]
    pub muted_chat: bool,
    /// Latest pose.
    #[serde(default)]
    pub pose: Option<PoseInspect>,
    /// Client RTT from the last PING (ms).
    #[serde(default)]
    pub rtt_ms: Option<u16>,
    /// Consecutive server-filled input tics (stall).
    #[serde(default)]
    pub miss_streak: u8,
}

/// Spectator row.
#[derive(Clone, Debug, Deserialize)]
pub struct SpectatorInspect {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Remote IP.
    #[serde(default)]
    pub remote_ip: String,
    /// Frames delivered.
    #[serde(default)]
    pub frames_sent: usize,
}

/// Full room inspect payload.
#[derive(Clone, Debug, Deserialize)]
pub struct RoomInspect {
    /// Room id.
    pub id: String,
    /// Map.
    #[serde(default)]
    pub map: String,
    /// Skill.
    #[serde(default)]
    pub skill: u8,
    /// Capacity.
    #[serde(default)]
    pub max_players: u8,
    /// RNG seed.
    #[serde(default)]
    pub rng_seed: u32,
    /// Started.
    #[serde(default)]
    pub started: bool,
    /// Verifier.
    #[serde(default)]
    pub enable_verifier: bool,
    /// Mod.
    #[serde(default, rename = "mod")]
    pub game_mod: Option<String>,
    /// Packs.
    #[serde(default)]
    pub packs: Vec<String>,
    /// Occupied.
    #[serde(default)]
    pub occupied: usize,
    /// Seats.
    #[serde(default)]
    pub seats: Vec<SeatInspect>,
    /// Spectators.
    #[serde(default)]
    pub spectators: Vec<SpectatorInspect>,
    /// Outline points.
    #[serde(default)]
    pub outline: Vec<i16>,
    /// Game id.
    #[serde(default)]
    pub game: String,
    /// Lockstep protocol (`lobby` / `v2` / `duke`).
    #[serde(default)]
    pub protocol: String,
    /// Next tic awaiting release.
    #[serde(default)]
    pub next_tic: u32,
    /// Last Doom frame relay latency (µs).
    #[serde(default)]
    pub last_relay_us: Option<u64>,
    /// Seconds since room create / start clock.
    #[serde(default)]
    pub uptime_secs: u64,
    /// Stall-fill grace (ms).
    #[serde(default)]
    pub stall_fill_ms: u64,
    /// Desync broadcasts from this room.
    #[serde(default)]
    pub desync_count: u64,
    /// Incomplete checksum tic sets.
    #[serde(default)]
    pub pending_checksum_tics: usize,
    /// Seats still missing a command for `next_tic`.
    #[serde(default)]
    pub waiting_seats: Vec<u8>,
}

/// Audit log row.
#[derive(Clone, Debug, Deserialize)]
pub struct AuditEntry {
    /// Unix seconds.
    pub ts: u64,
    /// Actor.
    #[serde(default)]
    pub actor: String,
    /// Action.
    #[serde(default)]
    pub action: String,
    /// Target.
    #[serde(default)]
    pub target: String,
    /// Detail.
    #[serde(default)]
    pub detail: String,
}

/// Ban row.
#[derive(Clone, Debug, Deserialize)]
pub struct Ban {
    /// Ban id.
    pub id: String,
    /// Scope.
    #[serde(default)]
    pub scope: String,
    /// Value.
    #[serde(default)]
    pub value: String,
    /// Reason.
    #[serde(default)]
    pub reason: String,
    /// Expiry (`0` = permanent).
    #[serde(default)]
    pub exp: u64,
}

/// Admin mod listing row.
#[derive(Clone, Debug, Deserialize)]
pub struct AdminModRow {
    /// Catalog name.
    pub name: String,
    /// Title.
    #[serde(default)]
    pub title: String,
    /// Fingerprint.
    #[serde(default)]
    pub fingerprint: String,
    /// Size bytes.
    #[serde(default)]
    pub size: u64,
    /// Allowed for new rooms.
    #[serde(default)]
    pub allowed: bool,
}

/// Parsed `/metrics` counters of interest.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    /// Live rooms gauge.
    pub rooms: u64,
    /// Drain flag.
    pub draining: u64,
    /// Verified rooms.
    pub verified_rooms: u64,
    /// Inbound WS messages.
    pub ws_msgs: u64,
    /// Inbound WS bytes.
    pub ws_bytes: u64,
    /// Violations.
    pub violations: u64,
    /// Tic frame fan-outs.
    pub tic_frames: u64,
    /// Relay latency sum (µs).
    pub relay_latency_sum: u64,
    /// Relay latency sample count.
    pub relay_latency_count: u64,
    /// Cumulative histogram buckets: ≤100, ≤300, ≤1000, ≤2000, ≤5000, +Inf.
    pub relay_buckets: [u64; 6],
}

impl Metrics {
    /// Mean relay latency in microseconds, when samples exist.
    #[must_use]
    pub fn mean_relay_us(&self) -> Option<u64> {
        self.relay_latency_sum.checked_div(self.relay_latency_count)
    }

    /// Per-bucket counts (non-cumulative) for the relay histogram.
    #[must_use]
    pub fn relay_bucket_deltas(&self) -> [u64; 6] {
        let mut d = [0u64; 6];
        let mut prev = 0u64;
        for (i, &cum) in self.relay_buckets.iter().enumerate() {
            d[i] = cum.saturating_sub(prev);
            prev = cum;
        }
        d
    }
}

/// Result of a password login attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginOutcome {
    /// Session cookie is ready.
    Session,
    /// Password ok; TOTP code required next.
    NeedTotp,
}

/// Admin API client.
#[derive(Clone, Debug)]
pub struct Client {
    base: HttpUrl,
    auth: Auth,
}

impl Client {
    /// Build a client against an admin base URL (`http://host:port`).
    ///
    /// # Errors
    ///
    /// Bad URL.
    pub fn new(base: &str) -> Result<Self, HttpError> {
        Ok(Self {
            base: HttpUrl::parse(base)?,
            auth: Auth::default(),
        })
    }

    /// Set bearer token auth.
    pub fn set_bearer(&mut self, token: impl Into<String>) {
        let t = token.into();
        self.auth.bearer = if t.is_empty() { None } else { Some(t) };
    }

    fn url(&self, path: &str) -> HttpUrl {
        self.base.with_path(path)
    }

    /// Password login; stores session + CSRF, or leaves a pending cookie when MFA is required.
    ///
    /// # Errors
    ///
    /// Transport or bad credentials.
    pub fn login(&mut self, user: &str, password: &str) -> Result<LoginOutcome, HttpError> {
        let res = http::post_json(
            &self.url("/admin/api/v1/auth/login"),
            &self.auth,
            &json!({ "username": user, "password": password }),
        )?;
        self.apply_auth_cookies(&res);
        if !(200..300).contains(&res.status) {
            return Err(HttpError::Status {
                status: res.status,
                body: res.text().chars().take(240).collect(),
            });
        }
        let body: serde_json::Value =
            serde_json::from_slice(&res.body).map_err(|e| HttpError::Response(e.to_string()))?;
        if let Some(csrf) = body.get("csrf").and_then(|v| v.as_str()) {
            self.auth.csrf = Some(csrf.to_owned());
        }
        if self.auth.session.is_some() {
            return Ok(LoginOutcome::Session);
        }
        let need = body
            .get("need")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let methods: Vec<&str> = body
            .get("methods")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        // Prefer TOTP in the terminal even when the server prefers WebAuthn.
        if need == "totp" || methods.contains(&"totp") {
            return Ok(LoginOutcome::NeedTotp);
        }
        if need == "webauthn" {
            return Err(HttpError::Response(
                "login needs a passkey — use the web admin UI, or enroll TOTP on that account"
                    .into(),
            ));
        }
        Err(HttpError::Response(format!(
            "login incomplete (need `{need}`)"
        )))
    }

    /// Complete a pending TOTP challenge after [`Self::login`] returned [`LoginOutcome::NeedTotp`].
    ///
    /// # Errors
    ///
    /// Transport or bad code.
    pub fn verify_totp(&mut self, code: &str) -> Result<(), HttpError> {
        let res = http::post_json(
            &self.url("/admin/api/v1/auth/totp"),
            &self.auth,
            &json!({ "code": code }),
        )?;
        self.apply_auth_cookies(&res);
        if !(200..300).contains(&res.status) {
            return Err(HttpError::Status {
                status: res.status,
                body: res.text().chars().take(240).collect(),
            });
        }
        let body: serde_json::Value =
            serde_json::from_slice(&res.body).map_err(|e| HttpError::Response(e.to_string()))?;
        if let Some(csrf) = body.get("csrf").and_then(|v| v.as_str()) {
            self.auth.csrf = Some(csrf.to_owned());
        }
        if self.auth.session.is_none() {
            return Err(HttpError::Response(
                "TOTP ok but no session cookie returned".into(),
            ));
        }
        self.auth.pending = None;
        Ok(())
    }

    /// Auth mode from `GET /admin/api/v1/auth/status` (`mfa` / `token` / `open`).
    ///
    /// # Errors
    ///
    /// Transport failures (401 is still a successful probe when mode is MFA).
    pub fn auth_mode(&self) -> Result<String, HttpError> {
        let res = http::request(
            &self.url("/admin/api/v1/auth/status"),
            "GET",
            &self.auth,
            None,
            None,
            std::time::Duration::from_secs(5),
        )?;
        // Status is public enough to answer without a session.
        if res.status == 401 || res.status == 403 {
            return Ok("mfa".into());
        }
        if !(200..300).contains(&res.status) {
            return Err(HttpError::Status {
                status: res.status,
                body: res.text().chars().take(240).collect(),
            });
        }
        let body: serde_json::Value =
            serde_json::from_slice(&res.body).map_err(|e| HttpError::Response(e.to_string()))?;
        Ok(body
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned())
    }

    fn apply_auth_cookies(&mut self, res: &crate::http::Response) {
        for (name, value) in res.set_cookies() {
            match name.as_str() {
                "room_admin_sid" => self.auth.session = Some(value),
                "room_admin_pending" => self.auth.pending = Some(value),
                _ => {}
            }
        }
    }

    /// `GET /admin/api/v1/summary`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn summary(&self) -> Result<Summary, HttpError> {
        http::get_json(&self.url("/admin/api/v1/summary"), &self.auth)
    }

    /// `GET /admin/api/v1/rooms`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn rooms(&self) -> Result<Vec<RoomRow>, HttpError> {
        http::get_json(&self.url("/admin/api/v1/rooms"), &self.auth)
    }

    /// `GET /admin/api/v1/rooms/{id}`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn inspect(&self, id: &str) -> Result<RoomInspect, HttpError> {
        http::get_json(&self.url(&format!("/admin/api/v1/rooms/{id}")), &self.auth)
    }

    /// `GET /admin/api/v1/audit`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn audit(&self) -> Result<Vec<AuditEntry>, HttpError> {
        http::get_json(&self.url("/admin/api/v1/audit"), &self.auth)
    }

    /// `GET /admin/api/v1/bans`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn bans(&self) -> Result<Vec<Ban>, HttpError> {
        http::get_json(&self.url("/admin/api/v1/bans"), &self.auth)
    }

    /// `GET /admin/api/v1/mods`.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn mods(&self) -> Result<Vec<AdminModRow>, HttpError> {
        http::get_json(&self.url("/admin/api/v1/mods"), &self.auth)
    }

    /// `GET /metrics` (game or admin bind — same handlers).
    ///
    /// # Errors
    ///
    /// Transport failures.
    pub fn metrics(&self) -> Result<Metrics, HttpError> {
        let text = http::get_text(&self.url("/metrics"), &self.auth)?;
        Ok(parse_metrics(&text))
    }

    /// Close a room.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn close_room(&self, id: &str) -> Result<(), HttpError> {
        http::post_empty(
            &self.url(&format!("/admin/api/v1/rooms/{id}/close")),
            &self.auth,
        )
    }

    /// Broadcast a notice into a room.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn notice(&self, id: &str, text: &str) -> Result<(), HttpError> {
        let res = http::post_json(
            &self.url(&format!("/admin/api/v1/rooms/{id}/notice")),
            &self.auth,
            &json!({ "text": text }),
        )?;
        if !(200..300).contains(&res.status) {
            return Err(HttpError::Status {
                status: res.status,
                body: res.text().chars().take(240).collect(),
            });
        }
        Ok(())
    }

    /// Kick a seat.
    ///
    /// # Errors
    ///
    /// Transport / auth failures.
    pub fn kick(&self, id: &str, seat: u8, reason: &str) -> Result<(), HttpError> {
        let res = http::post_json(
            &self.url(&format!("/admin/api/v1/rooms/{id}/seats/{seat}/kick")),
            &self.auth,
            &json!({ "reason": reason }),
        )?;
        if !(200..300).contains(&res.status) {
            return Err(HttpError::Status {
                status: res.status,
                body: res.text().chars().take(240).collect(),
            });
        }
        Ok(())
    }
}

fn parse_metrics(text: &str) -> Metrics {
    let mut m = Metrics::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let Some(value) = parts.next().and_then(|v| v.parse::<f64>().ok()) else {
            continue;
        };
        let v = value as u64;
        match name {
            "room_server_rooms" => m.rooms = v,
            "room_server_draining" => m.draining = v,
            "room_server_verified_rooms" => m.verified_rooms = v,
            "room_server_ws_msgs_total" => m.ws_msgs = v,
            "room_server_ws_bytes_total" => m.ws_bytes = v,
            "room_server_violations_total" => m.violations = v,
            "room_server_tic_frames_total" => m.tic_frames = v,
            "room_server_relay_latency_us_sum" => m.relay_latency_sum = v,
            "room_server_relay_latency_us_count" => m.relay_latency_count = v,
            "room_server_relay_latency_us_bucket{le=\"100\"}" => m.relay_buckets[0] = v,
            "room_server_relay_latency_us_bucket{le=\"300\"}" => m.relay_buckets[1] = v,
            "room_server_relay_latency_us_bucket{le=\"1000\"}" => m.relay_buckets[2] = v,
            "room_server_relay_latency_us_bucket{le=\"2000\"}" => m.relay_buckets[3] = v,
            "room_server_relay_latency_us_bucket{le=\"5000\"}" => m.relay_buckets[4] = v,
            "room_server_relay_latency_us_bucket{le=\"+Inf\"}" => m.relay_buckets[5] = v,
            _ => {}
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prometheus_snippet() {
        let text = "\
# HELP room_server_rooms Live rooms
room_server_rooms 3
room_server_ws_msgs_total 42
room_server_relay_latency_us_sum 1000
room_server_relay_latency_us_count 10
";
        let m = parse_metrics(text);
        assert_eq!(m.rooms, 3);
        assert_eq!(m.ws_msgs, 42);
        assert_eq!(m.mean_relay_us(), Some(100));
    }
}

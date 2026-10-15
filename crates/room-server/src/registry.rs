//! In-process room registry.

use crate::config::ServerConfig;
use crate::room::{
    AdminCmd, CreateRoomRequest, CreateRoomResponse, RoomCmd, RoomHandle, RoomSettings,
    make_create_response, new_rng_seed, spawn_room,
};
use crate::state::AdminStore;
use crate::ticket::{TicketClaims, TicketError, TicketKey, mint_ticket, verify_ticket};
use crate::util::new_room_id;
use dashmap::DashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::info;

/// Registry errors.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Node at capacity.
    #[error("too many rooms")]
    Full,
    /// Node is draining / maintenance.
    #[error("node draining")]
    Draining,
    /// Verifier room cap reached.
    #[error("verifier capacity")]
    VerifierFull,
    /// RNG failure.
    #[error("rng failed")]
    Rng,
    /// Unknown room.
    #[error("room not found")]
    NotFound,
    /// Bad / replayed / expired ticket.
    #[error("ticket: {0}")]
    Ticket(#[from] TicketError),
}

/// Shared registry of live rooms.
#[derive(Clone)]
pub struct RoomRegistry {
    rooms: Arc<DashMap<String, RoomHandle>>,
    /// `room_id` → owning `node_id` (cluster routing table; local writes).
    room_location: Arc<DashMap<String, String>>,
    /// Consumed ticket ids (one-time use).
    used_tickets: Arc<DashMap<String, u64>>,
    /// Live WebSocket count per client IP.
    conn_per_ip: Arc<DashMap<IpAddr, usize>>,
    cfg: ServerConfig,
    ticket_key: TicketKey,
    /// Shared ban / audit store (swappable; all registry clones see updates).
    admin: Arc<RwLock<AdminStore>>,
    next_conn: Arc<AtomicU64>,
    draining: Arc<AtomicBool>,
    verified_rooms: Arc<AtomicUsize>,
    /// Prometheus counters.
    pub(crate) metric_ws_msgs_in: Arc<AtomicU64>,
    pub(crate) metric_ws_bytes_in: Arc<AtomicU64>,
    pub(crate) metric_violations: Arc<AtomicU64>,
    pub(crate) metric_tic_frames: Arc<AtomicU64>,
    /// Relay latency histogram buckets: ≤100, ≤300, ≤1000, ≤2000, ≤5000, +Inf µs.
    pub(crate) metric_relay_us_buckets: Arc<[AtomicU64; 6]>,
    pub(crate) metric_relay_us_sum: Arc<AtomicU64>,
    pub(crate) metric_relay_us_count: Arc<AtomicU64>,
}

impl RoomRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new(cfg: ServerConfig) -> Self {
        let ticket_key = TicketKey::from_secret(&cfg.ticket_secret);
        Self {
            rooms: Arc::new(DashMap::new()),
            room_location: Arc::new(DashMap::new()),
            used_tickets: Arc::new(DashMap::new()),
            conn_per_ip: Arc::new(DashMap::new()),
            cfg,
            ticket_key,
            admin: Arc::new(RwLock::new(AdminStore::default())),
            next_conn: Arc::new(AtomicU64::new(1)),
            draining: Arc::new(AtomicBool::new(false)),
            verified_rooms: Arc::new(AtomicUsize::new(0)),
            metric_ws_msgs_in: Arc::new(AtomicU64::new(0)),
            metric_ws_bytes_in: Arc::new(AtomicU64::new(0)),
            metric_violations: Arc::new(AtomicU64::new(0)),
            metric_tic_frames: Arc::new(AtomicU64::new(0)),
            metric_relay_us_buckets: Arc::new([
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ]),
            metric_relay_us_sum: Arc::new(AtomicU64::new(0)),
            metric_relay_us_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Attach the process-wide admin / ban store (called from `serve`).
    ///
    /// Updates are visible to every [`RoomRegistry`] clone (room actors included).
    pub fn set_admin(&self, admin: AdminStore) {
        if let Ok(mut g) = self.admin.write() {
            *g = admin;
        }
    }

    /// Ban / audit store snapshot (cheap [`AdminStore`] clone sharing inner Arcs).
    #[must_use]
    pub fn admin(&self) -> AdminStore {
        self.admin.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// Increment inbound WS message / byte counters.
    pub fn note_ws_in(&self, bytes: usize) {
        self.metric_ws_msgs_in.fetch_add(1, Ordering::Relaxed);
        self.metric_ws_bytes_in
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Increment violation counter.
    pub fn note_violation(&self) {
        self.metric_violations.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment TIC_FRAME fan-out counter.
    pub fn note_tic_frame(&self) {
        self.metric_tic_frames.fetch_add(1, Ordering::Relaxed);
    }

    /// Record server-side relay latency (first cmd → TIC_FRAME) in microseconds.
    pub fn note_relay_us(&self, us: u64) {
        self.metric_relay_us_sum.fetch_add(us, Ordering::Relaxed);
        self.metric_relay_us_count.fetch_add(1, Ordering::Relaxed);
        let idx = if us <= 100 {
            0
        } else if us <= 300 {
            1
        } else if us <= 1_000 {
            2
        } else if us <= 2_000 {
            3
        } else if us <= 5_000 {
            4
        } else {
            5
        };
        self.metric_relay_us_buckets[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot of Prometheus counters (tests / admin).
    #[must_use]
    pub fn metrics_snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.metric_ws_msgs_in.load(Ordering::Relaxed),
            self.metric_ws_bytes_in.load(Ordering::Relaxed),
            self.metric_violations.load(Ordering::Relaxed),
            self.metric_tic_frames.load(Ordering::Relaxed),
        )
    }

    /// Server-side relay latency summary: `(count, avg_us, share_le_300us)`.
    #[must_use]
    pub fn relay_latency_summary(&self) -> Option<(u64, u64, f64)> {
        let count = self.metric_relay_us_count.load(Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        let sum = self.metric_relay_us_sum.load(Ordering::Relaxed);
        let le300 = self.metric_relay_us_buckets[0].load(Ordering::Relaxed)
            + self.metric_relay_us_buckets[1].load(Ordering::Relaxed);
        Some((count, sum / count, le300 as f64 / count as f64))
    }

    /// True when the node refuses new rooms (graceful drain).
    #[must_use]
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// Enter drain: stop creates, notice every room, schedule forced close.
    pub fn begin_drain(&self, message: &str) {
        self.draining.store(true, Ordering::Relaxed);
        info!(node = %self.cfg.node_id, "drain started");
        let notice = if message.is_empty() {
            format!(
                "server draining; rooms close in {}s",
                self.cfg.drain_grace_secs
            )
        } else {
            message.to_owned()
        };
        for entry in self.rooms.iter() {
            let _ = entry.tx.try_send(RoomCmd::Admin(AdminCmd::Notice {
                text: notice.clone(),
            }));
        }
        let reg = self.clone();
        let grace = self.cfg.drain_grace_secs;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(grace)).await;
            for id in reg.room_ids() {
                if let Some(h) = reg.get(&id) {
                    let _ = h.tx.try_send(RoomCmd::Admin(AdminCmd::Close {
                        reason: "drain".into(),
                    }));
                }
            }
        });
    }

    /// Look up which node owns `room_id` (local table).
    #[must_use]
    pub fn locate_room(&self, room_id: &str) -> Option<String> {
        self.room_location.get(room_id).map(|v| v.clone())
    }

    /// Snapshot of `room_location` rows (sorted by room id).
    #[must_use]
    pub fn room_locations(&self) -> Vec<(String, String)> {
        let mut rows: Vec<(String, String)> = self
            .room_location
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    /// Try to reserve a connection slot for `ip` (docs/22 §5).
    #[must_use]
    pub fn try_acquire_ip(&self, ip: IpAddr) -> bool {
        let max = self.cfg.max_conn_per_ip;
        if max == 0 {
            return true;
        }
        let mut entry = self.conn_per_ip.entry(ip).or_insert(0);
        if *entry >= max {
            return false;
        }
        *entry = entry.saturating_add(1);
        true
    }

    /// Release a connection slot for `ip`.
    pub fn release_ip(&self, ip: IpAddr) {
        if self.cfg.max_conn_per_ip == 0 {
            return;
        }
        if let Some(mut entry) = self.conn_per_ip.get_mut(&ip) {
            *entry = entry.saturating_sub(1);
        }
    }

    /// Allocate a connection id.
    #[must_use]
    pub fn alloc_conn(&self) -> crate::room::ConnId {
        crate::room::ConnId(self.next_conn.fetch_add(1, Ordering::Relaxed))
    }

    /// Server config snapshot.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.cfg
    }

    /// Ticket HMAC key.
    #[must_use]
    pub fn ticket_key(&self) -> &TicketKey {
        &self.ticket_key
    }

    /// Create a room and spawn its actor.
    ///
    /// # Errors
    ///
    /// Capacity / RNG.
    pub fn create_room(&self, req: CreateRoomRequest) -> Result<CreateRoomResponse, RegistryError> {
        if self.is_draining() {
            return Err(RegistryError::Draining);
        }
        if self.rooms.len() >= self.cfg.max_rooms {
            return Err(RegistryError::Full);
        }
        let want_verifier = req.enable_verifier && self.cfg.enable_verifier;
        if want_verifier {
            let cur = self.verified_rooms.load(Ordering::Relaxed);
            if cur >= self.cfg.max_verified_rooms {
                return Err(RegistryError::VerifierFull);
            }
        }
        let max_players = req.max_players.clamp(2, 8);
        let visibility = match req.visibility.to_ascii_lowercase().as_str() {
            "private" => "private".into(),
            _ => "public".into(),
        };
        let settings = RoomSettings {
            map: req.map,
            skill: req.skill.clamp(1, 5),
            max_players,
            visibility,
            enable_verifier: want_verifier,
        };
        let id = new_room_id().map_err(|_| RegistryError::Rng)?;
        let rng_seed = new_rng_seed();
        let handle = spawn_room(
            id.clone(),
            settings.clone(),
            rng_seed,
            self.cfg.clone(),
            self.clone(),
        );
        self.rooms.insert(id.clone(), handle);
        self.room_location
            .insert(id.clone(), self.cfg.node_id.clone());
        if want_verifier {
            self.verified_rooms.fetch_add(1, Ordering::Relaxed);
        }
        Ok(make_create_response(&id, settings, rng_seed))
    }

    /// Mint a one-time join ticket for a room.
    ///
    /// # Errors
    ///
    /// Missing room / RNG.
    pub fn issue_join_ticket(
        &self,
        room_id: &str,
        name: &str,
    ) -> Result<(String, TicketClaims), RegistryError> {
        if self.get(room_id).is_none() {
            return Err(RegistryError::NotFound);
        }
        let now = unix_now();
        mint_ticket(&self.ticket_key, room_id, name, now).map_err(RegistryError::from)
    }

    /// Verify and consume a ticket (one-time).
    ///
    /// # Errors
    ///
    /// Invalid / expired / replay / wrong room.
    pub fn consume_ticket(
        &self,
        token: &str,
        expected_room: &str,
    ) -> Result<TicketClaims, RegistryError> {
        let now = unix_now();
        let claims = verify_ticket(&self.ticket_key, token, expected_room, now)?;
        if self
            .used_tickets
            .insert(claims.ticket_id.clone(), claims.exp)
            .is_some()
        {
            return Err(RegistryError::Ticket(TicketError::Invalid));
        }
        // Opportunistic prune of expired ids.
        if self.used_tickets.len() > 10_000 {
            self.used_tickets.retain(|_, exp| *exp + 60 > now);
        }
        Ok(claims)
    }

    /// Look up a room by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<RoomHandle> {
        self.rooms.get(id).map(|r| r.clone())
    }

    /// Number of live rooms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// Snapshot of room ids (sorted).
    #[must_use]
    pub fn room_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.rooms.iter().map(|e| e.key().clone()).collect();
        ids.sort();
        ids
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// Remove a room id from the map (actor may already be stopped).
    pub fn remove(&self, id: &str) {
        if let Some((_, h)) = self.rooms.remove(id) {
            if h.settings.enable_verifier {
                self.verified_rooms.fetch_sub(1, Ordering::Relaxed);
            }
        }
        self.room_location.remove(id);
    }

    /// Count of rooms with the optional verifier enabled.
    #[must_use]
    pub fn verified_room_count(&self) -> usize {
        self.verified_rooms.load(Ordering::Relaxed)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

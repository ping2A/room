//! Multi-room game server library (`docs/22-room-server.md`).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod admin;
mod admin_auth;
mod bans;
mod br_stats;
mod config;
mod content;
mod http;
mod leaderboard;
mod limits;
mod load_plan;
mod lobby;
mod mods;
mod names;
mod registry;
mod room;
mod security;
mod state;
mod ticket;
mod util;

#[cfg(feature = "verifier")]
mod verifier;

#[cfg(feature = "verifier")]
pub use verifier::{VerifierChecksum, spawn_verifier_with_output as spawn_verifier_for_test};

pub use admin::{Summary as AdminSummary, admin_routes};
pub use admin_auth::AuthStore;
pub use bans::{
    BanHit, BanSubject, SCOPES as BAN_SCOPES, find_ban_hit, normalize_scope, validate_ban,
};
pub use br_stats::{BrMatchRow, BrPlayerStats, BrStats, MODE_BR};
pub use config::{ServerConfig, TurnAuth};
/// Default `--content-max-mb`.
#[must_use]
pub const fn content_default_max_mb() -> u64 {
    content::DEFAULT_MAX_CONTENT_MB
}
pub use content::{
    ContentEntry, ContentKind, ContentStore, Preset, PresetConfig, PublicPreset, PublicPresets,
    seed_content,
};
pub use http::{
    JoinRoomRequest, JoinRoomResponse, admin_router, bind_ephemeral, public_router, router, serve,
};
pub use leaderboard::{Leaderboard, MatchRow, PlayerStats};
pub use lobby::{PublicRoom, lobby_routes};
pub use mods::{AdminModRow, MAX_MOD_BYTES, ModCatalog, ModEntry, ModListing};
pub use names::{MAX_NAME_CHARS, sanitize_display_name};
pub use registry::{RegistryError, RoomRegistry};
pub use room::{
    AdminCmd, CreateRoomRequest, CreateRoomResponse, PoseInspect, RoomCmd, RoomHandle, RoomInspect,
    RoomSettings, SeatInspect,
};
pub use state::{AdminStore, AppState, Ban};
pub use ticket::{TicketClaims, TicketError, TicketKey, mint_ticket, verify_ticket};

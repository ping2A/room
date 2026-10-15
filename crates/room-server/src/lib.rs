//! Multi-room game server library (`docs/22-room-server.md`).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod admin;
mod bans;
mod config;
mod http;
mod limits;
mod lobby;
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
pub use bans::{
    BanHit, BanSubject, SCOPES as BAN_SCOPES, find_ban_hit, normalize_scope, validate_ban,
};
pub use config::{ServerConfig, TurnAuth};
pub use http::{
    JoinRoomRequest, JoinRoomResponse, admin_router, bind_ephemeral, public_router, router, serve,
};
pub use lobby::{PublicRoom, lobby_routes};
pub use names::{MAX_NAME_CHARS, sanitize_display_name};
pub use registry::{RegistryError, RoomRegistry};
pub use room::{
    AdminCmd, CreateRoomRequest, CreateRoomResponse, PoseInspect, RoomCmd, RoomHandle, RoomInspect,
    RoomSettings, SeatInspect,
};
pub use state::{AdminStore, AppState, Ban};
pub use ticket::{TicketClaims, TicketError, TicketKey, mint_ticket, verify_ticket};

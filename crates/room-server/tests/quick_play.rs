//! S-BR4: quick-play BR queue creates/joins an 8-seat room.
#![allow(clippy::unwrap_used)]

use room_server::{ModCatalog, RoomRegistry, ServerConfig};
use std::sync::Arc;

fn registry_with_royale() -> RoomRegistry {
    let wad = include_bytes!("../../../mods/doom1-royale/royale.wad");
    let mut mods = ModCatalog::default();
    mods.insert("royale", wad.to_vec(), None).unwrap();
    RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        quick_play_timeout_secs: 60,
        mods: Arc::new(mods),
        ..ServerConfig::default()
    })
}

#[tokio::test]
async fn quick_play_reuses_open_room() {
    let registry = registry_with_royale();
    let (ticket_a, claims_a) = registry.quick_play_br("Alpha").unwrap();
    let (ticket_b, claims_b) = registry.quick_play_br("Bravo").unwrap();
    assert_eq!(claims_a.room_id, claims_b.room_id);
    assert_ne!(ticket_a, ticket_b);
    let h = registry.get(&claims_a.room_id).unwrap();
    assert_eq!(h.settings.max_players, 8);
    assert!(h.settings.fill_bots);
    assert!(h.settings.quick_play);
    assert_eq!(h.settings.game_mod.as_ref().unwrap().name, "royale");
}

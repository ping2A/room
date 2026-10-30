//! Supply drops, vault, and interact progress (`docs/28` §6.2 / BR4).

use core::fmt::Write;

use room_script_sdk::game;
use room_script_sdk::game::Actor;
use room_script_sdk::hud::{Hud, anchor, font, layer};
use room_script_sdk::ClassRef;

use crate::host::Host;
use crate::loot::LootSpawner;
use crate::rarity::Rarity;
use crate::tables::{Item, MAX_PLAYERS, TICRATE};
use crate::text::Msg;
use crate::ui::id;
use crate::zone::{dist2, xy, ZoneRuntime};

/// Seconds after loot start when the inbound marker appears (15 s before land; §6.2).
pub const SUPPLY_ANNOUNCE_SECS: i32 = 30;
/// First supply crate lands (`docs/28` §5 — still inside the 60 s loot grace).
pub const SUPPLY_LAND_SECS: i32 = 45;
pub const SUPPLY_OPEN_RANGE2: f64 = 64.0 * 64.0;
pub const SUPPLY_OPEN_SECS: i32 = 2;
pub const DEATH_MARKER_SECS: i32 = 10;

mod capture {
    pub const USE: u32 = 1 << 3;
}

mod bt {
    pub const USE: u32 = 1 << 1;
}

const MARKER_SUPPLY: u16 = 60;
const MARKER_VAULT: u16 = 61;
const MARKER_DEATH: u16 = 62;

/// Supply drop + vault + death-loot markers.
#[derive(Clone, Copy, Debug, Default)]
pub struct LootExtras {
    supply_pos: [f64; 3],
    supply_announced: bool,
    supply_landed: bool,
    supply_opened: bool,
    supply_crate: Actor,
    open_progress: [i32; MAX_PLAYERS],
    capturing: [bool; MAX_PLAYERS],
    vault_pos: [f64; 3],
    vault_ready: bool,
    vault_opened: bool,
    death_marker_until: [i32; MAX_PLAYERS],
    death_marker_pos: [[f64; 3]; MAX_PLAYERS],
}

impl LootExtras {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Pick vault spot in a dense inner area (hot zone near first circle centre).
    pub fn place_vault<H: Host>(
        &mut self,
        host: &mut H,
        center: [f64; 2],
        spots: &[[f64; 3]],
        nspots: usize,
    ) {
        if nspots == 0 {
            let z = host.floor_z(center[0], center[1]);
            self.vault_pos = [center[0], center[1], z];
            return;
        }
        let mut best_i = 0usize;
        let mut best_d = f64::MAX;
        for (i, s) in spots.iter().take(nspots).enumerate() {
            let d = dist2(xy(*s), center);
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        self.vault_pos = spots[best_i];
    }

    fn pick_in_circle<H: Host>(
        host: &mut H,
        center: [f64; 2],
        radius: f64,
        spots: &[[f64; 3]],
        nspots: usize,
    ) -> [f64; 3] {
        let r2 = radius * radius;
        let mut inside = [0usize; 64];
        let mut n = 0usize;
        for (i, s) in spots.iter().take(nspots).enumerate() {
            if dist2(xy(*s), center) <= r2 && n < inside.len() {
                inside[n] = i;
                n += 1;
            }
        }
        if n == 0 {
            let dx = host.random(-1000, 1000) as f64 / 1000.0 * radius * 0.5;
            let dy = host.random(-1000, 1000) as f64 / 1000.0 * radius * 0.5;
            let x = center[0] + dx;
            let y = center[1] + dy;
            let z = host.floor_z(x, y);
            return [x, y, z];
        }
        let pick = host.random(0, n as i32 - 1) as usize;
        spots[inside[pick]]
    }

    fn show_supply_marker(pos: [f64; 3]) {
        let _ = Hud::marker(MARKER_SUPPLY)
            .seat(-1)
            .layer(layer::OVER_HUD)
            .world_at(pos[0] as i32, pos[1] as i32, pos[2] as i32)
            .size(10, 10)
            .rgba(0xffcc_00ff)
            .text("DROP")
            .show();
    }

    fn show_vault_marker(pos: [f64; 3], locked: bool) {
        let _ = Hud::marker(MARKER_VAULT)
            .seat(-1)
            .layer(layer::OVER_HUD)
            .world_at(pos[0] as i32, pos[1] as i32, pos[2] as i32)
            .size(8, 8)
            .rgba(if locked { 0xff40_40ff } else { 0x40ff_80ff })
            .text(if locked { "VAULT" } else { "OPEN" })
            .show();
    }

    fn clear_supply_marker() {
        let _ = Hud::remove(-1, MARKER_SUPPLY);
    }

    fn clear_vault_marker() {
        let _ = Hud::remove(-1, MARKER_VAULT);
    }

    /// Per-tic: supply schedule, vault unlock, crate opening.
    #[allow(clippy::too_many_arguments)]
    pub fn tick<H: Host>(
        &mut self,
        host: &mut H,
        ctx: &crate::loot::LootCtx,
        t: i32,
        t_rel: i32,
        contender: &[bool; MAX_PLAYERS],
        zone_rt: &ZoneRuntime,
        spots: &[[f64; 3]],
        nspots: usize,
        supply_class: ClassRef,
    ) {
        let announce_at = SUPPLY_ANNOUNCE_SECS * TICRATE;
        let land_at = SUPPLY_LAND_SECS * TICRATE;

        if !self.supply_announced && t_rel >= announce_at {
            self.supply_announced = true;
            let (nc, nr) = zone_rt.next_circle();
            self.supply_pos = Self::pick_in_circle(host, nc, nr.max(64.0), spots, nspots);
            Self::show_supply_marker(self.supply_pos);
            host.print(None, "SUPPLY DROP INBOUND");
        }

        if self.supply_announced && !self.supply_landed && t_rel >= land_at {
            self.supply_landed = true;
            if let Some(crate_a) = host.spawn(supply_class, self.supply_pos, 0.0, 0) {
                self.supply_crate = crate_a;
                LootSpawner::spawn_fog(ctx, host, self.supply_pos);
                host.print(None, "Supply crate landed");
            }
        }

        if self.supply_landed && !self.supply_opened {
            self.tick_open_crate(host, ctx, t, contender);
        }

        if !self.vault_ready && zone_rt.geom.phase >= 3 {
            self.vault_ready = true;
            Self::clear_vault_marker();
            Self::show_vault_marker(self.vault_pos, false);
            host.print(None, "VAULT UNLOCKED");
        }

        if self.vault_ready && !self.vault_opened {
            self.vault_opened = true;
            let pos = self.vault_pos;
            LootSpawner::spawn_item(ctx, host, Item::GreenArmor, Rarity::Legendary, pos);
            LootSpawner::spawn_item(
                ctx,
                host,
                Item::RocketLauncher,
                Rarity::Legendary,
                [pos[0] + 32.0, pos[1], pos[2]],
            );
            LootSpawner::spawn_fog(ctx, host, pos);
        }

        for seat in 0..MAX_PLAYERS {
            if self.death_marker_until[seat] > 0 && t >= self.death_marker_until[seat] {
                self.death_marker_until[seat] = 0;
                let _ = Hud::remove(seat as i32, MARKER_DEATH);
            } else if self.death_marker_until[seat] > t {
                let p = self.death_marker_pos[seat];
                let _ = Hud::marker(MARKER_DEATH)
                    .seat(seat as i32)
                    .layer(layer::OVER_HUD)
                    .world_at(p[0] as i32, p[1] as i32, p[2] as i32)
                    .size(8, 8)
                    .rgba(0xff80_40ff)
                    .text("LOOT")
                    .show();
            }
        }
    }

    fn tick_open_crate<H: Host>(
        &mut self,
        host: &mut H,
        ctx: &crate::loot::LootCtx,
        _t: i32,
        contender: &[bool; MAX_PLAYERS],
    ) {
        let pos = self.supply_pos;
        let mut any_near = false;
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            if host.actor_is_dead(body) {
                self.release_capture(s);
                continue;
            }
            let p = host.actor_pos(body);
            let near = dist2([p[0], p[1]], [pos[0], pos[1]]) <= SUPPLY_OPEN_RANGE2;
            if near {
                any_near = true;
                if !self.capturing[s] {
                    game::player_capture_input(s as i32, capture::USE);
                    self.capturing[s] = true;
                }
                let mut buf = [0u8; 24];
                let _ = game::player_cmd(s as i32, &mut buf);
                let buttons = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                if buttons & bt::USE != 0 {
                    self.open_progress[s] += 1;
                } else {
                    self.open_progress[s] = 0;
                }
                let need = SUPPLY_OPEN_SECS * TICRATE;
                let mut bar = Msg::new();
                let _ = write!(
                    bar,
                    "OPEN CRATE {}%",
                    (self.open_progress[s] * 100 / need.max(1)).min(100)
                );
                let _ = Hud::text(id::INTERACT)
                    .seat(s as i32)
                    .anchor(anchor::BOTTOM_CENTER)
                    .at(0, -48)
                    .font(font::SMALL)
                    .layer(layer::OVER_HUD)
                    .text(bar.as_str())
                    .show();
                let _ = Hud::bar(id::INTERACT + 1)
                    .seat(s as i32)
                    .anchor(anchor::BOTTOM_CENTER)
                    .at(-40, -38)
                    .size(80, 4)
                    .layer(layer::OVER_HUD)
                    .value(self.open_progress[s])
                    .max(need)
                    .rgba(0xffcc_00ff)
                    .rgba2(0x2020_20ff)
                    .show();
                if self.open_progress[s] >= need {
                    self.finish_open(host, ctx, s);
                    return;
                }
            } else {
                self.release_capture(s);
                self.open_progress[s] = 0;
                let _ = Hud::remove(s as i32, id::INTERACT);
                let _ = Hud::remove(s as i32, id::INTERACT + 1);
            }
        }
        if !any_near {
            for s in 0..MAX_PLAYERS {
                let _ = Hud::remove(s as i32, id::INTERACT);
                let _ = Hud::remove(s as i32, id::INTERACT + 1);
            }
        }
    }

    fn finish_open<H: Host>(
        &mut self,
        host: &mut H,
        ctx: &crate::loot::LootCtx,
        opener: usize,
    ) {
        self.supply_opened = true;
        if !self.supply_crate.is_null() {
            host.actor_destroy(self.supply_crate);
            self.supply_crate = Actor::NULL;
        }
        Self::clear_supply_marker();
        let pos = self.supply_pos;
        LootSpawner::spawn_item(
            ctx,
            host,
            Item::Chaingun,
            Rarity::Legendary,
            [pos[0] + 24.0, pos[1], pos[2]],
        );
        LootSpawner::spawn_item(
            ctx,
            host,
            Item::GreenArmor,
            Rarity::Legendary,
            [pos[0], pos[1] + 24.0, pos[2]],
        );
        host.print(None, "SUPPLY OPENED");
        host.print(
            Some(opener as i32),
            "Legendary loot from the supply crate!",
        );
        for s in 0..MAX_PLAYERS {
            self.release_capture(s);
            self.open_progress[s] = 0;
            let _ = Hud::remove(s as i32, id::INTERACT);
            let _ = Hud::remove(s as i32, id::INTERACT + 1);
        }
    }

    fn release_capture(&mut self, seat: usize) {
        if self.capturing[seat] {
            game::player_capture_input(seat as i32, 0);
            self.capturing[seat] = false;
        }
    }

    pub fn on_loaded_markers(&self) {
        Self::show_vault_marker(self.vault_pos, true);
    }

    pub fn on_match_over(&mut self) {
        Self::clear_supply_marker();
        Self::clear_vault_marker();
        for s in 0..MAX_PLAYERS {
            self.release_capture(s);
            let _ = Hud::remove(s as i32, MARKER_DEATH);
        }
    }

    /// Killer HUD marker for freshly dropped death loot.
    pub fn mark_death_loot(&mut self, killer_seat: usize, pos: [f64; 3], until: i32) {
        if killer_seat >= MAX_PLAYERS {
            return;
        }
        self.death_marker_pos[killer_seat] = pos;
        self.death_marker_until[killer_seat] = until;
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn supply_pos(&self) -> Option<[f64; 3]> {
        if self.supply_announced {
            Some(self.supply_pos)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;
    use crate::zone::ZoneRuntime;

    #[test]
    fn pick_in_circle_uses_spots() {
        let mut host = FakeHost::new(42);
        let center = [1000.0, 1000.0];
        let mut spots = [[0.0; 3]; 4];
        spots[0] = [1010.0, 1010.0, 0.0];
        spots[1] = [2000.0, 2000.0, 0.0];
        let p = LootExtras::pick_in_circle(&mut host, center, 500.0, &spots, 2);
        assert!(dist2(xy(p), center) <= 500.0 * 500.0 + 0.01);
    }

    #[test]
    fn supply_announces_at_30s_loot() {
        let mut host = FakeHost::new(1);
        let mut ex = LootExtras::default();
        ex.place_vault(&mut host, [0.0; 2], &[], 0);
        let zr = ZoneRuntime::new([512.0, 512.0], 800.0);
        let t_rel = SUPPLY_ANNOUNCE_SECS * TICRATE;
        let ctx = crate::loot::LootCtx {
            items: [ClassRef::NULL; 15],
            fog: ClassRef::NULL,
            rarity: crate::rarity::RarityTable::default(),
        };
        ex.tick(
            &mut host,
            &ctx,
            t_rel,
            t_rel,
            &[true, false, false, false, false, false, false, false],
            &zr,
            &[[600.0, 600.0, 0.0]],
            1,
            ClassRef::NULL,
        );
        assert!(ex.supply_announced);
        assert!(host.prints.iter().any(|(_, m)| m == "SUPPLY DROP INBOUND"));
    }
}

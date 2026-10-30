//! Phased bosses (`docs/27` §6.7).

use crate::announcer::{Announcer, Priority};
use crate::host::{default_spawn_flags, Host, MASTER_PTR, TARGET_PTR};
use room_script_sdk::game::{class_flag, player_prop, spawn_flags};
use crate::tables::TICRATE;
use crate::team::Team;
use room_script_sdk::game::{actor_f64, Actor};
use room_script_sdk::hud::{Hud, anchor, font, layer};
use room_script_sdk::ClassRef;

const BOSS_BAR: u16 = 30;
const BOSS_NAME: u16 = 31;

/// Active boss runtime.
#[derive(Clone, Copy, Debug, Default)]
pub struct BossState {
    pub actor: Actor,
    pub spawn_health: i32,
    pub phase: u8,
    pub class: ClassRef,
    pub active: bool,
    /// Maptime when phase-3 invuln ends (`0` = none).
    pub invuln_until: i32,
}

impl BossState {
    /// Spawn the Warden (Baron subclass) when `warden` resolves.
    pub fn spawn_warden(
        &mut self,
        host: &mut impl Host,
        ann: &mut Announcer,
        team: &Team,
        warden: ClassRef,
        pos: [f64; 3],
        players: i32,
    ) -> bool {
        if warden.is_null() {
            return false;
        }
        // Bosses are large; skip CHECK_POSITION so sealed-closet / tight spots still work.
        let flags = spawn_flags::TELEFOG;
        let Some(a) = host.spawn(warden, pos, 0.0, flags) else {
            return false;
        };
        let base = host.actor_health(a).max(1000);
        let hp = (base as f64 * (1.0 + 0.6 * f64::from(players - 1))) as i32;
        host.actor_set_health(a, hp);
        if let Some(t) = team.nearest_alive(host, pos) {
            host.actor_set_ref(a, TARGET_PTR, Some(t));
        }
        host.actor_set_state(a, "See");
        self.actor = a;
        self.spawn_health = hp;
        self.phase = 1;
        self.class = warden;
        self.active = true;
        self.invuln_until = 0;
        ann.push(host, None, Priority::Critical, "The Warden arrives!");
        self.patch_bar(host);
        true
    }

    /// Phase transitions at 66% / 33%.
    pub fn on_tick(&mut self, host: &mut impl Host, ann: &mut Announcer, team: &Team, now: i32) {
        if !self.active || !host.actor_exists(self.actor) || host.actor_is_dead(self.actor) {
            if self.active {
                self.active = false;
                let _ = Hud::remove(-1, BOSS_BAR);
                let _ = Hud::remove(-1, BOSS_NAME);
            }
            return;
        }
        let hp = host.actor_health(self.actor);
        let pct = if self.spawn_health > 0 {
            (hp * 100) / self.spawn_health
        } else {
            0
        };
        if self.phase == 1 && pct <= 66 {
            self.phase = 2;
            let sp = host.actor_get_f64(self.actor, actor_f64::SPEED);
            let sp = if sp <= 0.0 { 8.0 } else { sp };
            host.actor_set_f64(self.actor, actor_f64::SPEED, sp * 1.3);
            let imp = host.class_find("DoomImp");
            if !imp.is_null() {
                let pos = host.actor_pos(self.actor);
                for i in 0..4 {
                    let p = [pos[0] + f64::from(i) * 24.0, pos[1], pos[2]];
                    if let Some(s) = host.spawn(imp, p, 0.0, default_spawn_flags()) {
                        host.actor_set_ref(s, MASTER_PTR, Some(self.actor));
                        if let Some(t) = team.nearest_alive(host, p) {
                            host.actor_set_ref(s, TARGET_PTR, Some(t));
                        }
                        host.actor_set_state(s, "See");
                    }
                }
            }
            ann.push(host, None, Priority::High, "WARDEN - reinforcements!");
        } else if self.phase == 2 && pct <= 33 {
            self.phase = 3;
            host.actor_set_flag(self.actor, class_flag::INVULNERABLE as i32, true);
            self.invuln_until = now + 3 * TICRATE;
            for s in 0..8 {
                if host.player_in_game(s) {
                    host.player_set_i32(s, player_prop::DAMAGECOUNT, 18);
                }
            }
            ann.push(host, None, Priority::Critical, "WARDEN ENRAGED!");
        }
        if self.phase == 3 && self.invuln_until > 0 && now >= self.invuln_until {
            self.invuln_until = 0;
            host.actor_set_flag(self.actor, class_flag::INVULNERABLE as i32, false);
            let sp = host.actor_get_f64(self.actor, actor_f64::SPEED);
            let sp = if sp <= 0.0 { 8.0 } else { sp };
            host.actor_set_f64(self.actor, actor_f64::SPEED, sp * 1.2);
        }
        if now % 5 == 0 {
            self.patch_bar(host);
        }
    }

    fn patch_bar(&self, host: &impl Host) {
        if !self.active {
            return;
        }
        let hp = host.actor_health(self.actor).max(0);
        let _ = Hud::bar(BOSS_BAR)
            .seat(-1)
            .anchor(anchor::TOP_CENTER)
            .at(0, 40)
            .size(200, 6)
            .layer(layer::OVER_HUD)
            .value(hp)
            .max(self.spawn_health.max(1))
            .rgba(0xff20_20ff)
            .rgba2(0x2020_20ff)
            .show();
        let _ = Hud::text(BOSS_NAME)
            .seat(-1)
            .anchor(anchor::TOP_CENTER)
            .at(0, 28)
            .font(font::BIG)
            .layer(layer::OVER_HUD)
            .color("Red")
            .text("WARDEN")
            .show();
    }

    #[must_use]
    pub fn actor(&self) -> Option<Actor> {
        self.active.then_some(self.actor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::announcer::TEAM_GAP_TICS;
    use crate::host::FakeHost;
    use crate::team::Team;

    #[test]
    fn warden_phases_at_66_and_33_percent() {
        let mut host = FakeHost::new(4, 1);
        let _ = host.ensure_class("DoomImp");
        let warden = host.ensure_class("Warden");
        host.set_class_parent("Warden", "BaronOfHell");
        let _ = host.add_player(0, [0.0, 0.0, 0.0]);
        let mut team = Team::default();
        team.refresh(&host);
        let mut ann = Announcer::default();
        let mut boss = BossState::default();
        assert!(boss.spawn_warden(
            &mut host,
            &mut ann,
            &team,
            warden,
            [200.0, 0.0, 0.0],
            1
        ));
        assert_eq!(boss.phase, 1);
        let max = boss.spawn_health;
        host.maptime = 100;
        host.actor_set_health(boss.actor, (max * 66) / 100);
        boss.on_tick(&mut host, &mut ann, &team, 100);
        assert_eq!(boss.phase, 2, "66% → phase 2");
        let t3 = 100 + TEAM_GAP_TICS;
        host.maptime = t3;
        host.actor_set_health(boss.actor, (max * 33) / 100);
        boss.on_tick(&mut host, &mut ann, &team, t3);
        assert_eq!(boss.phase, 3, "33% → phase 3");
        assert!(boss.invuln_until > 0);
        assert!(host
            .prints
            .iter()
            .any(|(_, m)| m.contains("reinforcements") || m.contains("WARDEN")));
        assert!(host.prints.iter().any(|(_, m)| m.contains("enrage") || m.contains("ENRAG")));
    }
}

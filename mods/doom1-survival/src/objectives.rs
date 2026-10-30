//! Round objectives (`docs/27` §6.6).

use crate::announcer::{Announcer, Priority};
use crate::host::Host;
use crate::tables::TICRATE;
use crate::team::Team;
use crate::text::Msg;
use core::fmt::Write;
use room_script_sdk::hud::{Hud, anchor, font, layer};

/// Objective tracker widget id.
const WIDGET: u16 = 20;
/// Hold-point world marker.
const MARKER: u16 = 21;

/// Active objective kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// No objective this round.
    None,
    /// Stay near beacon for cumulative seconds.
    HoldPoint,
    /// No player below 50 % health.
    Flawless,
}

/// Runtime objective state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Objectives {
    pub kind: Option<ObjectiveKind>,
    pub progress: i32,
    pub goal: i32,
    pub fails: bool,
    pub success: bool,
    pub beacon: [f64; 3],
    pub started_at: i32,
}

impl Objectives {
    /// Roll an objective for `wave` (boss rounds skip).
    pub fn begin_round(
        &mut self,
        host: &mut impl Host,
        ann: &mut Announcer,
        wave: i32,
        chance: i32,
        team: &Team,
    ) {
        self.kind = None;
        self.progress = 0;
        self.goal = 0;
        self.fails = false;
        self.success = false;
        if wave < 2 || wave % 5 == 0 {
            return;
        }
        if host.random(0, 99) >= chance {
            return;
        }
        let roll = host.random(0, 1);
        let kind = if roll == 0 {
            ObjectiveKind::HoldPoint
        } else {
            ObjectiveKind::Flawless
        };
        self.kind = Some(kind);
        self.started_at = host.maptime();
        match kind {
            ObjectiveKind::HoldPoint => {
                self.goal = 25 * TICRATE;
                if let Some(p) = team.first_alive(host) {
                    let pp = host.actor_pos(p);
                    self.beacon = [pp[0] + 256.0, pp[1], pp[2]];
                }
                let _ = Hud::marker(MARKER)
                    .seat(-1)
                    .layer(layer::OVER_HUD)
                    .world_at(
                        self.beacon[0] as i32,
                        self.beacon[1] as i32,
                        self.beacon[2] as i32,
                    )
                    .size(8, 8)
                    .rgba(0x40ff_40ff)
                    .text("HOLD")
                    .show();
                ann.push(host, None, Priority::High, "OBJECTIVE: Hold the beacon");
            }
            ObjectiveKind::Flawless => {
                self.goal = 1;
                ann.push(host, None, Priority::High, "OBJECTIVE: Flawless - stay above 50%");
            }
            ObjectiveKind::None => {}
        }
        self.patch_hud();
    }

    /// Per-tic progress / fail checks.
    pub fn on_tick(&mut self, host: &impl Host, team: &Team) {
        let Some(kind) = self.kind else {
            return;
        };
        if self.success || self.fails {
            return;
        }
        match kind {
            ObjectiveKind::HoldPoint => {
                let mut any = false;
                for p in team.iter_alive(host) {
                    let pp = host.actor_pos(p);
                    let (dx, dy) = (pp[0] - self.beacon[0], pp[1] - self.beacon[1]);
                    if host.sqrt(dx * dx + dy * dy) <= 128.0 {
                        any = true;
                        break;
                    }
                }
                if any {
                    self.progress = (self.progress + 1).min(self.goal);
                    if self.progress >= self.goal {
                        self.success = true;
                    }
                }
            }
            ObjectiveKind::Flawless => {
                for p in team.iter_alive(host) {
                    let hp = host.actor_health(p);
                    if hp > 0 && hp < 50 {
                        self.fails = true;
                        break;
                    }
                }
            }
            ObjectiveKind::None => {}
        }
        if host.maptime() % 5 == 0 {
            self.patch_hud();
        }
    }

    /// On player damage — Flawless fail path.
    pub fn on_damaged(&mut self, health: i32) {
        if self.kind == Some(ObjectiveKind::Flawless) && health > 0 && health < 50 {
            self.fails = true;
        }
    }

    /// Resolve at round clear: Marks bonus, announce.
    pub fn resolve(
        &mut self,
        host: &mut impl Host,
        ann: &mut Announcer,
        economy: &mut crate::economy::Economy,
        alive_slots: &[usize],
    ) {
        let Some(kind) = self.kind else {
            return;
        };
        if kind == ObjectiveKind::Flawless && !self.fails {
            self.success = true;
        }
        if self.success {
            let bonus = match kind {
                ObjectiveKind::HoldPoint => 60,
                ObjectiveKind::Flawless => 100,
                ObjectiveKind::None => 0,
            };
            for &s in alive_slots {
                economy.marks[s] = economy.marks[s].saturating_add(bonus);
            }
            ann.push(host, None, Priority::High, "Objective complete!");
        } else if self.fails || kind == ObjectiveKind::HoldPoint {
            ann.push(host, None, Priority::Normal, "Objective failed.");
        }
        self.kind = None;
        let _ = Hud::remove(-1, WIDGET);
        let _ = Hud::remove(-1, MARKER);
        for s in 0..8 {
            let _ = Hud::remove(s as i32, WIDGET);
            let _ = Hud::remove(s as i32, MARKER);
        }
    }

    fn patch_hud(&self) {
        let Some(kind) = self.kind else {
            return;
        };
        let mut line = Msg::new();
        match kind {
            ObjectiveKind::HoldPoint => {
                let sec = self.progress / TICRATE;
                let goal = self.goal / TICRATE;
                let _ = write!(line, "Hold {sec}/{goal}s");
            }
            ObjectiveKind::Flawless => {
                let _ = write!(
                    line,
                    "Flawless {}",
                    if self.fails { "FAILED" } else { "OK" }
                );
            }
            ObjectiveKind::None => return,
        }
        let _ = Hud::text(WIDGET)
            .seat(-1)
            .anchor(anchor::TOP_LEFT)
            .at(8, 28)
            .font(font::SMALL)
            .layer(layer::OVER_HUD)
            .text(line.as_str())
            .show();
    }
}

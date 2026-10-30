//! Survival director: phases, threat budget, intensity (SV2) + elites/announcer (SV3).

use crate::announcer::{Announcer, Priority};
use crate::bosses::BossState;
use crate::economy::Economy;
use crate::elites::{Affix, EliteState};
use crate::encounters::{pick_archetype, Archetype};
use crate::host::{default_spawn_flags, Host, MASTER_PTR, TARGET_PTR};
use crate::objectives::Objectives;
use crate::perks::PerkState;
use crate::register;
use crate::ui::ModUi;
use crate::spots::{
    collect_spots, face_player, mark_proven, mark_stuck, pick_spot, proven_near_team, update_stuck,
    Spot, Tracked,
};
use crate::tables::{
    alive_cap, map_music_name, round_budget, tuning_for_skill, BARON_THREAT, BUILDUP_INTENSITY,
    BUILDUP_TICS, INTERMISSION_TICS, MAX_SPOTS, MAX_WAVE, PEAK_FADE_TICS, PEAK_INTENSITY, POOL,
    POOL_THREAT, STUCK_CHECK_PERIOD, STRAGGLER_TICS, STREAK_TICS, TICRATE, Tuning,
};
use crate::team::Team;
use crate::{say, say_prio};
use room_script_sdk::game::{event_mask, Actor, Event, RoomMod};
use room_script_sdk::ClassRef;

/// Director phase (`docs/27` §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Pre-round loot / announcement.
    Warmup,
    /// Trickle spawns, low intensity (entered from adaptive pacing later).
    #[allow(dead_code)]
    BuildUp,
    /// Main budget release.
    Assault,
    /// Hold intensity / stop releasing.
    Peak,
    /// Hunt stragglers.
    Clear,
    /// Between rounds.
    Intermission,
    /// Every 5th round boss (SV6 fleshes out).
    Boss,
    /// Extraction finale (SV5).
    Extraction,
}

impl Phase {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warmup => "WARMUP",
            Self::BuildUp => "BUILD UP",
            Self::Assault => "ASSAULT",
            Self::Peak => "PEAK",
            Self::Clear => "CLEAR",
            Self::Intermission => "INTERMISSION",
            Self::Boss => "BOSS",
            Self::Extraction => "EXTRACTION",
        }
    }
}

/// Owned director state for one game session.
pub struct Survival {
    spots: [Spot; MAX_SPOTS],
    nspots: usize,
    pool: [ClassRef; POOL.len()],
    baron: ClassRef,
    clip: ClassRef,
    shell: ClassRef,
    telefog: ClassRef,
    wave: i32,
    next_wave_at: i32,
    wave_spawned_at: i32,
    alive: [Tracked; MAX_WAVE],
    nalive: usize,
    streak: i32,
    last_kill: i32,
    warned_low_at: i32,
    music: [u8; 8],
    music_len: usize,
    team: Team,
    phase: Phase,
    phase_started_at: i32,
    archetype: Archetype,
    prev_archetype: Option<Archetype>,
    round_budget_left: i32,
    alive_threat: i32,
    intensity: i32,
    peak_since: i32,
    saw_peak_fade: bool,
    announcer: Announcer,
    elites: EliteState,
    economy: Economy,
    perks: PerkState,
    ui: ModUi,
    objectives: Objectives,
    score: i32,
    extraction_hold: i32,
    boss: BossState,
    warden: ClassRef,
}

impl Default for Survival {
    fn default() -> Self {
        Self {
            spots: [Spot::default(); MAX_SPOTS],
            nspots: 0,
            pool: [ClassRef::NULL; POOL.len()],
            baron: ClassRef::NULL,
            clip: ClassRef::NULL,
            shell: ClassRef::NULL,
            telefog: ClassRef::NULL,
            wave: 0,
            next_wave_at: 0,
            wave_spawned_at: 0,
            alive: [Tracked::default(); MAX_WAVE],
            nalive: 0,
            streak: 0,
            last_kill: -STREAK_TICS,
            warned_low_at: -10 * TICRATE,
            music: [0; 8],
            music_len: 0,
            team: Team::default(),
            phase: Phase::Warmup,
            phase_started_at: 0,
            archetype: Archetype::Swarm,
            prev_archetype: None,
            round_budget_left: 0,
            alive_threat: 0,
            intensity: 0,
            peak_since: -1,
            saw_peak_fade: false,
            announcer: Announcer::default(),
            elites: EliteState::default(),
            economy: Economy::default(),
            perks: PerkState::default(),
            ui: ModUi::default(),
            objectives: Objectives::default(),
            score: 0,
            extraction_hold: 0,
            boss: BossState::default(),
            warden: ClassRef::NULL,
        }
    }
}

impl Survival {
    fn tuning(&self, host: &impl Host) -> &'static Tuning {
        tuning_for_skill(host.skill())
    }

    fn players_alive(&self) -> i32 {
        i32::from(self.team.alive.max(1))
    }

    fn refresh_team(&mut self, host: &impl Host) {
        self.team.refresh(host);
    }

    fn enter_phase(&mut self, host: &mut impl Host, phase: Phase, now: i32) {
        self.phase = phase;
        self.phase_started_at = now;
        if self.wave > 0 {
            let arch = if matches!(phase, Phase::BuildUp | Phase::Assault | Phase::Boss) {
                self.archetype.label()
            } else {
                phase.label()
            };
            say_prio!(
                &mut self.announcer,
                host,
                Priority::High,
                "ROUND {} - {}",
                self.wave,
                arch
            );
        } else if phase != Phase::Warmup {
            say!(&mut self.announcer, host, "Phase: {}", phase.label());
        }
    }

    fn begin_round(&mut self, host: &mut impl Host, now: i32) {
        self.ui.close_all();
        self.refresh_team(host);
        self.wave += 1;
        self.nalive = 0;
        self.alive_threat = 0;
        self.wave_spawned_at = now;
        self.saw_peak_fade = false;
        self.peak_since = -1;
        self.archetype = pick_archetype(host, self.wave, self.prev_archetype);
        self.prev_archetype = Some(self.archetype);
        self.round_budget_left = round_budget(self.wave, host.skill(), self.players_alive());
        let obj_chance = self.tuning(host).objective_chance;
        let wave = self.wave;
        self.objectives.begin_round(
            host,
            &mut self.announcer,
            wave,
            obj_chance,
            &self.team,
        );
        let boss = self.archetype == Archetype::Boss;
        if boss {
            self.enter_phase(host, Phase::Boss, now);
            if self.music_len > 0 {
                host.set_music("D_E1M8");
            }
            say_prio!(
                &mut self.announcer,
                host,
                Priority::High,
                "BOSS: The Warden approaches!"
            );
            Announcer::cue(host, self.team.first_alive(host), "misc/teleport");
            let mut spawned_boss = false;
            // Prefer a healthy spot; if every spot was disabled by stuck recycle,
            // still place the boss (wave 5+ on sealed-closet maps).
            let boss_pos = pick_spot(host, &self.spots, self.nspots, &self.team, 8)
                .map(|(s, _)| s)
                .or_else(|| {
                    // Disabled-spot fallback: farthest from the team (never on the player).
                    let mut best: Option<([f64; 3], f64)> = None;
                    for s in self.spots.iter().take(self.nspots) {
                        let pos = s.pos();
                        let mut nearest = f64::MAX;
                        for p in self.team.iter_alive(host) {
                            let pp = host.actor_pos(p);
                            let (dx, dy) = (pos[0] - pp[0], pos[1] - pp[1]);
                            nearest = nearest.min(host.sqrt(dx * dx + dy * dy));
                        }
                        if nearest < 384.0 {
                            continue;
                        }
                        if best.is_none_or(|(_, d)| nearest > d) {
                            best = Some((pos, nearest));
                        }
                    }
                    best.map(|(p, _)| p)
                });
            if !self.warden.is_null() {
                if let Some(s) = boss_pos {
                    let alive_n = self.players_alive();
                    spawned_boss = self.boss.spawn_warden(
                        host,
                        &mut self.announcer,
                        &self.team,
                        self.warden,
                        s,
                        alive_n,
                    );
                    if spawned_boss {
                        if let Some(a) = self.boss.actor() {
                            self.alive[self.nalive] = Tracked {
                                actor: a,
                                spot_idx: 0,
                                spawn_tic: now,
                                best_dist: 512.0,
                                progress_tic: now,
                                threat_cost: BARON_THREAT,
                            };
                            self.nalive += 1;
                            self.alive_threat += BARON_THREAT;
                            self.round_budget_left = (self.round_budget_left - BARON_THREAT).max(0);
                        }
                    }
                }
            }
            if !spawned_boss {
                let _ = self.spawn_one(host, self.baron, BARON_THREAT, false);
            }
        } else {
            // Announce, then open with an Assault pulse so the first-wave
            // schedule matches SV0/SV1 e2e (BuildUp trickle is mid-fight only).
            self.enter_phase(host, Phase::Assault, now);
            let want = self.wave_size(host);
            say!(
                &mut self.announcer,
                host,
                "Wave {} - {} incoming",
                self.wave,
                want
            );
            // UV+: guarantee an elite by round 3; Elite Pack always leads with one.
            let force_elite = self.archetype == Archetype::ElitePack
                || (host.skill() >= 4 && self.wave <= 3 && self.elites.count() == 0);
            if force_elite {
                let _ = self.try_release_elite(host, true);
            }
            let mut left = want;
            let mut tries = 0;
            while left > 0 && tries < 64 {
                tries += 1;
                let before = self.nalive;
                self.try_release(host, self.archetype.max_unit_cost());
                if self.nalive > before {
                    left -= 1;
                }
            }
        }
    }

    fn wave_size(&self, host: &impl Host) -> i32 {
        (2 + self.wave + self.tuning(host).extra).min(MAX_WAVE as i32)
    }

    fn cap(&self, host: &impl Host) -> i32 {
        alive_cap(self.wave.max(1), host.skill(), self.players_alive())
    }

    fn class_threat(&self, class: ClassRef) -> i32 {
        if class == self.baron {
            return BARON_THREAT;
        }
        for (i, c) in self.pool.iter().enumerate() {
            if *c == class {
                return POOL_THREAT[i];
            }
        }
        3
    }

    fn try_release(&mut self, host: &mut impl Host, max_cost: i32) {
        if self.nalive == MAX_WAVE || self.round_budget_left <= 0 {
            return;
        }
        // Skip when every spot is disabled (sealed / learned-bad).
        let any_spot = self.spots.iter().take(self.nspots).any(|s| !s.disabled());
        if !any_spot {
            return;
        }
        let chance = self.tuning(host).elite_chance;
        let want_elite = self.archetype == Archetype::ElitePack
            || (self.wave >= 3 && host.random(0, 99) < chance);
        if want_elite && self.try_release_elite(host, false) {
            return;
        }
        let unlocked = (self.wave as usize).min(POOL.len()).max(1);
        let pick = host.random(0, unlocked as i32 - 1) as usize;
        let class = self.pool[pick];
        let cost = self.class_threat(class);
        if cost > max_cost || cost > self.round_budget_left {
            return;
        }
        if self.alive_threat + cost > self.cap(host) {
            return;
        }
        let _ = self.spawn_one(host, class, cost, false);
    }

    fn try_release_elite(&mut self, host: &mut impl Host, force: bool) -> bool {
        if self.elites.count() >= crate::elites::MAX_ELITES || self.nalive == MAX_WAVE {
            return false;
        }
        let unlocked = (self.wave as usize).min(POOL.len()).max(1);
        let pick = host.random(0, unlocked as i32 - 1) as usize;
        let base = self.pool[pick];
        let Some((elite_class, base_class)) = self.elites.elite_class_for(host, base) else {
            return false;
        };
        let base_threat = self.class_threat(base);
        let cost = (base_threat * crate::elites::ELITE_THREAT_NUM) / crate::elites::ELITE_THREAT_DEN;
        if !force {
            if cost > self.round_budget_left || self.alive_threat + cost > self.cap(host) {
                return false;
            }
        } else if self.alive_threat + cost > self.cap(host) && self.nalive > 0 {
            return false;
        }
        let affix = Affix::pick(host);
        self.spawn_elite(host, elite_class, base_class, affix, base_threat)
    }

    fn spawn_elite(
        &mut self,
        host: &mut impl Host,
        elite_class: ClassRef,
        base_class: ClassRef,
        affix: Affix,
        base_threat: i32,
    ) -> bool {
        let provisional = (base_threat * crate::elites::ELITE_THREAT_NUM)
            / crate::elites::ELITE_THREAT_DEN;
        if !self.spawn_one(host, elite_class, provisional, true) {
            return false;
        }
        // Last tracked actor is the elite body.
        let i = self.nalive - 1;
        let actor = self.alive[i].actor;
        let now = host.maptime();
        let cost = self.elites.promote(
            host,
            &mut self.announcer,
            actor,
            elite_class,
            base_class,
            affix,
            base_threat,
            now,
        );
        // Correct threat accounting to the promoted cost.
        self.alive_threat = (self.alive_threat - provisional + cost).max(0);
        self.alive[i].threat_cost = cost;
        // Elite Pack escorts: one cheap follower with master = elite.
        if self.archetype == Archetype::ElitePack && self.nalive < MAX_WAVE {
            let unlocked = (self.wave as usize).min(POOL.len()).max(1);
            let escort = self.pool[0.min(unlocked - 1)];
            let e_cost = self.class_threat(escort).min(3);
            if self.spawn_one(host, escort, e_cost, false) {
                let esc = self.alive[self.nalive - 1].actor;
                host.actor_set_ref(esc, MASTER_PTR, Some(actor));
            }
        }
        true
    }

    fn spawn_one(
        &mut self,
        host: &mut impl Host,
        class: ClassRef,
        cost: i32,
        _elite: bool,
    ) -> bool {
        if class.is_null() || self.nalive == MAX_WAVE {
            return false;
        }
        if self.alive_threat + cost > self.cap(host) {
            return false;
        }
        for _ in 0..8 {
            let Some((s, idx)) = pick_spot(host, &self.spots, self.nspots, &self.team, 8) else {
                continue;
            };
            let Some(target) = self.team.nearest_alive(host, s) else {
                return false;
            };
            let tp = host.actor_pos(target);
            let face = face_player(host, s, tp);
            if let Some(m) = host.spawn(class, s, face, default_spawn_flags()) {
                host.actor_set_ref(m, TARGET_PTR, Some(target));
                host.actor_set_state(m, "See");
                if let Some(spot) = self.spots.get_mut(idx) {
                    spot.uses = spot.uses.saturating_add(1);
                    spot.last_used = host.maptime();
                }
                let now = host.maptime();
                let nearest = {
                    let (dx, dy) = (s[0] - tp[0], s[1] - tp[1]);
                    host.sqrt(dx * dx + dy * dy)
                };
                self.alive[self.nalive] = Tracked {
                    actor: m,
                    spot_idx: idx as u16,
                    spawn_tic: now,
                    best_dist: nearest,
                    progress_tic: now,
                    threat_cost: cost,
                };
                self.nalive += 1;
                self.alive_threat += cost;
                self.round_budget_left = (self.round_budget_left - cost).max(0);
                return true;
            }
        }
        false
    }

    fn remove_alive_at(&mut self, i: usize) {
        if i >= self.nalive {
            return;
        }
        self.alive_threat = (self.alive_threat - self.alive[i].threat_cost).max(0);
        self.nalive -= 1;
        self.alive[i] = self.alive[self.nalive];
        self.alive[self.nalive] = Tracked::default();
    }

    fn wave_killed(&mut self, thing: Actor) -> bool {
        let Some(i) = self.alive[..self.nalive]
            .iter()
            .position(|t| t.actor == thing)
        else {
            return false;
        };
        self.remove_alive_at(i);
        true
    }

    fn recycle_monster(&mut self, host: &mut impl Host, i: usize, reason_stuck: bool) {
        if i >= self.nalive {
            return;
        }
        let t = self.alive[i];
        if reason_stuck {
            mark_stuck(&mut self.spots, t.spot_idx);
            // Do not refund `round_budget_left` — that re-opens Assault pulses on
            // the same bad spot and softlocks Clear. alive_threat already drops
            // in `remove_alive_at`.
        }
        if host.actor_exists(t.actor) {
            host.actor_destroy(t.actor);
        }
        self.remove_alive_at(i);
    }

    fn maybe_clear_wave(&mut self, host: &mut impl Host, killer: Option<Actor>, now: i32) {
        if self.nalive != 0 || self.wave == 0 {
            return;
        }
        if !matches!(self.phase, Phase::Assault | Phase::Peak | Phase::Boss | Phase::Clear | Phase::BuildUp)
        {
            return;
        }
        say!(&mut self.announcer, host, "Wave {} cleared!", self.wave);
        if let Some(k) = killer.filter(|&a| host.actor_exists(a)) {
            let _ = host.actor_give(k, self.clip, 20);
            let _ = host.actor_give(k, self.shell, 4 + self.wave);
        } else if let Some(p) = self.team.first_alive(host) {
            let _ = host.actor_give(p, self.clip, 20);
            let _ = host.actor_give(p, self.shell, 4 + self.wave);
        }
        if self.wave % 5 == 0 && self.music_len > 0 {
            let name = core::str::from_utf8(&self.music[..self.music_len]).unwrap_or("");
            host.set_music(name);
        }
        let mut alive_slots = [0usize; 8];
        let mut n_alive = 0usize;
        for slot in 0..crate::team::MAX_TEAM {
            if let Some(a) = host.player(slot as i32) {
                if !host.actor_is_dead(a) {
                    alive_slots[n_alive] = slot;
                    n_alive += 1;
                }
            }
        }
        self.economy
            .round_clear_bonus(&alive_slots[..n_alive], self.wave);
        self.score = self.score.saturating_add(100 * self.wave);
        self.objectives.resolve(
            host,
            &mut self.announcer,
            &mut self.economy,
            &alive_slots[..n_alive],
        );
        self.perks.clear_round_flags();
        if self.wave >= self.tuning(host).final_round {
            self.ui.close_all();
            self.enter_phase(host, Phase::Extraction, now);
            self.extraction_hold = 0;
            say_prio!(
                &mut self.announcer,
                host,
                Priority::Critical,
                "EXTRACTION - hold the zone!"
            );
            return;
        }
        self.enter_phase(host, Phase::Intermission, now);
        self.next_wave_at = now + INTERMISSION_TICS.min(self.tuning(host).wave_gap * TICRATE / 2);
        for &slot in &alive_slots[..n_alive] {
            self.ui
                .open_draft(host, slot, &self.perks, now, self.wave);
        }
        let mut in_game = [false; crate::team::MAX_TEAM];
        for slot in 0..crate::team::MAX_TEAM {
            in_game[slot] = host.player_in_game(slot as i32);
        }
        ModUi::patch_marks(&self.economy.marks, &in_game);
    }

    fn update_intensity(&mut self, host: &impl Host) {
        // Decay when not pressed; climb with nearby monsters.
        self.intensity = (self.intensity - 25).max(0);
        for t in &self.alive[..self.nalive] {
            if !host.actor_exists(t.actor) {
                continue;
            }
            let pos = host.actor_pos(t.actor);
            for p in self.team.iter_alive(host) {
                let pp = host.actor_pos(p);
                let (dx, dy) = (pos[0] - pp[0], pos[1] - pp[1]);
                if host.sqrt(dx * dx + dy * dy) < 512.0 && host.actor_can_see(t.actor, p) {
                    self.intensity = (self.intensity + 150).min(10_000);
                }
                if host.actor_health(p) < 30 {
                    self.intensity = (self.intensity + 40).min(10_000);
                }
            }
        }
    }

    fn maintain_alive(&mut self, host: &mut impl Host, maptime: i32) {
        self.refresh_team(host);
        if maptime % STUCK_CHECK_PERIOD == 0 {
            let mut i = 0;
            while i < self.nalive {
                let mut t = self.alive[i];
                if !host.actor_exists(t.actor) || host.actor_is_dead(t.actor) {
                    self.remove_alive_at(i);
                    continue;
                }
                let pos = host.actor_pos(t.actor);
                for p in self.team.iter_alive(host) {
                    let pp = host.actor_pos(p);
                    let (dx, dy) = (pos[0] - pp[0], pos[1] - pp[1]);
                    if host.sqrt(dx * dx + dy * dy) <= 256.0 {
                        mark_proven(&mut self.spots, t.spot_idx);
                        break;
                    }
                }
                let is_boss = self.boss.actor().is_some_and(|b| b == t.actor);
                if !is_boss && update_stuck(host, &mut t, &self.team, maptime) {
                    self.alive[i] = t;
                    self.recycle_monster(host, i, true);
                    continue;
                }
                self.alive[i] = t;
                i += 1;
            }
            if self.nalive == 0 {
                self.maybe_clear_wave(host, None, maptime);
            }
        }

        if self.nalive > 0
            && matches!(self.phase, Phase::Clear | Phase::Assault | Phase::Peak | Phase::Boss)
            && maptime - self.wave_spawned_at >= STRAGGLER_TICS
        {
            let dest = proven_near_team(host, &self.spots, self.nspots, &self.team);
            let mut i = 0;
            while i < self.nalive {
                let t = self.alive[i];
                if !host.actor_exists(t.actor) {
                    self.remove_alive_at(i);
                    continue;
                }
                if let Some(d) = dest {
                    let old = host.actor_pos(t.actor);
                    if !self.telefog.is_null() {
                        let _ = host.spawn(self.telefog, old, 0.0, 0);
                        let _ = host.spawn(self.telefog, d, 0.0, 0);
                    }
                    let _ = host.actor_set_pos(t.actor, d);
                    if let Some(target) = self.team.nearest_alive(host, d) {
                        host.actor_set_ref(t.actor, TARGET_PTR, Some(target));
                    }
                    if let Some(p) = self.team.nearest_alive(host, d) {
                        let pp = host.actor_pos(p);
                        let (dx, dy) = (d[0] - pp[0], d[1] - pp[1]);
                        self.alive[i].best_dist = host.sqrt(dx * dx + dy * dy);
                    }
                    self.alive[i].progress_tic = maptime;
                    i += 1;
                } else {
                    self.recycle_monster(host, i, true);
                }
            }
            self.wave_spawned_at = maptime;
            if self.nalive == 0 {
                self.maybe_clear_wave(host, None, maptime);
            }
        }
    }

    /// Handle a thing death.
    pub fn on_kill(&mut self, host: &mut impl Host, thing: Actor, source: Option<Actor>) {
        let threat = self
            .alive
            .iter()
            .find(|t| t.actor == thing)
            .map(|t| t.threat_cost)
            .unwrap_or(2);
        let elite = self.elites.find(thing).is_some();
        let _ = self.elites.on_death(host, thing);
        let in_wave = self.wave_killed(thing);
        let Some(killer) = source.filter(|&s| host.actor_player(s).is_some()) else {
            if in_wave && self.nalive == 0 {
                self.refresh_team(host);
                self.maybe_clear_wave(host, None, host.maptime());
            }
            return;
        };
        if !host.actor_is_monster(thing) {
            return;
        }
        let now = host.maptime();
        self.streak = if now - self.last_kill <= STREAK_TICS {
            self.streak + 1
        } else {
            1
        };
        self.last_kill = now;
        match self.streak {
            2 => say!(&mut self.announcer, host, "Double kill!"),
            3 => say!(&mut self.announcer, host, "Multi kill!"),
            5 => say!(&mut self.announcer, host, "Rampage!"),
            8 => say!(&mut self.announcer, host, "Unstoppable!"),
            _ => {}
        }
        let seat = host.actor_player(killer).map(|s| s - 1).unwrap_or(0);
        let mut pay_threat = threat.max(2);
        if self.perks.bounty_mul(seat as usize) && elite {
            pay_threat = (pay_threat * 3) / 2;
        }
        let pay = self.economy.on_kill(seat, pay_threat, now, elite);
        self.score = self.score.saturating_add(pay);
        self.perks.on_kill_heal(host, seat as usize, killer);
        self.perks
            .on_kill_ammo(host, seat as usize, killer, self.clip);
        let hp = host.actor_health(killer);
        let heal = self.tuning(host).heal;
        let cap = 100
            + 25
                * i32::from(self.perks.stack(seat as usize, crate::perks::PerkId::Vitality));
        if hp > 0 && hp < cap {
            host.actor_set_health(killer, (hp + heal).min(cap));
        }
        if in_wave && self.nalive == 0 {
            self.refresh_team(host);
            self.maybe_clear_wave(host, Some(killer), now);
        }
    }

    /// Map start.
    pub fn on_loaded(&mut self, host: &mut impl Host) {
        for (slot, name) in self.pool.iter_mut().zip(POOL) {
            *slot = host.class_find(name);
        }
        self.baron = host.class_find("BaronOfHell");
        self.clip = host.class_find("Clip");
        self.shell = host.class_find("Shell");
        self.telefog = host.class_find("TeleportFog");
        self.warden = host.class_find("Warden");
        self.elites.on_loaded(host);
        self.boss = BossState::default();
        self.announcer = Announcer::default();
        self.economy.clear();
        self.perks = PerkState::default();
        self.ui = ModUi::default();
        self.objectives = Objectives::default();
        self.score = 0;
        self.extraction_hold = 0;
        self.wave = 0;
        self.nalive = 0;
        self.streak = 0;
        self.alive_threat = 0;
        self.intensity = 0;
        self.nspots = collect_spots(host, &mut self.spots);
        self.refresh_team(host);
        let mut buf = [0u8; 8];
        let map = host.map_name(&mut buf);
        let mut music_buf = [0u8; 8];
        self.music_len = map_music_name(map, &mut music_buf);
        self.music = music_buf;
        let gap = self.tuning(host).wave_gap;
        self.next_wave_at = gap * TICRATE / 2;
        self.phase = Phase::Warmup;
        self.phase_started_at = 0;
        say!(
            &mut self.announcer,
            host,
            "SURVIVAL - skill {} - first wave in {}s",
            host.skill(),
            gap / 2
        );
    }

    /// Per-tic director.
    pub fn on_tick(&mut self, host: &mut impl Host, maptime: i32) {
        self.announcer.tick(host);
        self.refresh_team(host);
        if !self.team.any_alive(host) {
            return;
        }
        self.elites.on_tick(host, &self.team, maptime);
        self.boss
            .on_tick(host, &mut self.announcer, &self.team, maptime);
        self.objectives.on_tick(host, &self.team);
        self.update_intensity(host);

        match self.phase {
            Phase::Warmup | Phase::Intermission => {
                if self.phase == Phase::Intermission {
                    let drafts_were_open = !self.ui.all_closed();
                    self.ui.tick(
                        host,
                        &mut self.perks,
                        &mut self.announcer,
                        maptime,
                        self.next_wave_at,
                    );
                    // Draft pauses playsim (`rule::PAUSED`); maptime only moves
                    // again once every seat has picked (or timed out).
                    if drafts_were_open && self.ui.all_closed() {
                        // 5 s "next wave" banner after the frozen pick window.
                        self.next_wave_at = maptime + 5 * TICRATE;
                    } else if self.ui.all_closed()
                        && self.next_wave_at - maptime > 5 * TICRATE
                    {
                        self.next_wave_at = maptime + 5 * TICRATE;
                    }
                }
                let left = self.next_wave_at - maptime;
                if left == 5 * TICRATE {
                    say!(&mut self.announcer, host, "Next wave in 5...");
                }
                if left <= 0 {
                    self.ui.close_all();
                    self.begin_round(host, maptime);
                }
            }
            Phase::BuildUp => {
                if maptime % 7 == 0 {
                    self.try_release(host, 3);
                }
                if self.intensity >= BUILDUP_INTENSITY
                    || maptime - self.phase_started_at >= BUILDUP_TICS
                {
                    self.enter_phase(host, Phase::Assault, maptime);
                }
            }
            Phase::Assault | Phase::Boss => {
                // Opening pulse is in `begin_round`. Mid-fight releases only for
                // Horde Rush / Boss so stuck clocks in sealed closets stay honest.
                if maptime % 7 == 0
                    && (self.archetype == Archetype::HordeRush || self.phase == Phase::Boss)
                {
                    self.try_release(host, self.archetype.max_unit_cost());
                }
                if self.intensity >= PEAK_INTENSITY {
                    if self.peak_since < 0 {
                        self.peak_since = maptime;
                    } else if maptime - self.peak_since >= 5 * TICRATE {
                        self.enter_phase(host, Phase::Peak, maptime);
                        self.saw_peak_fade = true;
                    }
                } else {
                    self.peak_since = -1;
                }
                if self.nalive == 0 {
                    // Budget leftovers are abandoned once the map is empty
                    // (e.g. every spawn spot stuck-recycled).
                    self.round_budget_left = 0;
                    self.maybe_clear_wave(host, None, maptime);
                } else if self.round_budget_left <= 0 {
                    self.enter_phase(host, Phase::Clear, maptime);
                }
            }
            Phase::Peak => {
                // Stop releasing; wait for fade.
                if maptime - self.phase_started_at >= PEAK_FADE_TICS
                    || self.intensity < PEAK_INTENSITY / 2
                {
                    if self.round_budget_left > 0 {
                        self.enter_phase(host, Phase::Assault, maptime);
                        self.peak_since = -1;
                    } else {
                        self.enter_phase(host, Phase::Clear, maptime);
                    }
                }
            }
            Phase::Clear => {
                if self.nalive == 0 {
                    self.maybe_clear_wave(host, None, maptime);
                }
            }
            Phase::Extraction => {
                // Hold near team centroid for 5 s, then exit.
                self.extraction_hold = self.extraction_hold.saturating_add(1);
                if self.extraction_hold >= 5 * TICRATE {
                    say_prio!(
                        &mut self.announcer,
                        host,
                        Priority::Critical,
                        "Extracted! Score {}",
                        self.score
                    );
                    self.show_summary(host);
                    host.exit_level(false);
                }
            }
        }

        self.maintain_alive(host, maptime);

        if maptime % TICRATE == 0 {
            for t in &self.alive[..self.nalive] {
                if !host.actor_exists(t.actor) {
                    continue;
                }
                let need = match host.actor_target(t.actor) {
                    None => true,
                    Some(tgt) => host.actor_is_dead(tgt),
                };
                if need {
                    let pos = host.actor_pos(t.actor);
                    if let Some(p) = self.team.nearest_alive(host, pos) {
                        host.actor_set_ref(t.actor, TARGET_PTR, Some(p));
                    }
                }
            }
        }
    }

    /// Damage feedback (low-health + elite affixes).
    pub fn on_damaged(
        &mut self,
        host: &mut impl Host,
        thing: Actor,
        source: Option<Actor>,
        damage: i32,
        health: i32,
    ) {
        let now = host.maptime();
        if host.actor_player(thing).is_some() {
            self.elites
                .on_damaged_player(host, source, damage, now);
            self.intensity = (self.intensity + health.max(0).min(50) * 60).min(10_000);
            if health > 0 && health < 25 && now - self.warned_low_at > 10 * TICRATE {
                self.warned_low_at = now;
                say!(&mut self.announcer, host, "Low health - keep moving!");
            }
            if let Some(seat) = host.actor_player(thing) {
                self.perks
                    .maybe_second_wind(host, (seat - 1) as usize, thing, health);
            }
            self.objectives.on_damaged(health);
        } else {
            self.elites.on_elite_hurt(thing, now);
        }
    }

    /// Death summary.
    pub fn on_player_died(&mut self, host: &mut impl Host) {
        say_prio!(
            &mut self.announcer,
            host,
            Priority::Critical,
            "Survived {} waves",
            self.wave.saturating_sub(1).max(0)
        );
        if !self.team.any_alive(host) {
            self.show_summary(host);
        }
    }

    fn show_summary(&mut self, host: &mut impl Host) {
        use room_script_sdk::hud::{Hud, anchor, font, layer};
        let mut title = crate::text::Msg::new();
        let _ = core::fmt::Write::write_fmt(
            &mut title,
            format_args!("RUN OVER - score {}", self.score),
        );
        let _ = Hud::text(200)
            .seat(-1)
            .anchor(anchor::CENTER)
            .at(0, -20)
            .font(font::BIG)
            .layer(layer::MODAL)
            .color("Gold")
            .text(title.as_str())
            .show();
        let mut body = crate::text::Msg::new();
        let _ = core::fmt::Write::write_fmt(
            &mut body,
            format_args!(
                "Rounds {}  Marks {}",
                self.wave.saturating_sub(1).max(0),
                self.economy.marks(0)
            ),
        );
        let _ = Hud::text(201)
            .seat(-1)
            .anchor(anchor::CENTER)
            .at(0, 10)
            .font(font::SMALL)
            .layer(layer::MODAL)
            .text(body.as_str())
            .show();
        let _ = host;
    }

    /// Test helper: current alive threat.
    #[cfg(test)]
    #[must_use]
    pub fn alive_threat(&self) -> i32 {
        self.alive_threat
    }

    /// Test helper: whether Peak was entered.
    #[cfg(test)]
    #[must_use]
    pub fn saw_peak_fade(&self) -> bool {
        self.saw_peak_fade
    }
}

impl RoomMod for Survival {
    const NAME: &'static str = "SurvivalDirector";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED
        | event_mask::WORLD_THING_DAMAGED
        | event_mask::PLAYER_DIED;

    fn register(&mut self) {
        register::register_stock_classes();
    }

    fn on_event(&mut self, ev: Event) {
        let mut host = crate::host::SdkHost;
        match ev {
            Event::Loaded { .. } => self.on_loaded(&mut host),
            Event::Tick { maptime } => self.on_tick(&mut host, maptime),
            Event::ThingDied { thing, source } => {
                self.on_kill(&mut host, thing, source);
            }
            Event::ThingDamaged {
                thing,
                source,
                damage,
                health,
                ..
            } => {
                self.on_damaged(&mut host, thing, source, damage, health);
            }
            Event::PlayerDied { .. } => self.on_player_died(&mut host),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;
    use crate::tables::alive_cap;

    #[test]
    fn alive_threat_respects_cap_across_skills_and_players() {
        for skill in 1..=5 {
            for players in [1, 2, 4] {
                let mut host = FakeHost::new(skill, 7);
                for s in 0..players {
                    host.add_player(s, [f64::from(s) * 100.0, 0.0, 0.0]);
                }
                for i in 0..32 {
                    host.add_prop([800.0 + f64::from(i) * 70.0, 800.0, 0.0]);
                }
                for name in POOL {
                    let _ = host.ensure_class(name);
                }
                for name in [
                    "BaronOfHell",
                    "Clip",
                    "Shell",
                    "EliteZombie",
                    "EliteImp",
                    "EliteDemon",
                    "DoomImp",
                    "LostSoul",
                    "ExplosiveBarrel",
                    "Medikit",
                    "TeleportFog",
                ] {
                    let _ = host.ensure_class(name);
                }
                let mut d = Survival::default();
                d.on_loaded(&mut host);
                for round in 1..=20 {
                    // Force assault release.
                    d.wave = round - 1;
                    let t0 = host.maptime;
                    d.begin_round(&mut host, t0);
                    let cap = alive_cap(round, skill, players);
                    for _ in 0..200 {
                        host.maptime += 1;
                        d.intensity = 0; // keep releasing
                        let t = host.maptime;
                        d.on_tick(&mut host, t);
                        assert!(
                            d.alive_threat() <= cap,
                            "skill={skill} players={players} round={round} threat={} cap={cap}",
                            d.alive_threat()
                        );
                    }
                    // Wipe alive to advance.
                    while d.nalive > 0 {
                        d.recycle_monster(&mut host, 0, false);
                    }
                    let t = host.maptime;
                    d.maybe_clear_wave(&mut host, None, t);
                }
            }
        }
    }
}

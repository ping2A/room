//! Battle Royale match state and event handlers.

use room_script_sdk::game::{self, actor_f64, player_prop, rule, Actor, Event, RoomMod, event_mask};
use room_script_sdk::ClassRef;

use crate::combat::CombatHud;
use crate::death::{drop_death_loot, DeathExperience};
use crate::drop_select::{self, DropSelect};
use crate::extras::LootExtras;
use crate::loot::LootCtx;
use crate::host::{Host, SdkHost};
use crate::rarity::{Rarity, RarityTable};
use crate::tables::{
    DEPLOY_SECS, DROP_SELECT_SECS, END_SECS, FRAG_HEAL, GRACE_SECS, ITEM_CLASSES, MAX_PLAYERS,
    MAX_SPOTS, PREGAME_COUNTDOWN_SECS, TICRATE,
};
use crate::tell;
use crate::moments::{self, Moments};
use crate::teams::Squads;
use crate::zone::{
    clear_render_zone, compass, dist2, publish_render_zone, zone_damage, ZoneRuntime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchPhase {
    Pregame,
    DropSelect,
    Deploy,
    Loot,
    Zone,
}

pub struct Royale {
    /// Loot / fallback spawn spots (monster spots, then unused player starts).
    pub(crate) spots: [[f64; 3]; MAX_SPOTS],
    pub(crate) nspots: usize,
    /// Deathmatch starts: preferred player spawns.
    pub(crate) dm: [[f64; 3]; 32],
    pub(crate) ndm: usize,
    pub(crate) dm_taken: [bool; 32],
    /// Spot taken by a placed player.
    pub(crate) taken: [bool; MAX_SPOTS],
    pub(crate) items: [ClassRef; ITEM_CLASSES.len()],
    pub(crate) pistol: ClassRef,
    pub(crate) clip: ClassRef,
    pub(crate) fog: ClassRef,
    pub(crate) supply_crate: ClassRef,
    pub(crate) rarity: RarityTable,
    pub(crate) extras: LootExtras,
    pub(crate) placed: [[f64; 2]; MAX_PLAYERS],
    pub(crate) nplaced: usize,
    looted: bool,
    /// Player was in the map when the round started.
    pub(crate) contender: [bool; MAX_PLAYERS],
    drop: DropSelect,
    out: [bool; MAX_PLAYERS],
    zone_hit: [i32; MAX_PLAYERS],
    hinted: [i32; MAX_PLAYERS],
    /// Initial zone centre (also used for loot tiers).
    pub(crate) center: [f64; 2],
    pub(crate) r0: f64,
    phase: MatchPhase,
    pregame_ready: bool,
    countdown_end: i32,
    phase_until: i32,
    loot_start_tic: i32,
    shield_until: i32,
    zone_rt: ZoneRuntime,
    announced_phase: i32,
    announced_shrinking: bool,
    last_countdown_sec: i32,
    /// Decided: `end_at` is when the final screen shows.
    over: bool,
    winner: Option<usize>,
    end_at: i32,
    ended: bool,
    pub(crate) combat: CombatHud,
    death: DeathExperience,
    squads: Squads,
    moments: Moments,
    deploy_tic: i32,
}

impl Default for Royale {
    fn default() -> Self {
        Self {
            spots: [[0.0; 3]; MAX_SPOTS],
            nspots: 0,
            dm: [[0.0; 3]; 32],
            ndm: 0,
            dm_taken: [false; 32],
            taken: [false; MAX_SPOTS],
            items: [ClassRef::NULL; ITEM_CLASSES.len()],
            pistol: ClassRef::NULL,
            clip: ClassRef::NULL,
            fog: ClassRef::NULL,
            supply_crate: ClassRef::NULL,
            rarity: RarityTable::default(),
            extras: LootExtras::default(),
            placed: [[0.0; 2]; MAX_PLAYERS],
            nplaced: 0,
            looted: false,
            contender: [false; MAX_PLAYERS],
            out: [false; MAX_PLAYERS],
            zone_hit: [i32::MIN / 2; MAX_PLAYERS],
            hinted: [i32::MIN / 2; MAX_PLAYERS],
            center: [0.0; 2],
            r0: 0.0,
            phase: MatchPhase::Pregame,
            pregame_ready: false,
            countdown_end: 0,
            phase_until: 0,
            loot_start_tic: 0,
            shield_until: 0,
            zone_rt: ZoneRuntime::new([0.0; 2], 0.0),
            announced_phase: -1,
            announced_shrinking: false,
            last_countdown_sec: -1,
            over: false,
            winner: None,
            end_at: 0,
            ended: false,
            combat: CombatHud::default(),
            death: DeathExperience::default(),
            drop: DropSelect::default(),
            squads: Squads::default(),
            moments: Moments::default(),
            deploy_tic: 0,
        }
    }
}

impl Royale {
    pub(crate) fn contenders(&self) -> usize {
        self.contender.iter().filter(|&&c| c).count()
    }

    fn alive(&self) -> usize {
        (0..MAX_PLAYERS)
            .filter(|&s| self.contender[s] && !self.out[s])
            .count()
    }

    fn apply_landing_shield<H: Host>(&mut self, host: &mut H, until: i32) {
        self.shield_until = until;
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            host.actor_set_f64(body, actor_f64::DAMAGE_FACTOR, 0.0);
        }
    }

    fn clear_landing_shields<H: Host>(&mut self, host: &mut H) {
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            if host.actor_get_f64(body, actor_f64::DAMAGE_FACTOR) <= 0.0 {
                host.actor_set_f64(body, actor_f64::DAMAGE_FACTOR, 1.0);
            }
        }
    }

    fn deploy_from_drops<H: Host>(&mut self, host: &mut H) {
        self.drop.end_capture();
        self.nplaced = 0;
        self.dm_taken = [false; 32];
        self.taken = [false; MAX_SPOTS];
        let picks = self.drop.pick;
        let contender = self.contender;
        for s in 0..MAX_PLAYERS {
            if !contender[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            let mut others = [[0.0; 2]; MAX_PLAYERS];
            let mut n = 0usize;
            for j in 0..MAX_PLAYERS {
                if j == s || !contender[j] {
                    continue;
                }
                others[n] = picks[j];
                n += 1;
            }
            let spot = drop_select::resolve_drop(
                picks[s],
                &self.spots,
                self.nspots,
                &others[..n],
                &self.taken,
            );
            drop_select::mark_spot_taken(&self.spots, self.nspots, &mut self.taken, spot);
            self.teleport_player(host, body, spot, self.center);
            if self.nplaced < MAX_PLAYERS {
                self.placed[self.nplaced] = crate::zone::xy(spot);
                self.nplaced += 1;
            }
        }
    }

    fn begin_deploy<H: Host>(&mut self, host: &mut H, t: i32) {
        self.deploy_from_drops(host);
        self.phase = MatchPhase::Deploy;
        self.phase_until = t + DEPLOY_SECS * TICRATE;
        self.apply_landing_shield(host, self.phase_until);
        tell!(host, None, "DEPLOY");
        tell!(host, None, "GO");
    }

    fn begin_loot<H: Host>(&mut self, host: &mut H, t: i32) {
        self.phase = MatchPhase::Loot;
        self.loot_start_tic = t;
        self.deploy_tic = t;
        game::set_music("D_E1M1");
        host.set_rule(rule::DEATHMATCH, 1);
        self.clear_landing_shields(host);
        if !self.looted {
            self.looted = true;
            self.drop_loot(host);
        }
        tell!(host, None, "The zone closes in {}s", GRACE_SECS);
    }

    fn tick_match_phases<H: Host>(&mut self, host: &mut H, t: i32) {
        match self.phase {
            MatchPhase::Pregame => {
                if !self.pregame_ready {
                    self.pregame_ready = true;
                    self.countdown_end = t + PREGAME_COUNTDOWN_SECS * TICRATE;
                    tell!(host, None, "PREGAME");
                }
                if t % TICRATE == 0 {
                    let left = ((self.countdown_end - t) + TICRATE - 1) / TICRATE;
                    if left != self.last_countdown_sec && left >= 0 {
                        self.last_countdown_sec = left;
                        if left > 0 {
                            tell!(host, None, "Match starts in {}s", left);
                        }
                    }
                }
                if t >= self.countdown_end {
                    self.phase = MatchPhase::DropSelect;
                    self.phase_until = t + DROP_SELECT_SECS * TICRATE;
                    self.drop.begin(
                        host,
                        &self.contender,
                        self.center,
                        self.r0,
                        &self.spots,
                        self.nspots,
                    );
                    tell!(host, None, "DROP SELECT");
                }
            }
            MatchPhase::DropSelect => {
                let ends = self.phase_until;
                if self.drop.tick(
                    host,
                    &self.contender,
                    self.center,
                    self.r0,
                    &self.spots,
                    self.nspots,
                    t,
                    ends,
                ) {
                    self.begin_deploy(host, t);
                }
            }
            MatchPhase::Deploy => {
                if t >= self.phase_until {
                    self.begin_loot(host, t);
                }
            }
            MatchPhase::Loot => {
                let grace_end = self.loot_start_tic + GRACE_SECS * TICRATE;
                if t >= grace_end {
                    self.phase = MatchPhase::Zone;
                }
            }
            MatchPhase::Zone => {}
        }
        if t >= self.shield_until && self.shield_until > 0 {
            self.clear_landing_shields(host);
            self.shield_until = 0;
        }
    }

    pub(crate) fn on_loaded<H: Host>(&mut self, host: &mut H) {
        host.set_rule(rule::DEATHMATCH, 0);
        host.set_rule(rule::RESPAWN, 0);
        for (slot, name) in self.items.iter_mut().zip(ITEM_CLASSES) {
            *slot = host.class_find(name);
        }
        self.pistol = host.class_find("Pistol");
        self.clip = host.class_find("Clip");
        self.fog = host.class_find("TeleportFog");
        self.supply_crate = host.class_find("SupplyCrate");
        self.rarity.resolve(host);
        self.clear_monsters(host);
        self.read_starts(host);
        self.pick_zone(host);
        self.zone_rt = ZoneRuntime::new(self.center, self.r0);
        self.extras.reset();
        self.extras
            .place_vault(host, self.center, &self.spots, self.nspots);
        self.extras.on_loaded_markers();
        self.death.reset();
        self.moments.reset();
        self.deploy_tic = 0;
        for s in 0..MAX_PLAYERS {
            self.contender[s] = host.player(s as i32).is_some();
        }
        self.squads.on_loaded(host, &self.contender);
        let n = self.contenders();
        if n >= 2 {
            tell!(
                host,
                None,
                "BATTLE ROYALE - {} players. No guns: loot the map!",
                n
            );
            tell!(host, None, "Last one standing wins.");
        } else {
            tell!(host, None, "Battle Royale practice: 2+ players to win");
        }
    }

    /// Fists only, then teleport to the free deathmatch start (else monster
    /// spot) farthest from the other players.
    pub(crate) fn on_entered<H: Host>(&mut self, host: &mut H, body: Actor) {
        if let Some(seat) = host.actor_player(body) {
            let s = seat as usize;
            if s < MAX_PLAYERS {
                self.contender[s] = true;
            }
        }
        let _ = host.actor_take(body, self.pistol, 1);
        let _ = host.actor_take(body, self.clip, 999);
        let spot = if let Some(i) = self.farthest(host, &self.dm[..self.ndm], &self.dm_taken) {
            self.dm_taken[i] = true;
            self.dm[i]
        } else if let Some(i) = self.farthest(host, &self.spots[..self.nspots], &self.taken) {
            self.taken[i] = true;
            self.spots[i]
        } else {
            return;
        };
        self.teleport_player(host, body, spot, self.center);
    }

    pub(crate) fn teleport_player<H: Host>(
        &self,
        host: &mut H,
        body: Actor,
        spot: [f64; 3],
        look_at: [f64; 2],
    ) {
        let face = host
            .atan2(look_at[1] - spot[1], look_at[0] - spot[0])
            .to_degrees();
        host.actor_set_pos(body, spot);
        host.actor_set_vel(body, [0.0; 3]);
        host.actor_set_angle(body, face);
        if let Some(fog) = host.spawn(self.fog, spot, face, 0) {
            host.actor_sound(fog, "misc/teleport", 1.0);
        }
    }

    /// Announce and schedule the final screen (`winner` = last standing).
    fn decide<H: Host>(&mut self, host: &mut H, winner: Option<usize>, t: i32) {
        if self.over {
            return;
        }
        self.over = true;
        self.winner = winner;
        self.end_at = t + END_SECS * TICRATE;
        for s in 0..MAX_PLAYERS {
            if host.player(s as i32).is_some() {
                self.contender[s] = true;
            }
        }
        let (rows, n) = moments::build_standings(
            host,
            &self.contender,
            &self.death,
            &self.combat,
            &self.squads,
            self.deploy_tic,
            t,
            winner,
        );
        Moments::finish_match_ex(winner, &rows[..n]);
        self.moments.mark_standings_sent();
        self.moments.begin_resolution(host, winner, t);
        match winner {
            Some(w) => {
                let mut m = crate::text::Msg::new();
                crate::text::write_player_name(&mut m, w);
                let _ = core::fmt::Write::write_str(&mut m, " WINS THE BATTLE ROYALE!");
                host.print(None, m.as_str());
                tell!(host, Some(w as i32), "Winner! Last one standing.");
            }
            None if self.contenders() < 2 => tell!(host, None, "Practice over"),
            None => tell!(host, None, "No survivors - it's a draw!"),
        }
    }

    /// Time is up: the survivor with the most frags (then health) wins.
    fn time_up<H: Host>(&mut self, host: &mut H, t: i32) {
        let mut best: Option<(usize, i32, i32)> = None;
        let mut tie = false;
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] || self.out[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            let key = (
                host.player_stat(s as i32, player_prop::FRAGS),
                host.actor_health(body),
            );
            match best {
                Some((_, f, h)) if (key.0, key.1) == (f, h) => tie = true,
                Some((_, f, h)) if (key.0, key.1) < (f, h) => {}
                _ => {
                    best = Some((s, key.0, key.1));
                    tie = false;
                }
            }
        }
        tell!(host, None, "TIME!");
        let winner = best.filter(|_| !tie && self.contenders() >= 2).map(|b| b.0);
        self.decide(host, winner, t);
    }

    fn zone_active(&self) -> bool {
        matches!(self.phase, MatchPhase::Loot | MatchPhase::Zone)
    }

    fn zone_t_rel(&self, t: i32) -> Option<i32> {
        if !self.zone_active() || self.loot_start_tic == 0 {
            return None;
        }
        Some(t - self.loot_start_tic)
    }

    pub(crate) fn on_tick<H: Host>(&mut self, host: &mut H, t: i32) {
        if !self.over {
            self.tick_match_phases(host, t);
        }
        self.death.tick(
            host,
            &self.out,
            &self.contender,
            &self.combat,
            t,
        );
        self.squads
            .tick(host, &self.contender, &self.out, t);
        if self.phase == MatchPhase::DropSelect {
            self.drop
                .draw(host, &self.contender, t, self.phase_until);
        }
        if t % 5 == 0 {
            self.draw_hud(host, t);
        }
        if self.zone_active() {
            if let Some(t_rel) = self.zone_t_rel(t) {
                if t % TICRATE == 0 {
                    self.zone_rt.sync(
                        host,
                        self.r0,
                        self.center,
                        &self.spots,
                        self.nspots,
                        t_rel,
                    );
                }
                let ctx = LootCtx::from_royale(self);
                self.extras.tick(
                    host,
                    &ctx,
                    t,
                    t_rel,
                    &self.contender,
                    &self.zone_rt,
                    &self.spots,
                    self.nspots,
                    self.supply_crate,
                );
            }
        }
        self.moments.tick_announcer(host, t);
        if self.over {
            self.extras.on_match_over();
            clear_render_zone(host);
            if !self.ended {
                self.moments.tick_resolution(self.winner, t);
                if self.moments.resolution_done() {
                    self.ended = true;
                }
            }
            return;
        }
        let left = host.get_rule(rule::TIME_LEFT);
        if left == 1 {
            self.time_up(host, t);
            return;
        }
        if t % TICRATE != 0 {
            return;
        }
        if left == 60 * TICRATE + 1 || left == 10 * TICRATE + 1 {
            tell!(host, None, "{} seconds left!", left / TICRATE);
        }
        if self.phase == MatchPhase::Loot {
            let grace_left = self.loot_start_tic + GRACE_SECS * TICRATE - t;
            if grace_left == 10 * TICRATE {
                tell!(host, None, "The zone closes in 10s");
            }
        }
        let Some(t_rel) = self.zone_t_rel(t) else {
            return;
        };
        self.zone_rt.sync(
            host,
            self.r0,
            self.center,
            &self.spots,
            self.nspots,
            t_rel,
        );
        let geom = self.zone_rt.geom;
        publish_render_zone(host, &geom, self.zone_rt.next_circle());
        if geom.phase != self.announced_phase || geom.shrinking != self.announced_shrinking {
            self.announced_phase = geom.phase;
            self.announced_shrinking = geom.shrinking;
            publish_render_zone(host, &geom, self.zone_rt.next_circle());
            self.moments
                .on_zone_phase_banner(host, geom.phase, geom.shrinking, t);
            if geom.shrinking && geom.phase > 0 {
                tell!(host, None, "Zone phase {} - it is closing in!", geom.phase);
            } else if geom.phase > 0 {
                tell!(
                    host,
                    None,
                    "Zone phase {}: {} dmg/s outside",
                    geom.phase,
                    zone_damage(geom.phase)
                );
            }
        }
        if geom.phase == 0 {
            return;
        }
        for s in 0..MAX_PLAYERS {
            if !self.contender[s] || self.out[s] {
                continue;
            }
            let Some(body) = host.player(s as i32) else {
                continue;
            };
            let p = host.actor_pos(body);
            let d2 = dist2([p[0], p[1]], geom.center);
            if d2 <= geom.radius * geom.radius || host.actor_is_dead(body) {
                continue;
            }
            self.zone_hit[s] = t;
            let _ = host.actor_damage(body, zone_damage(geom.phase), None, None);
            if t - self.hinted[s] >= 4 * TICRATE {
                self.hinted[s] = t;
                let gap = ((host.sqrt(d2) - geom.radius) as i32).max(1);
                tell!(
                    host,
                    Some(s as i32),
                    "Outside the zone! Run {} ({} units)",
                    compass(host, [p[0], p[1]], geom.center),
                    gap
                );
            }
        }
    }

    pub(crate) fn on_player_died<H: Host>(
        &mut self,
        host: &mut H,
        victim: usize,
        source: Option<Actor>,
        t: i32,
    ) {
        if victim >= MAX_PLAYERS || !self.contender[victim] || self.out[victim] {
            return;
        }
        let victim_pos = host
            .player(victim as i32)
            .map(|b| host.actor_pos(b))
            .unwrap_or([0.0; 3]);
        if let Some(body) = host.player(victim as i32) {
            let pos = host.actor_pos(body);
            drop_death_loot(host, self, body, self.pistol, self.clip);
            if let Some(k) = source
                .and_then(|s| host.actor_player(s))
                .map(|k| k as usize)
                && k != victim
            {
                self.extras.mark_death_loot(
                    k,
                    pos,
                    t + crate::extras::DEATH_MARKER_SECS * TICRATE,
                );
            }
        }
        self.out[victim] = true;
        let left = self.alive();
        let killer_seat = source.and_then(|s| host.actor_player(s).map(|k| k as usize));
        self.combat
            .on_victim_died(victim, killer_seat, t);
        let killer = source.and_then(|s| host.actor_player(s).map(|k| (s, k as usize)));
        let place = left + 1;
        self.death.on_eliminated(
            host,
            victim,
            place,
            killer.map(|(b, k)| (b, k)),
            victim_pos,
            t,
        );
        match killer {
            Some((body, k)) if k != victim => {
                self.combat.on_player_frag(k, victim, t);
                let mut m = crate::text::Msg::new();
                crate::text::write_player_name(&mut m, k);
                let _ = core::fmt::Write::write_str(&mut m, " fragged ");
                crate::text::write_player_name(&mut m, victim);
                let _ = core::fmt::Write::write_fmt(&mut m, format_args!(" - {left} left"));
                host.print(None, m.as_str());
                let hp = host.actor_health(body);
                if hp > 0 {
                    host.actor_set_health(body, (hp + FRAG_HEAL).min(100.max(hp)));
                }
            }
            Some(_) => {
                let mut m = crate::text::Msg::new();
                crate::text::write_player_name(&mut m, victim);
                let _ = core::fmt::Write::write_fmt(&mut m, format_args!(" blew themselves up - {left} left"));
                host.print(None, m.as_str());
            }
            None if t - self.zone_hit[victim] <= 1 => {
                let mut m = crate::text::Msg::new();
                crate::text::write_player_name(&mut m, victim);
                let _ = core::fmt::Write::write_fmt(
                    &mut m,
                    format_args!(" was swallowed by the zone - {left} left"),
                );
                host.print(None, m.as_str());
            }
            None => {
                let mut m = crate::text::Msg::new();
                crate::text::write_player_name(&mut m, victim);
                let _ = core::fmt::Write::write_fmt(&mut m, format_args!(" is out - {left} left"));
                host.print(None, m.as_str());
            }
        }
        let mut alive_seats = [0usize; MAX_PLAYERS];
        let mut na = 0usize;
        for s in 0..MAX_PLAYERS {
            if self.contender[s] && !self.out[s] {
                alive_seats[na] = s;
                na += 1;
            }
        }
        self.moments.on_alive_count(
            host,
            left,
            &alive_seats[..na],
            self.contenders(),
            t,
        );
        if self.over {
            return;
        }
        tell!(
            host,
            Some(victim as i32),
            "Eliminated - you placed #{}.",
            left + 1
        );
        match left {
            0 => self.decide(host, None, t),
            1 if self.contenders() >= 2 => {
                let w = (0..MAX_PLAYERS).find(|&s| self.contender[s] && !self.out[s]);
                self.decide(host, w, t);
            }
            _ => {}
        }
    }

    fn draw_hud<H: Host>(&mut self, host: &mut H, t: i32) {
        if self.over {
            self.combat.clear_widgets();
            crate::drop_select::clear_drop_widgets();
            return;
        }
        if self.phase == MatchPhase::DropSelect {
            return;
        }
        let mut frame = crate::ui::HudFrame {
            t,
            match_over: false,
            show_zone: self.loot_start_tic > 0,
            t_rel: self.zone_t_rel(t),
            alive_n: self.alive(),
            contender: &self.contender,
            combat: &self.combat,
            zone_rt: &mut self.zone_rt,
            r0: self.r0,
            center: self.center,
            spots: &self.spots,
            nspots: self.nspots,
        };
        frame.draw(host);
    }

    pub(crate) fn on_item_pickup<H: Host>(&mut self, host: &mut H, toucher: Actor, item: ClassRef) {
        let Some(seat) = host.actor_player(toucher) else {
            return;
        };
        let label = self.rarity_label(item);
        if !label.is_empty() {
            tell!(host, Some(seat), "{label} pickup");
        }
    }

    fn rarity_label(&self, item: ClassRef) -> &'static str {
        for (base_i, tiers) in self.rarity.classes.iter().enumerate() {
            for (r_i, &cls) in tiers.iter().enumerate() {
                if cls == item && r_i > 0 {
                    return Rarity::LABELS[r_i];
                }
            }
            let _ = base_i;
        }
        if self
            .rarity
            .classes
            .iter()
            .any(|tiers| tiers[0] == item)
        {
            return Rarity::Common.label();
        }
        ""
    }

    pub(crate) fn on_thing_damaged<H: Host>(
        &mut self,
        host: &mut H,
        thing: Actor,
        source: Option<Actor>,
        damage: i32,
        health: i32,
        t: i32,
    ) {
        if self.over {
            return;
        }
        self.combat
            .on_thing_damaged(host, thing, source, damage, health, t);
        self.squads.on_damaged(host, thing, source, health, t);
    }
}

impl RoomMod for Royale {
    const NAME: &'static str = "BattleRoyale";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED
        | event_mask::WORLD_THING_DAMAGED
        | event_mask::WORLD_ITEM_PICKUP
        | event_mask::PLAYER_ENTERED;

    fn register(&mut self) {
        crate::register::register_loot_classes();
    }

    fn on_event(&mut self, event: Event) {
        let mut host = SdkHost;
        match event {
            Event::Loaded { .. } => {
                *self = Self::default();
                self.on_loaded(&mut host);
            }
            Event::PlayerEntered { actor, .. } => self.on_entered(&mut host, actor),
            Event::Tick { maptime } => self.on_tick(&mut host, maptime),
            Event::ThingDied { thing, source } => {
                if let Some(victim) = thing.player().and_then(|p| usize::try_from(p).ok()) {
                    let t = host.maptime();
                    self.on_player_died(&mut host, victim, source, t);
                }
            }
            Event::ThingDamaged {
                thing,
                source,
                damage,
                health,
                ..
            } => {
                let t = host.maptime();
                self.on_thing_damaged(&mut host, thing, source, damage, health, t);
            }
            Event::ItemPickup { toucher, item } => {
                self.on_item_pickup(&mut host, toucher, item);
            }
            _ => {}
        }
    }
}

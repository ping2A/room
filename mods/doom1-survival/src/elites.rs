//! Elite affixes (`docs/27` §6.4).

use crate::announcer::{Announcer, Priority};
use crate::host::{default_spawn_flags, Host, MASTER_PTR, TARGET_PTR};
use crate::tables::TICRATE;
use crate::team::Team;
use crate::text::Msg;
use core::fmt::Write;
use room_script_sdk::game::{actor_f64, Actor};
use room_script_sdk::ClassRef;

/// Max simultaneous tracked elites.
pub const MAX_ELITES: usize = 8;
/// Threat cost multiplier for elites (+50 %).
pub const ELITE_THREAT_NUM: i32 = 3;
pub const ELITE_THREAT_DEN: i32 = 2;

/// Affix ids (`docs/27` §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Affix {
    Brute = 0,
    Haste = 1,
    Regenerating = 2,
    Vampiric = 3,
    Volatile = 4,
    Splitter = 5,
    Summoner = 6,
    Phasing = 7,
}

impl Affix {
    const ALL: [Affix; 8] = [
        Self::Brute,
        Self::Haste,
        Self::Regenerating,
        Self::Vampiric,
        Self::Volatile,
        Self::Splitter,
        Self::Summoner,
        Self::Phasing,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Brute => "Brute",
            Self::Haste => "Haste",
            Self::Regenerating => "Regenerating",
            Self::Vampiric => "Vampiric",
            Self::Volatile => "Volatile",
            Self::Splitter => "Splitter",
            Self::Summoner => "Summoner",
            Self::Phasing => "Phasing",
        }
    }

    #[must_use]
    pub fn pick(host: &mut impl Host) -> Self {
        Self::ALL[host.random(0, 7) as usize]
    }
}

/// One live elite.
#[derive(Clone, Copy)]
pub struct Elite {
    pub actor: Actor,
    pub affix: Affix,
    /// Base class for Splitter copies (non-elite).
    pub base_class: ClassRef,
    pub spawn_health: i32,
    pub last_damage_tic: i32,
    pub next_special_tic: i32,
    pub summons: u8,
    pub phasing: bool,
    pub threat_cost: i32,
}

impl Default for Elite {
    fn default() -> Self {
        Self {
            actor: Actor::NULL,
            affix: Affix::Brute,
            base_class: ClassRef::NULL,
            spawn_health: 0,
            last_damage_tic: 0,
            next_special_tic: 0,
            summons: 0,
            phasing: false,
            threat_cost: 0,
        }
    }
}

/// Tracked elite set + class refs.
#[derive(Clone, Copy)]
pub struct EliteState {
    elites: [Elite; MAX_ELITES],
    n: usize,
    pub elite_zombie: ClassRef,
    pub elite_imp: ClassRef,
    pub elite_demon: ClassRef,
    pub lost_soul: ClassRef,
    pub barrel: ClassRef,
    pub medikit: ClassRef,
    pub clip: ClassRef,
}

impl Default for EliteState {
    fn default() -> Self {
        Self {
            elites: [Elite::default(); MAX_ELITES],
            n: 0,
            elite_zombie: ClassRef::NULL,
            elite_imp: ClassRef::NULL,
            elite_demon: ClassRef::NULL,
            lost_soul: ClassRef::NULL,
            barrel: ClassRef::NULL,
            medikit: ClassRef::NULL,
            clip: ClassRef::NULL,
        }
    }
}

impl EliteState {
    /// Resolve elite / support classes after stock registration.
    pub fn on_loaded(&mut self, host: &impl Host) {
        self.elite_zombie = host.class_find("EliteZombie");
        self.elite_imp = host.class_find("EliteImp");
        self.elite_demon = host.class_find("EliteDemon");
        self.lost_soul = host.class_find("LostSoul");
        self.barrel = host.class_find("ExplosiveBarrel");
        self.medikit = host.class_find("Medikit");
        self.clip = host.class_find("Clip");
        self.n = 0;
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.n
    }

    /// Map a pool class onto an elite subclass when available.
    #[must_use]
    pub fn elite_class_for(&self, host: &impl Host, base: ClassRef) -> Option<(ClassRef, ClassRef)> {
        if base.is_null() {
            return None;
        }
        let zombie = host.class_find("ZombieMan");
        let shotgun = host.class_find("ShotgunGuy");
        let imp = host.class_find("DoomImp");
        let phantom = host.class_find("PhantomImp");
        let demon = host.class_find("Demon");
        let spectre = host.class_find("Spectre");
        let jug = host.class_find("Juggernaut");

        let is_zombie_family = (!zombie.is_null() && (base == zombie || host.class_is(base, zombie)))
            || (!shotgun.is_null() && base == shotgun);
        if is_zombie_family && !self.elite_zombie.is_null() {
            return Some((self.elite_zombie, if zombie.is_null() { base } else { zombie }));
        }

        let is_imp_family = (!imp.is_null() && (base == imp || host.class_is(base, imp)))
            || (!phantom.is_null() && base == phantom);
        if is_imp_family && !self.elite_imp.is_null() {
            let bc = if !phantom.is_null() && base == phantom {
                phantom
            } else if imp.is_null() {
                base
            } else {
                imp
            };
            return Some((self.elite_imp, bc));
        }

        let is_demon_family = (!demon.is_null() && (base == demon || host.class_is(base, demon)))
            || (!spectre.is_null() && base == spectre)
            || (!jug.is_null() && base == jug);
        if is_demon_family && !self.elite_demon.is_null() {
            return Some((self.elite_demon, if demon.is_null() { base } else { demon }));
        }

        if !self.elite_imp.is_null() {
            Some((self.elite_imp, if imp.is_null() { base } else { imp }))
        } else if !self.elite_demon.is_null() {
            Some((
                self.elite_demon,
                if demon.is_null() { base } else { demon },
            ))
        } else if !self.elite_zombie.is_null() {
            Some((
                self.elite_zombie,
                if zombie.is_null() { base } else { zombie },
            ))
        } else {
            None
        }
    }

    /// Display stem for announcements ("Imp", "Demon", …).
    #[must_use]
    pub fn label_for_elite_class(&self, class: ClassRef) -> &'static str {
        if class == self.elite_zombie {
            "Zombie"
        } else if class == self.elite_imp {
            "Imp"
        } else if class == self.elite_demon {
            "Demon"
        } else {
            "Elite"
        }
    }

    #[must_use]
    pub fn find(&self, thing: Actor) -> Option<usize> {
        self.elites[..self.n]
            .iter()
            .position(|e| e.actor == thing)
    }

    /// Apply affix stats right after spawn. Returns threat cost used.
    pub fn promote(
        &mut self,
        host: &mut impl Host,
        ann: &mut Announcer,
        actor: Actor,
        elite_class: ClassRef,
        base_class: ClassRef,
        affix: Affix,
        base_threat: i32,
        now: i32,
    ) -> i32 {
        if self.n == MAX_ELITES || actor.is_null() {
            return 0;
        }
        let cost = (base_threat * ELITE_THREAT_NUM) / ELITE_THREAT_DEN;
        apply_affix_spawn(host, actor, affix);
        let hp = host.actor_health(actor).max(1);
        self.elites[self.n] = Elite {
            actor,
            affix,
            base_class,
            spawn_health: hp,
            last_damage_tic: now,
            next_special_tic: now + special_period(affix),
            summons: 0,
            phasing: false,
            threat_cost: cost,
        };
        self.n += 1;

        let mut line = Msg::new();
        let _ = write!(
            line,
            "{} {}",
            affix.name(),
            self.label_for_elite_class(elite_class)
        );
        ann.push(host, None, Priority::High, line.as_str());
        Announcer::cue(host, Some(actor), "misc/teleport");
        cost
    }

    /// Per-tic affix logic (≤ 8 elites).
    pub fn on_tick(&mut self, host: &mut impl Host, team: &Team, now: i32) {
        let mut i = 0;
        while i < self.n {
            let e = self.elites[i];
            if !host.actor_exists(e.actor) || host.actor_is_dead(e.actor) {
                self.remove_at(i);
                continue;
            }
            match e.affix {
                Affix::Regenerating => {
                    if now - e.last_damage_tic >= 3 * TICRATE && now % 35 == 0 {
                        let hp = host.actor_health(e.actor);
                        let add = (e.spawn_health * 2 / 100).max(1);
                        host.actor_set_health(e.actor, (hp + add).min(e.spawn_health));
                    }
                }
                Affix::Summoner => {
                    if now >= e.next_special_tic && e.summons < 3 {
                        let mut can = false;
                        for p in team.iter_alive(host) {
                            if host.actor_can_see(e.actor, p) {
                                can = true;
                                break;
                            }
                        }
                        if can && !self.lost_soul.is_null() {
                            let pos = host.actor_pos(e.actor);
                            let spawn_pos = [pos[0] + 24.0, pos[1], pos[2]];
                            if let Some(s) =
                                host.spawn(self.lost_soul, spawn_pos, 0.0, default_spawn_flags())
                            {
                                host.actor_set_ref(s, MASTER_PTR, Some(e.actor));
                                if let Some(t) = team.nearest_alive(host, spawn_pos) {
                                    host.actor_set_ref(s, TARGET_PTR, Some(t));
                                }
                                host.actor_set_state(s, "See");
                                self.elites[i].summons = e.summons.saturating_add(1);
                            }
                        }
                        self.elites[i].next_special_tic = now + 8 * TICRATE;
                    }
                }
                Affix::Phasing => {
                    let period = 6 * TICRATE;
                    let phase_len = 2 * TICRATE;
                    let cycle_start = e.next_special_tic;
                    let age = now - cycle_start;
                    let should = age >= 0 && (age % period) < phase_len;
                    if should != e.phasing {
                        self.elites[i].phasing = should;
                        if should {
                            host.actor_set_f64(e.actor, actor_f64::ALPHA, 0.3);
                            host.actor_set_f64(e.actor, actor_f64::DAMAGE_FACTOR, 0.25);
                            Announcer::cue(host, Some(e.actor), "misc/teleport");
                        } else {
                            host.actor_set_f64(e.actor, actor_f64::ALPHA, 1.0);
                            host.actor_set_f64(e.actor, actor_f64::DAMAGE_FACTOR, 1.0);
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Vampiric heal when this elite damages a player.
    pub fn on_damaged_player(
        &mut self,
        host: &mut impl Host,
        source: Option<Actor>,
        damage: i32,
        now: i32,
    ) {
        let Some(src) = source else {
            return;
        };
        let Some(i) = self.find(src) else {
            return;
        };
        self.elites[i].last_damage_tic = now;
        if self.elites[i].affix == Affix::Vampiric && damage > 0 {
            let hp = host.actor_health(src);
            host.actor_set_health(src, hp + damage * 2);
        }
    }

    /// Record damage taken by an elite (breaks regenerating quiet window).
    pub fn on_elite_hurt(&mut self, thing: Actor, now: i32) {
        if let Some(i) = self.find(thing) {
            self.elites[i].last_damage_tic = now;
        }
    }

    /// Death hooks: Volatile / Splitter + loot.
    pub fn on_death(&mut self, host: &mut impl Host, thing: Actor) -> Option<i32> {
        let Some(i) = self.find(thing) else {
            return None;
        };
        let e = self.elites[i];
        let pos = host.actor_pos(thing);
        match e.affix {
            Affix::Volatile => {
                if !self.barrel.is_null() {
                    if let Some(b) = host.spawn(self.barrel, pos, 0.0, 0) {
                        let _ = host.actor_damage(b, None, None, 100);
                    }
                }
            }
            Affix::Splitter => {
                if !e.base_class.is_null() {
                    for dx in [-20.0_f64, 20.0] {
                        let p = [pos[0] + dx, pos[1], pos[2]];
                        if let Some(c) = host.spawn(e.base_class, p, 0.0, default_spawn_flags()) {
                            let hp = (e.spawn_health / 2).max(1);
                            host.actor_set_health(c, hp);
                            host.actor_set_f64(c, actor_f64::SCALE_X, 0.8);
                            host.actor_set_f64(c, actor_f64::SCALE_Y, 0.8);
                            host.actor_set_state(c, "See");
                        }
                    }
                }
            }
            _ => {}
        }
        if !self.clip.is_null() {
            let _ = host.spawn(self.clip, pos, 0.0, 0);
        } else if !self.medikit.is_null() {
            let _ = host.spawn(self.medikit, pos, 0.0, 0);
        }
        let cost = e.threat_cost;
        self.remove_at(i);
        Some(cost)
    }

    fn remove_at(&mut self, i: usize) {
        if i >= self.n {
            return;
        }
        self.n -= 1;
        self.elites[i] = self.elites[self.n];
        self.elites[self.n] = Elite::default();
    }
}

fn special_period(affix: Affix) -> i32 {
    match affix {
        Affix::Summoner => 8 * TICRATE,
        Affix::Phasing => 0,
        _ => 0,
    }
}

fn apply_affix_spawn(host: &mut impl Host, actor: Actor, affix: Affix) {
    match affix {
        Affix::Brute => {
            let hp = host.actor_health(actor);
            host.actor_set_health(actor, (hp * 5) / 2);
            let sx = host.actor_get_f64(actor, actor_f64::SCALE_X);
            let sy = host.actor_get_f64(actor, actor_f64::SCALE_Y);
            let sx = if sx <= 0.0 { 1.0 } else { sx };
            let sy = if sy <= 0.0 { 1.0 } else { sy };
            host.actor_set_f64(actor, actor_f64::SCALE_X, sx * 1.25);
            host.actor_set_f64(actor, actor_f64::SCALE_Y, sy * 1.25);
            host.actor_set_f64(actor, actor_f64::DAMAGE_FACTOR, 0.8);
        }
        Affix::Haste => {
            let sp = host.actor_get_f64(actor, actor_f64::SPEED);
            let sp = if sp <= 0.0 { 8.0 } else { sp };
            host.actor_set_f64(actor, actor_f64::SPEED, sp * 1.5);
        }
        Affix::Regenerating
        | Affix::Vampiric
        | Affix::Volatile
        | Affix::Splitter
        | Affix::Summoner
        | Affix::Phasing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;
    use crate::team::Team;

    fn primed_host() -> FakeHost {
        let mut host = FakeHost::new(4, 9);
        host.add_player(0, [0.0, 0.0, 0.0]);
        for name in [
            "ZombieMan",
            "DoomImp",
            "Demon",
            "EliteZombie",
            "EliteImp",
            "EliteDemon",
            "LostSoul",
            "ExplosiveBarrel",
            "Clip",
            "Medikit",
            "PhantomImp",
            "ShotgunGuy",
            "Spectre",
            "Juggernaut",
        ] {
            let _ = host.ensure_class(name);
        }
        host.set_class_parent("EliteZombie", "ZombieMan");
        host.set_class_parent("EliteImp", "DoomImp");
        host.set_class_parent("EliteDemon", "Demon");
        host.set_class_parent("PhantomImp", "DoomImp");
        host
    }

    fn spawn_with(host: &mut FakeHost, st: &mut EliteState, affix: Affix) -> Actor {
        let mut ann = Announcer::default();
        st.on_loaded(host);
        let base = host.class_find("DoomImp");
        let (ec, bc) = st.elite_class_for(host, base).expect("elite class");
        let a = host.spawn(ec, [400.0, 0.0, 0.0], 0.0, 0).expect("spawn");
        host.actor_set_health(a, 60);
        let _ = st.promote(host, &mut ann, a, ec, bc, affix, 4, 0);
        a
    }

    #[test]
    fn each_affix_applies_without_panic() {
        for affix in Affix::ALL {
            let mut host = primed_host();
            let mut st = EliteState::default();
            let a = spawn_with(&mut host, &mut st, affix);
            assert!(st.find(a).is_some(), "{affix:?}");
            match affix {
                Affix::Brute => {
                    assert!(host.actor_health(a) >= 150, "brute health");
                    assert!((host.actor_get_f64(a, actor_f64::SCALE_X) - 1.25).abs() < 0.01);
                    assert!((host.actor_get_f64(a, actor_f64::DAMAGE_FACTOR) - 0.8).abs() < 0.01);
                }
                Affix::Haste => {
                    assert!((host.actor_get_f64(a, actor_f64::SPEED) - 12.0).abs() < 0.01);
                }
                _ => {}
            }
            let mut team = Team::default();
            team.refresh(&host);
            for t in 0..8 * TICRATE {
                host.maptime = t;
                st.on_tick(&mut host, &team, t);
            }
            let _ = st.on_death(&mut host, a);
        }
    }

    #[test]
    fn vampiric_heals_on_player_damage() {
        let mut host = primed_host();
        let mut st = EliteState::default();
        let a = spawn_with(&mut host, &mut st, Affix::Vampiric);
        host.actor_set_health(a, 50);
        st.on_damaged_player(&mut host, Some(a), 10, 10);
        assert_eq!(host.actor_health(a), 70);
    }

    #[test]
    fn volatile_spawns_barrel_on_death() {
        let mut host = primed_host();
        let mut st = EliteState::default();
        let a = spawn_with(&mut host, &mut st, Affix::Volatile);
        let before = host.spawn_count();
        let _ = st.on_death(&mut host, a);
        assert!(host.spawn_count() > before);
    }
}

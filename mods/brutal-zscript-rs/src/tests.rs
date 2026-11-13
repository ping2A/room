//! The ported functions against a fake engine, case by case against the
//! ZScript they replace.

use crate::damage::{self, DAMAGE_CONTROL, Hit};
use crate::host::Host;
use crate::lumps::LumpTable;
use crate::zandro::{self, Args, Func, Ret};
use room_script_sdk::game::Actor;
use room_script_sdk::{ActorRef, ClassRef};
use std::collections::BTreeMap;

fn actor(n: u64) -> Actor {
    Actor(ActorRef(n))
}

#[derive(Default)]
struct FakeHost {
    skill: i32,
    players: BTreeMap<i32, Actor>,
    monsters: Vec<Actor>,
    classes: BTreeMap<u64, &'static str>,
    inv: BTreeMap<(u64, String), i32>,
    tids: BTreeMap<u64, i32>,
    sound_class: BTreeMap<u64, String>,
    genders: BTreeMap<i32, i32>,
    cvars: BTreeMap<(i32, String), i32>,
    lumps: Vec<(&'static str, Vec<u8>)>,
    printed: Vec<String>,
}

impl FakeHost {
    fn count(&self, a: Actor, item: &str) -> i32 {
        self.inv.get(&(a.0.0, item.to_owned())).copied().unwrap_or(0)
    }
}

impl Host for FakeHost {
    fn skill(&self) -> i32 {
        self.skill
    }
    fn class_find(&self, _name: &str) -> ClassRef {
        ClassRef(1)
    }
    fn player(&self, slot: i32) -> Option<Actor> {
        self.players.get(&slot).copied()
    }
    fn is_monster(&self, a: Actor) -> bool {
        self.monsters.contains(&a)
    }
    fn is_player_pawn(&self, a: Actor) -> bool {
        self.players.values().any(|&p| p == a)
    }
    fn class_name_is(&self, a: Actor, name: &str) -> bool {
        self.classes
            .get(&a.0.0)
            .is_some_and(|c| c.eq_ignore_ascii_case(name))
    }
    fn give(&mut self, a: Actor, item: &str, amount: i32) {
        *self.inv.entry((a.0.0, item.to_owned())).or_default() += amount;
    }
    fn take(&mut self, a: Actor, item: &str, amount: i32) {
        let n = self.inv.entry((a.0.0, item.to_owned())).or_default();
        *n = (*n - amount).max(0);
    }
    fn tid(&self, a: Actor) -> i32 {
        self.tids.get(&a.0.0).copied().unwrap_or(0)
    }
    fn change_tid(&mut self, a: Actor, tid: i32) {
        self.tids.insert(a.0.0, tid);
    }
    fn set_sound_class(&mut self, a: Actor, name: &str) {
        self.sound_class.insert(a.0.0, name.to_owned());
    }
    fn player_gender(&self, slot: i32) -> i32 {
        self.genders.get(&slot).copied().unwrap_or(0)
    }
    fn player_cvar(&self, slot: i32, name: &str) -> i32 {
        self.cvars
            .get(&(slot, name.to_ascii_lowercase()))
            .copied()
            .unwrap_or(0)
    }
    fn lump_find(&self, name: &str) -> i32 {
        self.lumps
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
            .map_or(-1, |i| i as i32)
    }
    fn lump_read(&self, lump: i32) -> Vec<u8> {
        self.lumps[lump as usize].1.clone()
    }
    fn print(&mut self, msg: &str) {
        self.printed.push(msg.to_owned());
    }
}

/// `ScriptCall` arguments as ACS would pass them.
#[derive(Clone)]
enum A {
    I(i32),
    S(&'static str),
}

struct Call(Vec<A>);

impl Args for Call {
    fn int(&self, i: usize) -> i32 {
        match self.0.get(i) {
            Some(A::I(v)) => *v,
            _ => 0,
        }
    }
    fn string(&self, i: usize) -> String {
        match self.0.get(i) {
            Some(A::S(s)) => (*s).to_owned(),
            _ => String::new(),
        }
    }
}

fn run(h: &mut FakeHost, t: &mut LumpTable, f: Func, args: &[A]) -> Ret {
    zandro::call(h, t, f, &Call(args.to_vec()))
}

fn int(r: Ret) -> i32 {
    match r {
        Ret::Int(v) => v,
        Ret::Str(s) => panic!("expected int, got {s:?}"),
    }
}

fn text(r: Ret) -> String {
    match r {
        Ret::Str(s) => s,
        Ret::Int(v) => panic!("expected string, got {v}"),
    }
}

#[test]
fn damage_control_is_given_only_from_skill_six() {
    let mut h = FakeHost {
        skill: 5,
        ..FakeHost::default()
    };
    h.players.insert(0, actor(1));
    h.monsters.push(actor(2));
    damage::player_entered(&mut h, 0);
    damage::thing_spawned(&mut h, actor(2));
    assert_eq!(h.count(actor(1), DAMAGE_CONTROL), 0);
    assert_eq!(h.count(actor(2), DAMAGE_CONTROL), 0);

    h.skill = 6;
    damage::player_entered(&mut h, 0);
    damage::thing_spawned(&mut h, actor(2));
    damage::thing_spawned(&mut h, actor(3)); // not a monster
    damage::player_entered(&mut h, 4); // nobody there
    assert_eq!(h.count(actor(1), DAMAGE_CONTROL), 1);
    assert_eq!(h.count(actor(2), DAMAGE_CONTROL), 1);
    assert_eq!(h.count(actor(3), DAMAGE_CONTROL), 0);
}

#[test]
fn modify_damage_matches_the_zscript() {
    let mut h = FakeHost::default();
    h.players.insert(0, actor(1));
    h.monsters.extend([actor(2), actor(3)]);
    h.classes.insert(3, "TehLostSoul");
    let hit = |source: Option<Actor>, inflictor: Option<Actor>, passive: bool, dtype| Hit {
        damage: 10,
        damage_type: dtype,
        passive,
        inflictor,
        source,
    };
    let puff = Some(actor(9));
    // Monster hits the carrier: x7, fire x3, the lost soul x2 (even fire).
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(2)), puff, true, "")), 70);
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(2)), puff, true, "Burn")), 30);
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(2)), puff, true, "burn")), 30);
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(3)), puff, true, "Burn")), 20);
    // Player sources count too.
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(1)), puff, true, "")), 70);
    // Early-outs: active side, no source, no inflictor, a barrel as source.
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(2)), puff, false, "")), 10);
    assert_eq!(damage::modify_damage(&h, &hit(None, puff, true, "")), 10);
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(2)), None, true, "")), 10);
    assert_eq!(damage::modify_damage(&h, &hit(Some(actor(7)), puff, true, "")), 10);
}

#[test]
fn lump_open_read_close() {
    let mut h = FakeHost::default();
    h.lumps.push(("PLAYPAL", vec![1, 2, 3, 200, 0x34, 0x12, 0xff, 0x7f]));
    h.lumps.push(("TITLEPIC", b"hello world".to_vec()));
    let mut t = LumpTable::default();
    use A::{I, S};

    // `ZC_LumpOpen(name)` passes start = -1, flags = 0.
    let pic = int(run(&mut h, &mut t, Func::LumpOpen, &[S("TITLEPIC"), I(-1), I(0)]));
    assert_eq!(pic, 1);
    assert_eq!(int(run(&mut h, &mut t, Func::LumpOpen, &[S("MISSING"), I(-1), I(0)])), -1);
    // FindLump from start + 1: TITLEPIC (1) is not after 1.
    assert_eq!(int(run(&mut h, &mut t, Func::LumpOpen, &[S("TITLEPIC"), I(1), I(0)])), -1);
    assert_eq!(int(run(&mut h, &mut t, Func::LumpGetInfoInt, &[I(pic), I(0)])), 11);
    assert_eq!(text(run(&mut h, &mut t, Func::LumpGetInfoStr, &[I(pic), I(1)])), "TITLEPIC");
    assert_eq!(text(run(&mut h, &mut t, Func::LumpReadString, &[I(pic), I(6), I(5)])), "world");
    assert_eq!(text(run(&mut h, &mut t, Func::LumpReadString, &[I(pic), I(6), I(99)])), "world");

    let pal = int(run(&mut h, &mut t, Func::LumpOpen, &[S("playpal"), I(-2), I(0)]));
    assert_eq!(pal, 0);
    let read = |h: &mut FakeHost, t: &mut LumpTable, pos, kind| {
        int(run(h, t, Func::LumpRead, &[I(pal), I(pos), I(kind)]))
    };
    assert_eq!(read(&mut h, &mut t, 3, zandro::LUMP_READ_UBYTE), 200);
    // BD's sign handling negates: 200 → -200.
    assert_eq!(read(&mut h, &mut t, 3, zandro::LUMP_READ_BYTE), -200);
    assert_eq!(read(&mut h, &mut t, 4, zandro::LUMP_READ_USHORT), 0x1234);
    assert_eq!(read(&mut h, &mut t, 6, zandro::LUMP_READ_SHORT), 0x7fff);
    assert_eq!(read(&mut h, &mut t, 0, zandro::LUMP_READ_INT), 0xc8_03_02_01_u32 as i32);
    assert_eq!(read(&mut h, &mut t, 7, zandro::LUMP_READ_INT), 0x7f); // past the end reads 0
    assert_eq!(read(&mut h, &mut t, 0, zandro::LUMP_READ_FLOAT), 0);
    assert!(h.printed.last().unwrap().contains("LUMP_READ_FLOAT"));

    // Two opens need two closes.
    run(&mut h, &mut t, Func::LumpOpen, &[S("PLAYPAL"), I(-2), I(0)]);
    run(&mut h, &mut t, Func::LumpClose, &[I(pal)]);
    assert_eq!(read(&mut h, &mut t, 3, zandro::LUMP_READ_UBYTE), 200);
    run(&mut h, &mut t, Func::LumpClose, &[I(pal)]);
    assert!(!t.is_open(pal));
    assert_eq!(read(&mut h, &mut t, 3, zandro::LUMP_READ_UBYTE), 0);
    assert!(h.printed.last().unwrap().contains("non-existent lump handle"));

    // Closing -1 (what a failed ZC_LumpOpen returns) only leaves a count.
    run(&mut h, &mut t, Func::LumpClose, &[I(-1)]);
    assert_eq!(t.ref_count(-1), -1);
    // A closed lump reopens with its contents.
    assert_eq!(int(run(&mut h, &mut t, Func::LumpOpen, &[S("PLAYPAL"), I(-2), I(0)])), 0);
    assert_eq!(read(&mut h, &mut t, 3, zandro::LUMP_READ_UBYTE), 200);
    assert_eq!(int(run(&mut h, &mut t, Func::LumpGetInfoInt, &[I(-1), I(0)])), 0);
}

#[test]
fn players_tids_and_gender() {
    let mut h = FakeHost::default();
    h.players.insert(0, actor(1));
    h.players.insert(1, actor(2));
    h.tids.insert(1, 1000);
    let mut t = LumpTable::default();
    use A::I;
    assert_eq!(int(run(&mut h, &mut t, Func::PlayerInGame, &[I(0)])), 1);
    assert_eq!(int(run(&mut h, &mut t, Func::PlayerInGame, &[I(3)])), 0);
    assert_eq!(int(run(&mut h, &mut t, Func::GetPlayerTid, &[I(0)])), 1000);
    assert_eq!(int(run(&mut h, &mut t, Func::GetPlayerTid, &[I(9)])), 0);
    run(&mut h, &mut t, Func::SetPlayerTid, &[I(1), I(1001)]);
    run(&mut h, &mut t, Func::SetPlayerTid, &[I(-1), I(5)]);
    assert_eq!(h.tid(actor(2)), 1001);
    assert_eq!(int(run(&mut h, &mut t, Func::IsClient, &[])), 1);
    assert_eq!(int(run(&mut h, &mut t, Func::ConsolePlayerNumber, &[])), 0);
    assert_eq!(int(run(&mut h, &mut t, Func::HasZScript, &[])), 1);

    // Male by default.
    run(&mut h, &mut t, Func::GetGender, &[I(0)]);
    assert_eq!(h.sound_class[&1], "player");
    assert_eq!((h.count(actor(1), "GenderMale"), h.count(actor(1), "GenderFemale")), (1, 0));
    // bd_Gender 1 switches to the female voice and face.
    h.cvars.insert((0, "bd_gender".into()), 1);
    run(&mut h, &mut t, Func::GetGender, &[I(0)]);
    assert_eq!(h.sound_class[&1], "fplayer");
    assert_eq!((h.count(actor(1), "GenderMale"), h.count(actor(1), "GenderFemale")), (0, 1));
    // So does a female userinfo gender.
    h.genders.insert(1, 1);
    run(&mut h, &mut t, Func::GetGender, &[I(1)]);
    assert_eq!(h.sound_class[&2], "fplayer");
    // No body: nothing happens.
    run(&mut h, &mut t, Func::GetGender, &[I(5)]);
    assert_eq!(h.sound_class.len(), 2);
}

#[test]
fn every_function_has_its_zscript_name() {
    let names: Vec<_> = Func::ALL.iter().map(|f| f.name()).collect();
    assert_eq!(names.len(), 13);
    for (i, f) in Func::ALL.iter().enumerate() {
        assert_eq!(Func::from_index(i as i32), Some(*f));
    }
    assert_eq!(Func::from_index(13), None);
    assert!(names.contains(&"GetPlayerTID") && names.contains(&"SetPlayerTID"));
}

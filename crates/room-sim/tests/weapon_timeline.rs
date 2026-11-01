//! WF0 — vanilla weapon fire/flash timeline harness (`docs/30` §1 / §6.1).
//!
//! Compares each Doom gun's compiled `Fire` / `Flash` states against the
//! reference tables under `testdata/weapons/`, and checks held/full intervals.

#![allow(clippy::unwrap_used)]

use room_sim::{ClassRegistry, StateTable};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameRef {
    sprite: String,
    frame: u8,
    tics: i16,
    action: Option<String>,
}

#[derive(Debug)]
struct WeaponRef {
    class: &'static str,
    held: i32,
    full: i32,
    fire: Vec<FrameRef>,
    flash: Vec<FrameRef>,
}

fn parse_frame(line: &str) -> FrameRef {
    let mut parts = line.split_whitespace();
    let sprite = parts.next().unwrap().to_ascii_uppercase();
    let letter = parts.next().unwrap().chars().next().unwrap();
    let frame = match letter {
        'A'..='Z' => (letter as u8) - b'A',
        _ => panic!("bad frame letter in {line}"),
    };
    let tics: i16 = parts.next().unwrap().parse().unwrap();
    let action = parts.next().map(str::to_owned);
    FrameRef {
        sprite,
        frame,
        tics,
        action,
    }
}

fn parse_ref(class: &'static str, src: &str) -> WeaponRef {
    let mut held = 0;
    let mut full = 0;
    let mut fire = Vec::new();
    let mut flash = Vec::new();
    let mut section = "";
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("held ") {
            held = rest.trim().parse().unwrap();
            continue;
        }
        if let Some(rest) = line.strip_prefix("full ") {
            full = rest.trim().parse().unwrap();
            continue;
        }
        if line.eq_ignore_ascii_case("Fire:") {
            section = "fire";
            continue;
        }
        if line.eq_ignore_ascii_case("Flash:") {
            section = "flash";
            continue;
        }
        let fr = parse_frame(line);
        match section {
            "fire" => fire.push(fr),
            "flash" => flash.push(fr),
            _ => panic!("frame outside Fire/Flash: {line}"),
        }
    }
    WeaponRef {
        class,
        held,
        full,
        fire,
        flash,
    }
}

fn walk_label(
    states: &StateTable,
    class: room_core::ClassId,
    label: &str,
    stop_at_refire: bool,
) -> Vec<FrameRef> {
    let Some(mut sid) = states.find_label(class, label) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let owner0 = states.get(sid).map(|s| s.owner);
    for _ in 0..64 {
        let Some(st) = states.get(sid) else {
            break;
        };
        // Flash segments: stop when we leave this class (Goto LightDone → Weapon).
        if label.eq_ignore_ascii_case("Flash") && Some(st.owner) != owner0 {
            break;
        }
        if st.sprite.is_empty() {
            break;
        }
        out.push(FrameRef {
            sprite: st.sprite.to_ascii_uppercase(),
            frame: st.frame,
            tics: st.tics,
            action: st.action.clone(),
        });
        if stop_at_refire && st.action.as_deref() == Some("A_ReFire") {
            break;
        }
        // Contiguous flash: next state in table even when `next` points at LightDone.
        if label.eq_ignore_ascii_case("Flash") {
            let next_id = room_core::StateId::from_raw(sid.raw() + 1);
            if states.get(next_id).is_some_and(|s| Some(s.owner) == owner0 && !s.sprite.is_empty())
            {
                sid = next_id;
                continue;
            }
            break;
        }
        match st.next {
            Some(n) => sid = n,
            None => break,
        }
    }
    out
}

fn full_tics(fire: &[FrameRef]) -> i32 {
    fire.iter().map(|f| i32::from(f.tics)).sum()
}

fn held_tics(fire: &[FrameRef]) -> i32 {
    // A_ReFire runs when its frame *starts* — that frame's tics are not spent.
    let mut sum = 0i32;
    for f in fire {
        if f.action.as_deref() == Some("A_ReFire") {
            break;
        }
        sum += i32::from(f.tics);
    }
    sum
}

fn load_classes() -> (Arc<ClassRegistry>, Arc<StateTable>) {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let file = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
    let states = Arc::new(StateTable::from_decorate(&file, &classes));
    (classes, states)
}

macro_rules! weapon_case {
    ($name:ident, $class:expr, $file:expr) => {
        #[test]
        fn $name() {
            let reference = parse_ref($class, include_str!(concat!("../../../testdata/weapons/", $file)));
            let (classes, states) = load_classes();
            let cid = classes.find_name(reference.class).unwrap_or_else(|| {
                panic!("missing class {}", reference.class);
            });
            let fire = walk_label(&states, cid, "Fire", true);
            assert_eq!(
                fire, reference.fire,
                "{} Fire frames mismatch",
                reference.class
            );
            assert_eq!(
                full_tics(&fire),
                reference.full,
                "{} full interval",
                reference.class
            );
            assert_eq!(
                held_tics(&fire),
                reference.held,
                "{} held interval",
                reference.class
            );
            if !reference.flash.is_empty() {
                let flash = walk_label(&states, cid, "Flash", false);
                assert_eq!(
                    flash, reference.flash,
                    "{} Flash frames mismatch",
                    reference.class
                );
            }
        }
    };
}

weapon_case!(timeline_fist, "Fist", "fist.txt");
weapon_case!(timeline_chainsaw, "Chainsaw", "chainsaw.txt");
weapon_case!(timeline_pistol, "Pistol", "pistol.txt");
weapon_case!(timeline_shotgun, "Shotgun", "shotgun.txt");
weapon_case!(timeline_supershotgun, "SuperShotgun", "supershotgun.txt");
weapon_case!(timeline_chaingun, "Chaingun", "chaingun.txt");
weapon_case!(timeline_rocket, "RocketLauncher", "rocketlauncher.txt");
weapon_case!(timeline_plasma, "PlasmaRifle", "plasmarifle.txt");
weapon_case!(timeline_bfg, "BFG9000", "bfg9000.txt");

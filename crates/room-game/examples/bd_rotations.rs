//! BD: for every sprite frame a monster can show, check each of the 8 view
//! rotations resolves with the renderer's lookup (`sprite_lump_candidates_rot`).
//! A miss means the enemy vanishes when seen from that side.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_ui::PatchCache;
use room_vfs::VfsBuilder;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&a[1])
        .unwrap()
        .mount_path(&a[2])
        .unwrap()
        .build()
        .unwrap();
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    let (classes, states) = (data.classes, data.states);
    let mut patches = PatchCache::from_vfs(&vfs);
    // All sprite-ish lump names, grouped by 4-char prefix.
    let mut by_prefix: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_, li) in vfs.iter() {
        let n = li.short_name.to_string().to_ascii_uppercase();
        if n.len() >= 6 {
            by_prefix.entry(n[..4].to_string()).or_default().insert(n);
        }
    }
    let monsters: BTreeSet<_> = classes
        .iter()
        .filter(|c| c.flags & room_sim::FLAG_COUNTKILL != 0)
        .map(|c| c.id)
        .collect();
    let mut frames: BTreeMap<(String, u8), BTreeSet<String>> = BTreeMap::new();
    for i in 0..states.len() {
        let Some(st) = states.get(room_core::StateId::from_raw(i as u32)) else {
            continue;
        };
        if !monsters.contains(&st.owner) && std::env::var_os("ALL").is_none() {
            continue;
        }
        let s = st.sprite.to_ascii_uppercase();
        if s.len() != 4 || s == "TNT1" || s == "----" || s == "####" {
            continue;
        }
        if st.frame > 28 {
            continue;
        }
        let owner = classes
            .get(st.owner)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        frames.entry((s, st.frame)).or_default().insert(owner);
    }
    let mut bad = 0;
    for ((s, f), owners) in &frames {
        let mut miss = Vec::new();
        for rot in 1..=8u8 {
            let mut c = cands(s, *f, rot);
            if std::env::var_os("NO_INDEX").is_none() {
                if let Some(hit) = patches.sprite_frame_lump(&vfs, s, *f, rot) {
                    c.insert(0, hit);
                }
            }
            if !c.iter().any(|(n, _)| patches.ensure(&vfs, n).is_some()) {
                miss.push(rot);
            }
        }
        if !miss.is_empty() {
            bad += 1;
            let l = letter(*f);
            let have: Vec<_> = by_prefix
                .get(s)
                .map(|v| v.iter().filter(|n| n[4..].contains(l)).cloned().collect())
                .unwrap_or_default();
            if bad <= 100000 {
                println!(
                    "{s}{l} missing rot {miss:?} owners={:?} lumps={have:?}",
                    owners.iter().take(3).collect::<Vec<_>>()
                );
            }
        }
    }
    println!("frames={} with_missing_rotations={bad}", frames.len());
}

fn letter(f: u8) -> char {
    match f {
        0..=25 => char::from(b'A' + f),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    }
}
fn mirror(d: u8) -> Option<u8> {
    match d {
        2 => Some(8),
        3 => Some(7),
        4 => Some(6),
        6 => Some(4),
        7 => Some(3),
        8 => Some(2),
        _ => None,
    }
}
/// Copy of `room_render::sprite_lump_candidates_rot`.
fn cands(s: &str, f: u8, rot: u8) -> Vec<(String, bool)> {
    let l = letter(f);
    let mut out = vec![(format!("{s}{l}{rot}"), false)];
    if let Some(m) = mirror(rot) {
        out.push((format!("{s}{l}{m}"), true));
        out.push((format!("{s}{l}{rot}{l}{m}"), false));
        out.push((format!("{s}{l}{m}{l}{rot}"), true));
    }
    out.push((format!("{s}{l}0"), false));
    out.push((format!("{s}{l}1"), false));
    out
}

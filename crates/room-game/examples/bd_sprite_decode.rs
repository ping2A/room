//! Static check: every sprite frame used by any state resolves to a picture
//! Room can actually decode (PatchCache), split by "no lump" / "lump but decode failed".
use room_ui::PatchCache;
use room_vfs::{Namespace, VfsBuilder};
use std::collections::{BTreeMap, BTreeSet};

fn letter(f: u8) -> char {
    match f {
        0..=25 => char::from(b'A' + f),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    }
}

fn main() {
    let iwad = std::env::args().nth(1).unwrap();
    let pk3 = std::env::args().nth(2).unwrap();
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&iwad)
        .unwrap()
        .mount_path(&pk3)
        .unwrap()
        .build()
        .unwrap();
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    let mut patches = PatchCache::from_vfs(&vfs);
    let mut seen = BTreeSet::new();
    let mut no_lump: BTreeMap<String, u32> = BTreeMap::new();
    let mut bad_decode: BTreeMap<String, String> = BTreeMap::new();
    let mut ok = 0;
    for i in 0..data.states.len() {
        let Some(st) = data.states.get(room_core::StateId::from_raw(i as u32)) else {
            continue;
        };
        let s = st.sprite.trim().to_ascii_uppercase();
        if s.is_empty() || s == "TNT1" || s == "----" || s == "####" {
            continue;
        }
        if !seen.insert((s.clone(), st.frame)) {
            continue;
        }
        let l = letter(st.frame);
        let mut names = vec![format!("{s}{l}0")];
        for r in 1..=8 {
            names.push(format!("{s}{l}{r}"));
        }
        for (a, b) in [(2, 8), (3, 7), (4, 6), (8, 2), (7, 3), (6, 4)] {
            names.push(format!("{s}{l}{a}{l}{b}"));
        }
        let mut any_ok = false;
        let mut existing = None;
        for n in &names {
            if patches.ensure(&vfs, n).is_some() {
                any_ok = true;
                break;
            }
            let id = vfs
                .find(n, Namespace::Sprites)
                .or_else(|| vfs.find_any_ns(n));
            if id.is_some() && existing.is_none() {
                existing = Some(n.clone());
            }
        }
        let cls = data
            .classes
            .get(st.owner)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        if any_ok {
            ok += 1;
        } else if let Some(n) = existing {
            let id = vfs
                .find(&n, Namespace::Sprites)
                .or_else(|| vfs.find_any_ns(&n))
                .unwrap();
            let b = vfs.read(id).unwrap();
            let info = vfs
                .lump(id)
                .map(|i| i.full_path.clone())
                .unwrap_or_default();
            bad_decode.insert(
                format!("{cls} {s}{l} ({n})"),
                format!(
                    "{info} {} bytes head={:02x?}",
                    b.len(),
                    &b[..b.len().min(8)]
                ),
            );
        } else {
            *no_lump.entry(format!("{s}{l}")).or_default() += 1;
            let _ = cls;
        }
    }
    println!(
        "frames ok {ok}, no lump {}, lump-but-undecodable {}",
        no_lump.len(),
        bad_decode.len()
    );
    for (k, v) in bad_decode.iter().take(40) {
        println!("  UNDECODABLE {k}: {v}");
    }
    println!(
        "no-lump sample: {:?}",
        no_lump.keys().take(40).collect::<Vec<_>>()
    );
}

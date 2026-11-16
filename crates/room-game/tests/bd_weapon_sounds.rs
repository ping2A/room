//! Every sound Brutal Doom's weapons play resolves and decodes on top of a
//! given IWAD: `ROOM_BD_IWAD=…/freedoom2.wad ROOM_BRUTALDOOM_PK3=…/brutal.pk3
//! cargo test -p room-game --test bd_weapon_sounds -- --ignored --nocapture`.
//! The sound list (`tests/data/bd_weapon_sounds.txt`) is every name the
//! pk3's `actors/Weapons/*` play; `ROOM_BD_SOUNDS` overrides it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
#[ignore = "needs ROOM_BD_IWAD and ROOM_BRUTALDOOM_PK3"]
fn brutal_weapon_sounds_resolve() {
    let iwad = std::env::var("ROOM_BD_IWAD").unwrap();
    let pk3 = std::env::var("ROOM_BRUTALDOOM_PK3").unwrap();
    let list = std::env::var("ROOM_BD_SOUNDS")
        .map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| include_str!("data/bd_weapon_sounds.txt").to_owned());
    let mut b = room_game::mount_embedded_engine(room_vfs::VfsBuilder::new()).unwrap();
    b = b.mount_path(&iwad).unwrap().mount_path(&pk3).unwrap();
    let vfs = b.build().unwrap();
    let mut snd = room_game::SoundSystem::new();
    snd.load_sndinfo_from_vfs(&vfs).unwrap();
    let mut bad = Vec::new();
    for name in list.lines().filter(|l| !l.trim().is_empty()) {
        if snd.resolve(&vfs, name, 0).is_none() {
            bad.push(name.to_owned());
        }
    }
    // Why: definition, lump lookup or decoding.
    let mut text = String::new();
    for id in vfs.find_all("SNDINFO", room_vfs::Namespace::Global) {
        text.push_str(&String::from_utf8_lossy(&vfs.read(id).unwrap()));
        text.push('\n');
    }
    let reg = room_defs::parse_sndinfo(&text).unwrap();
    for name in &bad {
        let lump = reg.resolve_lump(name, 0).map(str::to_owned);
        let Some(lump) = lump else {
            eprintln!("  {name}: not defined in SNDINFO");
            continue;
        };
        let key = lump.to_ascii_uppercase();
        let id = vfs
            .find(&key, room_vfs::Namespace::Sounds)
            .or_else(|| vfs.find(&key, room_vfs::Namespace::Global))
            .or_else(|| vfs.find_any_ns(&key));
        match id {
            None => eprintln!("  {name} → {lump}: no such lump"),
            Some(id) => {
                let bytes = vfs.read(id).unwrap();
                match room_formats::sound::decode_sound(&bytes) {
                    Ok(_) => eprintln!("  {name} → {lump}: decodes fine?"),
                    Err(e) => eprintln!(
                        "  {name} → {lump}: decode error {e} ({} bytes, {:?})",
                        bytes.len(),
                        &bytes[..4.min(bytes.len())]
                    ),
                }
            }
        }
    }
    eprintln!(
        "{} of {} weapon sounds do not resolve:\n{}",
        bad.len(),
        list.lines().count(),
        bad.join("\n")
    );
    // Regression: a two-number `$rolloff` used to swallow the next line's
    // name and shift every later definition (rifle reload/select, plasma,
    // rocket, sniper, chaingun all went silent).
    for must in [
        "Rifle/Fire",
        "Rifle/Select",
        "Rifle/Reload2",
        "Plasma/Fire",
        "Rocket/Fire",
        "Sniper/Fire",
        "Chaingun/Fire",
        "FARRIF",
    ] {
        assert!(
            !bad.iter().any(|b| b.eq_ignore_ascii_case(must)),
            "{must} does not resolve"
        );
    }
}

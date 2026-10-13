//! Save / load bootstrap (`.fsave` ZIP with `info.json` + `world.bin` [+ `scripts.bin`]).
//!
//! Format v2 stores player armor class, pitch, ready weapon, and inventory stacks.
//! Optional `scripts.bin` holds WASM play-module memory (M11).

use room_core::{Angle, DVec3};
use room_sim::{ModuleMemorySnapshot, PlayerState, TicCmd, World, set_inventory};
use std::io::{Cursor, Read, Write};
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Save format errors.
#[derive(Debug, Error)]
pub enum SaveError {
    /// Parse / I/O / mismatch.
    #[error("{0}")]
    Message(String),
}

/// Metadata written to `info.json` (minimal M5 subset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveInfo {
    /// Format version.
    pub format_version: u32,
    /// Map lump name.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Level time at save.
    pub level_time: u32,
    /// World checksum at save (for smoke verify).
    pub checksum: u64,
}

/// `2` = armor class, pitch, ready weapon, inventory stacks.
const FORMAT_VERSION: u32 = 2;
const WORLD_MAGIC: &[u8; 4] = b"FWLD";

/// Capture a save blob from the current world.
pub fn save_world(world: &mut World, map: &str, skill: u8) -> Result<Vec<u8>, SaveError> {
    let info = SaveInfo {
        format_version: FORMAT_VERSION,
        map: map.to_owned(),
        skill,
        level_time: world.level.time,
        checksum: world.checksum(),
    };
    let info_json = format!(
        "{{\"format_version\":{},\"map\":{},\"skill\":{},\"level_time\":{},\"checksum\":{}}}",
        info.format_version,
        json_string(&info.map),
        info.skill,
        info.level_time,
        info.checksum
    );
    let world_bin = encode_world(world)?;
    let scripts_bin = encode_scripts(&world.scripts.snapshot_play_modules());

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("info.json", opts)
            .map_err(|e| SaveError::Message(e.to_string()))?;
        zip.write_all(info_json.as_bytes())
            .map_err(|e| SaveError::Message(e.to_string()))?;
        zip.start_file("world.bin", opts)
            .map_err(|e| SaveError::Message(e.to_string()))?;
        zip.write_all(&world_bin)
            .map_err(|e| SaveError::Message(e.to_string()))?;
        if !scripts_bin.is_empty() {
            zip.start_file("scripts.bin", opts)
                .map_err(|e| SaveError::Message(e.to_string()))?;
            zip.write_all(&scripts_bin)
                .map_err(|e| SaveError::Message(e.to_string()))?;
        }
        zip.finish()
            .map_err(|e| SaveError::Message(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

/// Read `info.json` from a `.fsave` blob.
pub fn read_save_info(bytes: &[u8]) -> Result<SaveInfo, SaveError> {
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| SaveError::Message(e.to_string()))?;
    let mut file = zip
        .by_name("info.json")
        .map_err(|e| SaveError::Message(e.to_string()))?;
    let mut s = String::new();
    file.read_to_string(&mut s)
        .map_err(|e| SaveError::Message(e.to_string()))?;
    parse_info_json(&s)
}

/// Structurally validate a `world.bin` blob without a live [`World`].
///
/// Caps sector / inventory counts so fuzz inputs cannot allocate unbounded work.
/// Returns `Ok(())` when the layout is well-formed for format v2.
pub fn validate_world_bin(bin: &[u8]) -> Result<(), SaveError> {
    const MAX_SECTORS: usize = 65_536;
    const MAX_INV: usize = 4_096;
    const MAX_NAME: usize = 1_024;

    if bin.len() < 4 + 4 + 4 + 4 + 4 {
        return Err(SaveError::Message("world.bin too short".into()));
    }
    if &bin[0..4] != WORLD_MAGIC {
        return Err(SaveError::Message("bad world.bin magic".into()));
    }
    let mut o = 4usize;
    let ver = read_u32(bin, o)?;
    o += 4;
    if ver != FORMAT_VERSION {
        return Err(SaveError::Message(format!("unsupported world.bin {ver}")));
    }
    let _level_time = read_u32(bin, o)?;
    o += 4;
    let _rng_seed = read_u32(bin, o)?;
    o += 4;
    let nsec = read_u32(bin, o)? as usize;
    o += 4;
    if nsec > MAX_SECTORS {
        return Err(SaveError::Message("sector count too large".into()));
    }
    let sector_bytes = nsec
        .checked_mul(8 + 8 + 2)
        .ok_or_else(|| SaveError::Message("sector size overflow".into()))?;
    if o.checked_add(sector_bytes).map_or(true, |end| end > bin.len()) {
        return Err(SaveError::Message("sectors truncated".into()));
    }
    o += sector_bytes;
    // Player core: x y z ang health armor + 8 flags + state
    if bin.len() < o + 8 * 4 + 4 + 4 + 9 {
        return Err(SaveError::Message("player fields truncated".into()));
    }
    o += 8 * 4 + 4 + 4 + 9;
    // v2 trailer: armor_class + pitch + ready name + inv
    if bin.len() < o + 1 + 8 + 2 {
        return Err(SaveError::Message("v2 player trailer truncated".into()));
    }
    o += 1; // armor_class
    let _pitch = read_u64(bin, o)?;
    o += 8;
    let (ready_name, n) = read_pascal_str(bin, o)?;
    if ready_name.len() > MAX_NAME {
        return Err(SaveError::Message("ready weapon name too long".into()));
    }
    o = n;
    let n_inv = read_u16(bin, o)? as usize;
    o += 2;
    if n_inv > MAX_INV {
        return Err(SaveError::Message("inventory count too large".into()));
    }
    for _ in 0..n_inv {
        let (name, n) = read_pascal_str(bin, o)?;
        if name.len() > MAX_NAME {
            return Err(SaveError::Message("inventory name too long".into()));
        }
        o = n;
        let _amount = read_i32(bin, o)?;
        o += 4;
    }
    let _ = o;
    Ok(())
}

/// Apply `world.bin` (+ optional `scripts.bin`) into an already-loaded world.
/// Returns the checksum stored in the save (caller may compare to `world.checksum()`).
pub fn load_world(world: &mut World, bytes: &[u8]) -> Result<u64, SaveError> {
    let info = read_save_info(bytes)?;
    if info.format_version != FORMAT_VERSION {
        return Err(SaveError::Message(format!(
            "unsupported save format {}",
            info.format_version
        )));
    }
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| SaveError::Message(e.to_string()))?;
    let mut bin = Vec::new();
    {
        let mut file = zip
            .by_name("world.bin")
            .map_err(|e| SaveError::Message(e.to_string()))?;
        file.read_to_end(&mut bin)
            .map_err(|e| SaveError::Message(e.to_string()))?;
    }
    decode_world(world, &bin)?;
    if let Ok(mut file) = zip.by_name("scripts.bin") {
        let mut scripts = Vec::new();
        file.read_to_end(&mut scripts)
            .map_err(|e| SaveError::Message(e.to_string()))?;
        let snaps = decode_scripts(&scripts)?;
        world
            .scripts
            .restore_play_modules(&snaps)
            .map_err(|e| SaveError::Message(e.to_string()))?;
    }
    Ok(info.checksum)
}

const SCRIPTS_MAGIC: &[u8; 4] = b"FSCR";
const SCRIPTS_VERSION: u32 = 1;

fn encode_scripts(snaps: &[ModuleMemorySnapshot]) -> Vec<u8> {
    if snaps.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    out.extend_from_slice(SCRIPTS_MAGIC);
    out.extend_from_slice(&SCRIPTS_VERSION.to_le_bytes());
    out.extend_from_slice(&(snaps.len() as u32).to_le_bytes());
    for s in snaps {
        let name = s.name.as_bytes();
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&s.module_hash.to_le_bytes());
        out.extend_from_slice(&(s.memory.len() as u32).to_le_bytes());
        out.extend_from_slice(&s.memory);
    }
    out
}

fn decode_scripts(bytes: &[u8]) -> Result<Vec<ModuleMemorySnapshot>, SaveError> {
    if bytes.len() < 12 || &bytes[0..4] != SCRIPTS_MAGIC {
        return Err(SaveError::Message("bad scripts.bin magic".into()));
    }
    let ver = read_u32(bytes, 4)?;
    if ver != SCRIPTS_VERSION {
        return Err(SaveError::Message(format!("scripts.bin version {ver}")));
    }
    let count = read_u32(bytes, 8)? as usize;
    let mut off = 12;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let nlen = read_u32(bytes, off)? as usize;
        off += 4;
        if off + nlen + 8 + 4 > bytes.len() {
            return Err(SaveError::Message("scripts.bin truncated name".into()));
        }
        let name = String::from_utf8(bytes[off..off + nlen].to_vec())
            .map_err(|_| SaveError::Message("scripts.bin bad name".into()))?;
        off += nlen;
        let module_hash = read_u64(bytes, off)?;
        off += 8;
        let mlen = read_u32(bytes, off)? as usize;
        off += 4;
        if off + mlen > bytes.len() {
            return Err(SaveError::Message("scripts.bin truncated memory".into()));
        }
        let memory = bytes[off..off + mlen].to_vec();
        off += mlen;
        out.push(ModuleMemorySnapshot {
            name,
            module_hash,
            memory,
        });
    }
    Ok(out)
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_info_json(s: &str) -> Result<SaveInfo, SaveError> {
    // Tiny hand parser for the fixed shape we emit (no serde yet).
    let format_version = json_u32(s, "format_version")?;
    let map = json_str(s, "map")?;
    let skill = json_u32(s, "skill")? as u8;
    let level_time = json_u32(s, "level_time")?;
    let checksum = json_u64(s, "checksum")?;
    Ok(SaveInfo {
        format_version,
        map,
        skill,
        level_time,
        checksum,
    })
}

fn json_u32(s: &str, key: &str) -> Result<u32, SaveError> {
    let needle = format!("\"{key}\":");
    let rest = s
        .split_once(&needle)
        .map(|(_, r)| r.trim_start())
        .ok_or_else(|| SaveError::Message(format!("missing {key}")))?;
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse()
        .map_err(|_| SaveError::Message(format!("bad {key}")))
}

fn json_u64(s: &str, key: &str) -> Result<u64, SaveError> {
    let needle = format!("\"{key}\":");
    let rest = s
        .split_once(&needle)
        .map(|(_, r)| r.trim_start())
        .ok_or_else(|| SaveError::Message(format!("missing {key}")))?;
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse()
        .map_err(|_| SaveError::Message(format!("bad {key}")))
}

fn json_str(s: &str, key: &str) -> Result<String, SaveError> {
    let needle = format!("\"{key}\":\"");
    let rest = s
        .split_once(&needle)
        .map(|(_, r)| r)
        .ok_or_else(|| SaveError::Message(format!("missing {key}")))?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => break,
            },
            c => out.push(c),
        }
    }
    Ok(out)
}

fn encode_world(world: &World) -> Result<Vec<u8>, SaveError> {
    let mut p = Vec::new();
    p.extend_from_slice(WORLD_MAGIC);
    p.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    p.extend_from_slice(&world.level.time.to_le_bytes());
    p.extend_from_slice(&world.rng.seed().to_le_bytes());
    p.extend_from_slice(&(world.sectors.len() as u32).to_le_bytes());
    for s in &world.sectors {
        p.extend_from_slice(&s.floor_z.to_bits().to_le_bytes());
        p.extend_from_slice(&s.ceiling_z.to_bits().to_le_bytes());
        p.extend_from_slice(&s.light.to_le_bytes());
    }
    // Player 0 bootstrap.
    let Some(player) = world.players[0].as_ref() else {
        return Err(SaveError::Message("no player 0".into()));
    };
    let mo = player
        .mo
        .ok_or_else(|| SaveError::Message("player has no mo".into()))?;
    let actor = world
        .actors
        .get(mo)
        .ok_or_else(|| SaveError::Message("player mo missing".into()))?;
    p.extend_from_slice(&actor.pos.x().to_bits().to_le_bytes());
    p.extend_from_slice(&actor.pos.y().to_bits().to_le_bytes());
    p.extend_from_slice(&actor.pos.z().to_bits().to_le_bytes());
    p.extend_from_slice(&actor.angle.degrees().to_bits().to_le_bytes());
    p.extend_from_slice(&actor.health.to_le_bytes());
    p.extend_from_slice(&player.armor.to_le_bytes());
    p.push(u8::from(player.god));
    p.push(u8::from(player.berserk));
    p.push(u8::from(player.invulnerable));
    p.push(u8::from(player.blur));
    p.push(u8::from(player.radsuit));
    p.push(u8::from(player.allmap));
    p.push(u8::from(player.infrared));
    p.push(u8::from(player.backpack));
    p.push(match player.player_state {
        PlayerState::Gone => 0,
        PlayerState::Live => 1,
        PlayerState::Dead => 2,
        PlayerState::Reborn => 3,
    });
    p.push(player.armor_class);
    p.extend_from_slice(&actor.pitch.degrees().to_bits().to_le_bytes());
    let ready_name = player
        .ready_weapon
        .and_then(|cid| world.classes.get(cid))
        .map(|c| c.name.as_str())
        .unwrap_or("");
    write_pascal_str(&mut p, ready_name);
    let inv: Vec<(String, i32)> = world
        .inventory
        .iter(mo)
        .filter(|i| i.amount > 0)
        .filter_map(|i| {
            world
                .classes
                .get(i.class)
                .map(|c| (c.name.clone(), i.amount))
        })
        .collect();
    p.extend_from_slice(&(inv.len() as u16).to_le_bytes());
    for (name, amount) in inv {
        write_pascal_str(&mut p, &name);
        p.extend_from_slice(&amount.to_le_bytes());
    }
    Ok(p)
}

fn decode_world(world: &mut World, bin: &[u8]) -> Result<(), SaveError> {
    if bin.len() < 4 + 4 + 4 + 4 + 4 {
        return Err(SaveError::Message("world.bin too short".into()));
    }
    if &bin[0..4] != WORLD_MAGIC {
        return Err(SaveError::Message("bad world.bin magic".into()));
    }
    let mut o = 4usize;
    let ver = read_u32(bin, o)?;
    o += 4;
    if ver != FORMAT_VERSION {
        return Err(SaveError::Message(format!("unsupported world.bin {ver}")));
    }
    world.level.time = read_u32(bin, o)?;
    o += 4;
    let _rng_seed = read_u32(bin, o)?;
    o += 4;
    let nsec = read_u32(bin, o)? as usize;
    o += 4;
    if nsec != world.sectors.len() {
        return Err(SaveError::Message("sector count mismatch".into()));
    }
    for i in 0..nsec {
        let floor = f64::from_bits(read_u64(bin, o)?);
        o += 8;
        let ceil = f64::from_bits(read_u64(bin, o)?);
        o += 8;
        let light = read_i16(bin, o)?;
        o += 2;
        if let Some(s) = world.sectors.get_mut(i) {
            s.floor_z = floor;
            s.ceiling_z = ceil;
            s.light = light;
        }
    }
    let x = f64::from_bits(read_u64(bin, o)?);
    o += 8;
    let y = f64::from_bits(read_u64(bin, o)?);
    o += 8;
    let z = f64::from_bits(read_u64(bin, o)?);
    o += 8;
    let ang = f64::from_bits(read_u64(bin, o)?);
    o += 8;
    let health = read_i32(bin, o)?;
    o += 4;
    let armor = read_i32(bin, o)?;
    o += 4;
    if bin.len() < o + 9 {
        return Err(SaveError::Message("player fields truncated".into()));
    }
    let flags = &bin[o..o + 8];
    let pstate = bin[o + 8];
    let mo = world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .ok_or_else(|| SaveError::Message("no player mo to restore".into()))?;
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = DVec3::new(x, y, z);
        a.angle = Angle::from_degrees(ang);
        a.health = health;
    }
    if let Some(p) = world.players[0].as_mut() {
        p.armor = armor;
        p.god = flags[0] != 0;
        p.berserk = flags[1] != 0;
        p.invulnerable = flags[2] != 0;
        p.blur = flags[3] != 0;
        p.radsuit = flags[4] != 0;
        p.allmap = flags[5] != 0;
        p.infrared = flags[6] != 0;
        p.backpack = flags[7] != 0;
        p.player_state = match pstate {
            1 => PlayerState::Live,
            2 => PlayerState::Dead,
            3 => PlayerState::Reborn,
            _ => PlayerState::Gone,
        };
        p.cmd = TicCmd::default();
    }
    o += 9;
    // Format v2+: armor class, pitch, ready weapon, inventory.
    if bin.len() < o + 1 + 8 + 2 {
        return Err(SaveError::Message("v2 player trailer truncated".into()));
    }
    let armor_class = bin[o];
    o += 1;
    let pitch_deg = f64::from_bits(read_u64(bin, o)?);
    o += 8;
    let (ready_name, n) = read_pascal_str(bin, o)?;
    o = n;
    if let Some(a) = world.actors.get_mut(mo) {
        a.pitch = Angle::from_degrees(pitch_deg);
    }
    if let Some(p) = world.players[0].as_mut() {
        p.armor_class = armor_class;
        p.ready_weapon = if ready_name.is_empty() {
            None
        } else {
            world.classes.find_name(&ready_name)
        };
    }
    let n_inv = read_u16(bin, o)? as usize;
    o += 2;
    world.inventory.clear(mo);
    for _ in 0..n_inv {
        let (name, n) = read_pascal_str(bin, o)?;
        o = n;
        let amount = read_i32(bin, o)?;
        o += 4;
        if let Some(cid) = world.classes.find_name(&name) {
            set_inventory(world, mo, cid, amount);
        }
    }
    let _ = o;
    Ok(())
}

fn write_pascal_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = u16::try_from(bytes.len()).unwrap_or(u16::MAX) as usize;
    buf.extend_from_slice(&(len as u16).to_le_bytes());
    buf.extend_from_slice(&bytes[..len]);
}

fn read_pascal_str(buf: &[u8], o: usize) -> Result<(String, usize), SaveError> {
    let len = read_u16(buf, o)? as usize;
    let start = o + 2;
    let end = start + len;
    let bytes = buf
        .get(start..end)
        .ok_or_else(|| SaveError::Message("truncated string".into()))?;
    let s = std::str::from_utf8(bytes)
        .map_err(|_| SaveError::Message("non-utf8 string".into()))?
        .to_owned();
    Ok((s, end))
}

fn read_u16(buf: &[u8], o: usize) -> Result<u16, SaveError> {
    let b: [u8; 2] = buf
        .get(o..o + 2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| SaveError::Message("truncated u16".into()))?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(buf: &[u8], o: usize) -> Result<u32, SaveError> {
    let b: [u8; 4] = buf
        .get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| SaveError::Message("truncated u32".into()))?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(buf: &[u8], o: usize) -> Result<u64, SaveError> {
    let b: [u8; 8] = buf
        .get(o..o + 8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| SaveError::Message("truncated u64".into()))?;
    Ok(u64::from_le_bytes(b))
}

fn read_i16(buf: &[u8], o: usize) -> Result<i16, SaveError> {
    let b: [u8; 2] = buf
        .get(o..o + 2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| SaveError::Message("truncated i16".into()))?;
    Ok(i16::from_le_bytes(b))
}

fn read_i32(buf: &[u8], o: usize) -> Result<i32, SaveError> {
    let b: [u8; 4] = buf
        .get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| SaveError::Message("truncated i32".into()))?;
    Ok(i32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_sim::World;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn save_load_restores_player_and_sectors() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].floor_z = 16.0;
        w.sectors[0].light = 200;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(140.0, 150.0, 16.0);
            a.health = 77;
            a.pitch = Angle::from_degrees(-12.0);
        }
        if let Some(p) = w.players[0].as_mut() {
            p.armor = 50;
            p.armor_class = 2;
            p.berserk = true;
            p.allmap = true;
        }
        w.level.time = 1234;
        let checksum = w.checksum();
        let blob = save_world(&mut w, "MAP01", 3).unwrap();
        let info = read_save_info(&blob).unwrap();
        assert_eq!(info.map, "MAP01");
        assert_eq!(info.level_time, 1234);
        assert_eq!(info.checksum, checksum);

        // Fresh world, then load.
        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map2);
        let mut w2 = World::from_map(map2).unwrap();
        let stored = load_world(&mut w2, &blob).unwrap();
        assert_eq!(stored, info.checksum);
        assert_eq!(w2.level.time, 1234);
        assert!((w2.sectors[0].floor_z - 16.0).abs() < 1e-9);
        assert_eq!(w2.sectors[0].light, 200);
        let mo2 = w2.players[0].as_ref().unwrap().mo.unwrap();
        let a = w2.actors.get(mo2).unwrap();
        assert!((a.pos.x() - 140.0).abs() < 1e-9);
        assert!((a.pos.y() - 150.0).abs() < 1e-9);
        assert_eq!(a.health, 77);
        assert!((a.pitch.degrees() + 12.0).abs() < 1e-9);
        let p = w2.players[0].as_ref().unwrap();
        assert_eq!(p.armor, 50);
        assert_eq!(p.armor_class, 2);
        assert!(p.berserk);
        assert!(p.allmap);

        let mut zip = ZipArchive::new(Cursor::new(&blob)).unwrap();
        let mut world_bin = Vec::new();
        zip.by_name("world.bin")
            .unwrap()
            .read_to_end(&mut world_bin)
            .unwrap();
        validate_world_bin(&world_bin).expect("encoded world.bin validates");
        assert!(validate_world_bin(b"nope").is_err());
        assert!(validate_world_bin(&[]).is_err());
    }

    #[test]
    fn save_load_restores_inventory_and_ready_weapon() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable, count_inventory, set_inventory};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let key = classes.find_name("BlueCard").unwrap();
        set_inventory(&mut w, mo, clip, 40);
        set_inventory(&mut w, mo, shotgun, 1);
        set_inventory(&mut w, mo, key, 1);
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(shotgun);
        }
        let blob = save_world(&mut w, "MAP01", 2).unwrap();

        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map2);
        let mut w2 = World::from_map_with_classes(map2, classes, states).unwrap();
        load_world(&mut w2, &blob).unwrap();
        let mo2 = w2.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(count_inventory(&w2, mo2, clip), 40);
        assert_eq!(count_inventory(&w2, mo2, shotgun), 1);
        assert_eq!(count_inventory(&w2, mo2, key), 1);
        assert_eq!(w2.players[0].as_ref().unwrap().ready_weapon, Some(shotgun));
    }

    #[test]
    fn save_load_restores_wasm_module_memory() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let wasm = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/save_counter.wat"
        ))
        .unwrap();
        w.scripts.load_play_module("counter", &wasm).unwrap();
        assert_eq!(w.scripts.dispatch_first(1, 0, 0).unwrap(), 1);
        assert_eq!(w.scripts.dispatch_first(1, 0, 0).unwrap(), 2);
        let blob = save_world(&mut w, "MAP01", 3).unwrap();

        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map2);
        let mut w2 = World::from_map(map2).unwrap();
        w2.scripts.load_play_module("counter", &wasm).unwrap();
        assert_eq!(w2.scripts.dispatch_first(1, 0, 0).unwrap(), 1);
        load_world(&mut w2, &blob).unwrap();
        assert_eq!(w2.scripts.dispatch_first(2, 0, 0).unwrap(), 2);
    }
}

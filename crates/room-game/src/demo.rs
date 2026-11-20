//! Room demo record / verify (`.fdem` FDHD+BODY+CSUM bootstrap).

use room_defs::Complevel;
use room_sim::{Buttons, TicCmd, World};
use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// Checksums every N tics (GZDoom-style consistency interval).
pub const CSUM_INTERVAL: u32 = 35;

/// `.fdem` container format version.
pub const FDEM_FORMAT_VERSION: u16 = 1;
/// Engine version stamped into `FDHD` (bump when TicCmd layout changes).
pub const FDEM_ENGINE_VERSION: u16 = 1;
/// Script ABI version stamped into `FDHD`.
pub const FDEM_ABI_VERSION: u16 = 1;

/// Demo errors.
#[derive(Debug, Error)]
pub enum DemoError {
    /// Verify mismatch / truncated data / bad file.
    #[error("{0}")]
    Message(String),
}

/// Compact recorded tic (all players' cmds + world checksum when due).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoTic {
    /// Player ticcmds (index = player slot).
    pub cmds: Vec<TicCmd>,
    /// World checksum after this tic, if recorded.
    pub checksum: Option<u64>,
}

/// In-memory demo recorder (`-record` path later writes `.fdem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemoRecorder {
    /// Map lump name.
    pub map: String,
    /// Skill level (0..=4).
    pub skill: u8,
    /// RNG seed at start.
    pub rng_seed: u32,
    /// `COMPLVL` / compatmode preset at record time (`Complevel`).
    pub complevel: Complevel,
    /// Player slots present at record time.
    pub player_count: u8,
    /// Local console player index.
    pub consoleplayer: u8,
    /// Recorded tics.
    pub tics: Vec<DemoTic>,
    /// Tics since start.
    pub tic: u32,
}

impl DemoRecorder {
    /// Begin recording.
    #[must_use]
    pub fn start(map: impl Into<String>, rng_seed: u32) -> Self {
        Self::start_with_complevel(map, rng_seed, Complevel::Default)
    }

    /// Begin recording with an explicit complevel stamp.
    #[must_use]
    pub fn start_with_complevel(
        map: impl Into<String>,
        rng_seed: u32,
        complevel: Complevel,
    ) -> Self {
        Self {
            map: map.into(),
            skill: 3,
            rng_seed,
            complevel,
            player_count: 1,
            consoleplayer: 0,
            tics: Vec::new(),
            tic: 0,
        }
    }

    /// Append one tic after `world.tick()`.
    pub fn record_tic(&mut self, world: &World, cmds: &[TicCmd]) {
        self.tic = self.tic.saturating_add(1);
        let checksum = if self.tic % CSUM_INTERVAL == 0 {
            Some(world.checksum())
        } else {
            None
        };
        self.tics.push(DemoTic {
            cmds: cmds.to_vec(),
            checksum,
        });
    }

    /// Serialize to `.fdem` bytes (`FDHD` + `BODY` + `CSUM`).
    #[must_use]
    pub fn to_fdem(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_chunk(&mut out, b"FDHD", &encode_fdhd(self));
        write_chunk(&mut out, b"BODY", &encode_body(&self.tics));
        write_chunk(&mut out, b"CSUM", &encode_csum(&self.tics));
        out
    }

    /// Parse a `.fdem` blob into an in-memory recorder.
    pub fn from_fdem(bytes: &[u8]) -> Result<Self, DemoError> {
        let mut map = String::new();
        let mut skill = 3u8;
        let mut rng_seed = 0u32;
        let mut complevel = Complevel::Default;
        let mut player_count = 1u8;
        let mut consoleplayer = 0u8;
        let mut tics: Option<Vec<DemoTic>> = None;
        let mut csums: Vec<(u32, u64)> = Vec::new();

        let mut cursor = 0usize;
        while cursor < bytes.len() {
            if bytes.len() - cursor < 8 {
                return Err(DemoError::Message("truncated chunk header".into()));
            }
            let id = &bytes[cursor..cursor + 4];
            let len = read_u32(bytes, cursor + 4)? as usize;
            cursor += 8;
            if bytes.len() - cursor < len {
                return Err(DemoError::Message("truncated chunk payload".into()));
            }
            let payload = &bytes[cursor..cursor + len];
            cursor += len;
            match id {
                b"FDHD" => {
                    let h = decode_fdhd(payload)?;
                    map = h.map;
                    skill = h.skill;
                    rng_seed = h.rng_seed;
                    complevel = h.complevel;
                    player_count = h.player_count;
                    consoleplayer = h.consoleplayer;
                }
                b"BODY" => {
                    tics = Some(decode_body(payload)?);
                }
                b"CSUM" => {
                    csums = decode_csum(payload)?;
                }
                b"LOAD" | b"CVAR" | b"SAVE" => {
                    // Optional chunks ignored in M5 bootstrap.
                }
                _ => {
                    // Unknown chunk: skip (forward compatible).
                }
            }
        }

        let mut tics = tics.ok_or_else(|| DemoError::Message("missing BODY chunk".into()))?;
        for (tic_num, cs) in csums {
            let idx = tic_num.saturating_sub(1) as usize;
            if let Some(t) = tics.get_mut(idx) {
                t.checksum = Some(cs);
            }
        }
        let tic = tics.len() as u32;
        Ok(Self {
            map,
            skill,
            rng_seed,
            complevel,
            player_count,
            consoleplayer,
            tics,
            tic,
        })
    }

    /// Stable fingerprint of the recording (for CI golden demos).
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.map.as_bytes());
        bytes.extend_from_slice(&self.rng_seed.to_le_bytes());
        bytes.extend_from_slice(&(self.tics.len() as u32).to_le_bytes());
        for t in &self.tics {
            bytes.extend_from_slice(&(t.cmds.len() as u32).to_le_bytes());
            for c in &t.cmds {
                bytes.extend_from_slice(&c.buttons.0.to_le_bytes());
                bytes.extend_from_slice(&c.yaw.to_le_bytes());
                bytes.extend_from_slice(&c.pitch.to_le_bytes());
                bytes.extend_from_slice(&c.forward_move.to_le_bytes());
                bytes.extend_from_slice(&c.side_move.to_le_bytes());
            }
            if let Some(cs) = t.checksum {
                bytes.extend_from_slice(&cs.to_le_bytes());
            }
        }
        xxh3_64(&bytes)
    }
}

/// Multi-map demo pack (`.fdbn` — Room Demo Bundle).
///
/// Length-prefixed `.fdem` segments in play order (MAP01, MAP02, …).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemoBundle {
    /// Per-map recordings in order.
    pub demos: Vec<DemoRecorder>,
}

/// `.fdbn` magic / version.
pub const FDBN_MAGIC: &[u8; 4] = b"FDBN";
/// Bundle format version.
pub const FDBN_FORMAT_VERSION: u16 = 1;

impl DemoBundle {
    /// Empty pack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a finished map recording.
    pub fn push(&mut self, demo: DemoRecorder) {
        self.demos.push(demo);
    }

    /// Serialize to `.fdbn` bytes.
    #[must_use]
    pub fn to_fdbn(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(FDBN_MAGIC);
        out.extend_from_slice(&FDBN_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.demos.len() as u32).to_le_bytes());
        for d in &self.demos {
            let blob = d.to_fdem();
            out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
            out.extend_from_slice(&blob);
        }
        out
    }

    /// Parse a `.fdbn` blob.
    pub fn from_fdbn(bytes: &[u8]) -> Result<Self, DemoError> {
        if bytes.len() < 4 + 2 + 4 {
            return Err(DemoError::Message("FDBN too short".into()));
        }
        if &bytes[0..4] != FDBN_MAGIC {
            return Err(DemoError::Message("not an FDBN bundle".into()));
        }
        let ver = u16::from_le_bytes([bytes[4], bytes[5]]);
        if ver != FDBN_FORMAT_VERSION {
            return Err(DemoError::Message(format!(
                "unsupported FDBN version {ver}"
            )));
        }
        let count = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        let mut o = 10usize;
        let mut demos = Vec::with_capacity(count);
        for _ in 0..count {
            if bytes.len() < o + 4 {
                return Err(DemoError::Message("FDBN truncated length".into()));
            }
            let len =
                u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as usize;
            o += 4;
            if bytes.len() < o + len {
                return Err(DemoError::Message("FDBN truncated segment".into()));
            }
            demos.push(DemoRecorder::from_fdem(&bytes[o..o + len])?);
            o += len;
        }
        Ok(Self { demos })
    }

    /// Bundle fingerprint (ordered segment fingerprints).
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::new();
        for d in &self.demos {
            bytes.extend_from_slice(&d.fingerprint().to_le_bytes());
        }
        xxh3_64(&bytes)
    }
}

struct Fdhd {
    map: String,
    skill: u8,
    rng_seed: u32,
    player_count: u8,
    consoleplayer: u8,
    complevel: Complevel,
}

fn write_chunk(out: &mut Vec<u8>, id: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
}

fn read_u16(buf: &[u8], o: usize) -> Result<u16, DemoError> {
    let b: [u8; 2] = buf
        .get(o..o + 2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| DemoError::Message("truncated u16".into()))?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(buf: &[u8], o: usize) -> Result<u32, DemoError> {
    let b: [u8; 4] = buf
        .get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| DemoError::Message("truncated u32".into()))?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(buf: &[u8], o: usize) -> Result<u64, DemoError> {
    let b: [u8; 8] = buf
        .get(o..o + 8)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| DemoError::Message("truncated u64".into()))?;
    Ok(u64::from_le_bytes(b))
}

fn read_i16(buf: &[u8], o: usize) -> Result<i16, DemoError> {
    let b: [u8; 2] = buf
        .get(o..o + 2)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| DemoError::Message("truncated i16".into()))?;
    Ok(i16::from_le_bytes(b))
}

fn encode_fdhd(rec: &DemoRecorder) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&FDEM_FORMAT_VERSION.to_le_bytes());
    p.extend_from_slice(&FDEM_ENGINE_VERSION.to_le_bytes());
    p.extend_from_slice(&FDEM_ABI_VERSION.to_le_bytes());
    p.push(rec.skill);
    p.push(rec.player_count);
    p.push(rec.consoleplayer);
    p.extend_from_slice(&rec.rng_seed.to_le_bytes());
    let map_bytes = rec.map.as_bytes();
    let map_len = u16::try_from(map_bytes.len()).unwrap_or(u16::MAX);
    p.extend_from_slice(&map_len.to_le_bytes());
    p.extend_from_slice(&map_bytes[..map_len as usize]);
    // Optional trailing byte (absent in pre-M6 demos → Default).
    p.push(complevel_to_byte(rec.complevel));
    p
}

fn complevel_to_byte(c: Complevel) -> u8 {
    match c {
        Complevel::Default => 0,
        Complevel::Vanilla => 1,
        Complevel::Boom => 2,
        Complevel::Mbf => 3,
        Complevel::Mbf21 => 4,
    }
}

fn complevel_from_byte(b: u8) -> Complevel {
    match b {
        1 => Complevel::Vanilla,
        2 => Complevel::Boom,
        3 => Complevel::Mbf,
        4 => Complevel::Mbf21,
        _ => Complevel::Default,
    }
}

fn decode_fdhd(payload: &[u8]) -> Result<Fdhd, DemoError> {
    if payload.len() < 2 + 2 + 2 + 1 + 1 + 1 + 4 + 2 {
        return Err(DemoError::Message("FDHD too short".into()));
    }
    let mut o = 0usize;
    let format = read_u16(payload, o)?;
    o += 2;
    if format != FDEM_FORMAT_VERSION {
        return Err(DemoError::Message(format!(
            "unsupported .fdem format {format}"
        )));
    }
    o += 2; // engine
    o += 2; // abi
    let skill = payload[o];
    o += 1;
    let player_count = payload[o];
    o += 1;
    let consoleplayer = payload[o];
    o += 1;
    let rng_seed = read_u32(payload, o)?;
    o += 4;
    let map_len = read_u16(payload, o)? as usize;
    o += 2;
    if payload.len() < o + map_len {
        return Err(DemoError::Message("FDHD map truncated".into()));
    }
    let map = std::str::from_utf8(&payload[o..o + map_len])
        .map_err(|_| DemoError::Message("FDHD map not utf-8".into()))?
        .to_owned();
    o += map_len;
    let complevel = if o < payload.len() {
        complevel_from_byte(payload[o])
    } else {
        Complevel::Default
    };
    Ok(Fdhd {
        map,
        skill,
        rng_seed,
        player_count,
        consoleplayer,
        complevel,
    })
}

fn encode_ticcmd(c: &TicCmd) -> [u8; 17] {
    let mut b = [0u8; 17];
    b[0..4].copy_from_slice(&c.buttons.0.to_le_bytes());
    b[4..6].copy_from_slice(&c.pitch.to_le_bytes());
    b[6..8].copy_from_slice(&c.yaw.to_le_bytes());
    b[8..10].copy_from_slice(&c.roll.to_le_bytes());
    b[10..12].copy_from_slice(&c.forward_move.to_le_bytes());
    b[12..14].copy_from_slice(&c.side_move.to_le_bytes());
    b[14..16].copy_from_slice(&c.up_move.to_le_bytes());
    b[16] = c.weapon_select;
    b
}

fn decode_ticcmd(bytes: &[u8]) -> Result<TicCmd, DemoError> {
    if bytes.len() < 17 {
        return Err(DemoError::Message("TicCmd truncated".into()));
    }
    Ok(TicCmd {
        buttons: Buttons(read_u32(bytes, 0)?),
        pitch: read_i16(bytes, 4)?,
        yaw: read_i16(bytes, 6)?,
        roll: read_i16(bytes, 8)?,
        forward_move: read_i16(bytes, 10)?,
        side_move: read_i16(bytes, 12)?,
        up_move: read_i16(bytes, 14)?,
        weapon_select: bytes[16],
    })
}

fn encode_body(tics: &[DemoTic]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&(tics.len() as u32).to_le_bytes());
    for t in tics {
        p.push(t.cmds.len() as u8);
        for c in &t.cmds {
            p.extend_from_slice(&encode_ticcmd(c));
        }
    }
    p
}

fn decode_body(payload: &[u8]) -> Result<Vec<DemoTic>, DemoError> {
    if payload.len() < 4 {
        return Err(DemoError::Message("BODY too short".into()));
    }
    let count = read_u32(payload, 0)? as usize;
    let mut o = 4usize;
    let mut tics = Vec::with_capacity(count);
    for _ in 0..count {
        if o >= payload.len() {
            return Err(DemoError::Message("BODY truncated".into()));
        }
        let n = payload[o] as usize;
        o += 1;
        let mut cmds = Vec::with_capacity(n);
        for _ in 0..n {
            if payload.len() < o + 17 {
                return Err(DemoError::Message("BODY TicCmd truncated".into()));
            }
            cmds.push(decode_ticcmd(&payload[o..o + 17])?);
            o += 17;
        }
        tics.push(DemoTic {
            cmds,
            checksum: None,
        });
    }
    Ok(tics)
}

fn encode_csum(tics: &[DemoTic]) -> Vec<u8> {
    let mut p = Vec::new();
    let mut n = 0u32;
    for t in tics {
        if t.checksum.is_some() {
            n += 1;
        }
    }
    p.extend_from_slice(&n.to_le_bytes());
    for (i, t) in tics.iter().enumerate() {
        if let Some(cs) = t.checksum {
            let tic_num = (i as u32).saturating_add(1);
            p.extend_from_slice(&tic_num.to_le_bytes());
            p.extend_from_slice(&cs.to_le_bytes());
        }
    }
    p
}

fn decode_csum(payload: &[u8]) -> Result<Vec<(u32, u64)>, DemoError> {
    if payload.len() < 4 {
        return Err(DemoError::Message("CSUM too short".into()));
    }
    let n = read_u32(payload, 0)? as usize;
    let mut o = 4usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if payload.len() < o + 12 {
            return Err(DemoError::Message("CSUM truncated".into()));
        }
        let tic = read_u32(payload, o)?;
        let cs = read_u64(payload, o + 4)?;
        o += 12;
        out.push((tic, cs));
    }
    Ok(out)
}

/// Result of verifying a demo against a live world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoVerifyResult {
    /// Tics successfully replayed.
    pub tics_ok: u32,
    /// First failing tic (1-based), if any.
    pub first_mismatch: Option<u32>,
}

/// Replay recorded cmds into a world and compare checksums.
pub struct DemoPlayer<'a> {
    /// Recording.
    pub demo: &'a DemoRecorder,
}

impl<'a> DemoPlayer<'a> {
    /// Verify by replaying into `world` (caller supplies a fresh world matching demo start).
    pub fn verify(&self, world: &mut World) -> Result<DemoVerifyResult, DemoError> {
        crate::headless::apply_complevel_to_world(world, self.demo.complevel);
        let mut tics_ok = 0u32;
        for (i, tic) in self.demo.tics.iter().enumerate() {
            world.set_ticcmds(&tic.cmds);
            world.tick();
            tics_ok = tics_ok.saturating_add(1);
            if let Some(expected) = tic.checksum {
                let got = world.checksum();
                if got != expected {
                    return Ok(DemoVerifyResult {
                        tics_ok: tics_ok.saturating_sub(1),
                        first_mismatch: Some((i as u32).saturating_add(1)),
                    });
                }
            }
        }
        Ok(DemoVerifyResult {
            tics_ok,
            first_mismatch: None,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_sim::World;
    use room_testkit::{MapBuilder, rect};

    fn room_world() -> World {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        World::from_map(map).unwrap()
    }

    #[test]
    fn record_and_verify_checksums() {
        let mut world = room_world();
        let mut rec = DemoRecorder::start("MAP01", world.rng.seed());
        let cmd = TicCmd::default();
        for _ in 0..CSUM_INTERVAL {
            world.set_ticcmds(&[cmd]);
            world.tick();
            rec.record_tic(&world, &[cmd]);
        }
        assert!(rec.tics.last().unwrap().checksum.is_some());

        let mut world2 = room_world();
        let player = DemoPlayer { demo: &rec };
        let result = player.verify(&mut world2).unwrap();
        assert_eq!(result.first_mismatch, None);
        assert_eq!(result.tics_ok, CSUM_INTERVAL);
        assert_ne!(rec.fingerprint(), 0);
    }

    #[test]
    fn fdhd_roundtrips_complevel_and_reads_legacy() {
        let mut rec = DemoRecorder::start_with_complevel("MAP01", 1, Complevel::Mbf21);
        rec.tics.push(DemoTic {
            cmds: vec![TicCmd::default()],
            checksum: None,
        });
        let bytes = rec.to_fdem();
        let loaded = DemoRecorder::from_fdem(&bytes).unwrap();
        assert_eq!(loaded.complevel, Complevel::Mbf21);

        // Legacy FDHD without trailing complevel byte → Default.
        let mut legacy = DemoRecorder::start("MAP02", 2);
        legacy.tics.push(DemoTic {
            cmds: vec![TicCmd::default()],
            checksum: None,
        });
        let mut raw = legacy.to_fdem();
        // Shrink FDHD payload by 1 (strip complevel) while keeping BODY/CSUM.
        assert_eq!(&raw[0..4], b"FDHD");
        let fdhd_len = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
        assert!(fdhd_len >= 1);
        let new_len = (fdhd_len - 1) as u32;
        raw[4..8].copy_from_slice(&new_len.to_le_bytes());
        raw.remove(8 + fdhd_len - 1);
        let legacy_loaded = DemoRecorder::from_fdem(&raw).unwrap();
        assert_eq!(legacy_loaded.complevel, Complevel::Default);
        assert_eq!(legacy_loaded.map, "MAP02");
    }

    #[test]
    fn committed_map01_smoke_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom2_map01_smoke.fdem");
        let demo = DemoRecorder::from_fdem(bytes).unwrap();
        assert_eq!(demo.map, "MAP01");
        assert_eq!(demo.rng_seed, 1993);
        assert_eq!(demo.tics.len(), 70);
        assert_eq!(demo.fingerprint(), 0xdf46a6298242988a);
    }

    #[test]
    fn committed_map01_03_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom2_map01_03.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 3);
        assert_eq!(bundle.fingerprint(), 0xdef8854f53385efe);
    }

    #[test]
    fn committed_map01_10_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom2_map01_10.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 10);
        assert_eq!(bundle.fingerprint(), 0xcad47cee5e1f509c);
    }

    #[test]
    fn committed_map11_20_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom2_map11_20.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 10);
        assert_eq!(bundle.fingerprint(), 0xa8ec8aba55eec15f);
    }

    #[test]
    fn committed_map21_30_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom2_map21_30.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 10);
        assert_eq!(bundle.fingerprint(), 0xc83574ab80b936e0);
    }

    #[test]
    fn committed_freedoom1_e1_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom1_e1.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 8);
        assert_eq!(bundle.fingerprint(), 0x408629dd6923c794);
    }

    #[test]
    fn committed_freedoom1_e2_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom1_e2.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 8);
        assert_eq!(bundle.fingerprint(), 0x3fd7981567385e63);
    }

    #[test]
    fn committed_freedoom1_e3_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom1_e3.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 8);
        assert_eq!(bundle.fingerprint(), 0x19b2465eba4c6b7d);
    }

    #[test]
    fn committed_freedoom1_e4_bundle_fingerprint_stable() {
        let bytes = include_bytes!("../../../testdata/demos/freedoom1_e4.fdbn");
        let bundle = DemoBundle::from_fdbn(bytes).unwrap();
        assert_eq!(bundle.demos.len(), 8);
        assert_eq!(bundle.fingerprint(), 0x90eec315e9fb1e2d);
    }

    #[test]
    fn fdbn_roundtrip_preserves_segments() {
        let mut a = DemoRecorder::start("MAP01", 1);
        a.tics.push(DemoTic {
            cmds: vec![TicCmd::default()],
            checksum: Some(1),
        });
        let mut b = DemoRecorder::start("MAP02", 1);
        b.tics.push(DemoTic {
            cmds: vec![TicCmd {
                forward_move: 100,
                ..TicCmd::default()
            }],
            checksum: None,
        });
        let mut pack = DemoBundle::new();
        pack.push(a.clone());
        pack.push(b.clone());
        let bytes = pack.to_fdbn();
        let got = DemoBundle::from_fdbn(&bytes).unwrap();
        assert_eq!(got.demos.len(), 2);
        assert_eq!(got.demos[0].map, "MAP01");
        assert_eq!(got.demos[1].map, "MAP02");
        assert_eq!(got.demos[1].tics[0].cmds[0].forward_move, 100);
        assert_eq!(got.fingerprint(), pack.fingerprint());
    }

    #[test]
    fn fdem_roundtrip_preserves_cmds_and_checksums() {
        let mut world = room_world();
        let mut rec = DemoRecorder::start("MAP01", world.rng.seed());
        rec.skill = 2;
        let cmd = TicCmd {
            forward_move: 2048,
            weapon_select: 2,
            ..Default::default()
        };
        for _ in 0..CSUM_INTERVAL {
            world.set_ticcmds(&[cmd]);
            world.tick();
            rec.record_tic(&world, &[cmd]);
        }
        let bytes = rec.to_fdem();
        assert!(bytes.windows(4).any(|w| w == b"FDHD"));
        assert!(bytes.windows(4).any(|w| w == b"BODY"));
        assert!(bytes.windows(4).any(|w| w == b"CSUM"));
        let loaded = DemoRecorder::from_fdem(&bytes).unwrap();
        assert_eq!(loaded.map, "MAP01");
        assert_eq!(loaded.skill, 2);
        assert_eq!(loaded.rng_seed, rec.rng_seed);
        assert_eq!(loaded.tics.len(), rec.tics.len());
        assert_eq!(
            loaded.tics.last().unwrap().checksum,
            rec.tics.last().unwrap().checksum
        );
        assert_eq!(loaded.tics[0].cmds[0].forward_move, 2048);
        assert_eq!(loaded.tics[0].cmds[0].weapon_select, 2);
        assert_eq!(loaded.fingerprint(), rec.fingerprint());

        let mut world2 = room_world();
        let result = DemoPlayer { demo: &loaded }.verify(&mut world2).unwrap();
        assert_eq!(result.first_mismatch, None);
    }
}

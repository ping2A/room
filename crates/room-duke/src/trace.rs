//! Per-tic Duke demo sync traces (`docs/duke-trace-format.md`).

use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;

/// Format magic / version line.
pub const HEADER: &str = "DUKE_TRACE 1";

/// Atomic Edition weapon slots (`MAX_WEAPONS`).
pub const MAX_WEAPONS: usize = 12;

/// One player seat snapshot.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TracePlayer {
    /// Build x.
    pub posx: i32,
    /// Build y.
    pub posy: i32,
    /// Build z.
    pub posz: i32,
    /// Angle 0..2047.
    pub ang: i16,
    /// Look pitch.
    pub horiz: i16,
    /// Health (`extra`).
    pub extra: i16,
    /// Current weapon index.
    pub curr_weapon: i16,
    /// Ammo per weapon (`MAX_WEAPONS`).
    pub ammo: Vec<i16>,
}

/// One tic of a Duke sync trace.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraceTic {
    /// Tic index from demo start.
    pub tic: u32,
    /// Build `randomseed` after the tic.
    pub randomseed: i32,
    /// Duke `global_random` after the tic.
    pub global_random: i32,
    /// Live sprite count.
    pub numsprites: u32,
    /// Per-seat player state.
    pub players: Vec<TracePlayer>,
    /// SHA-256 hex of live sprite fields (see format doc).
    pub sprite_hash: String,
}

/// Parsed trace file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceFile {
    /// `meta key=value` pairs (order preserved).
    pub meta: Vec<(String, String)>,
    /// Tic rows in order.
    pub tics: Vec<TraceTic>,
}

/// First mismatch between two traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceDiff {
    /// Tic index (or the shorter length when `field == "length"`).
    pub tic: u32,
    /// Field path (`players[0].posx`, `sprite_hash`, `length`, …).
    pub field: String,
    /// Value from file A.
    pub a: String,
    /// Value from file B.
    pub b: String,
}

impl fmt::Display for TraceDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"tic\":{},\"field\":{},\"a\":{},\"b\":{}}}",
            self.tic,
            json_str(&self.field),
            json_str(&self.a),
            json_str(&self.b)
        )
    }
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

/// SHA-256 hex of live sprite records (little-endian concat).
#[must_use]
pub fn hash_sprites<'a, I>(sprites: I) -> String
where
    I: IntoIterator<Item = (i32, i32, i32, i16, i16)>,
{
    let mut h = Sha256::new();
    for (x, y, z, picnum, statnum) in sprites {
        h.update(x.to_le_bytes());
        h.update(y.to_le_bytes());
        h.update(z.to_le_bytes());
        h.update(picnum.to_le_bytes());
        h.update(statnum.to_le_bytes());
    }
    hex_lower(&h.finalize())
}

/// SHA-256 hex of arbitrary bytes (manifest helpers).
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

pub(crate) fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    let b = bytes.as_ref();
    let mut out = String::with_capacity(b.len() * 2);
    for byte in b {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Parse a full trace document from text.
pub fn parse_trace(text: &str) -> Result<TraceFile, String> {
    let mut meta = Vec::new();
    let mut tics = Vec::new();
    let mut saw_header = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == HEADER {
            if saw_header {
                return Err("duplicate DUKE_TRACE header".into());
            }
            saw_header = true;
            continue;
        }
        if !saw_header {
            return Err(format!("expected `{HEADER}` before {line:?}"));
        }
        if let Some(rest) = line.strip_prefix("meta ") {
            let Some((k, v)) = rest.split_once('=') else {
                return Err(format!("meta needs key=value, got {rest:?}"));
            };
            meta.push((k.trim().to_owned(), v.trim().to_owned()));
            continue;
        }
        let tic: TraceTic =
            serde_json::from_str(line).map_err(|e| format!("tic json: {e} ({line})"))?;
        tics.push(tic);
    }
    if !saw_header {
        return Err(format!("missing `{HEADER}` header"));
    }
    Ok(TraceFile { meta, tics })
}

/// Load and parse a trace file from disk.
pub fn load_trace(path: &Path) -> Result<TraceFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_trace(&text)
}

/// Diff two traces; `None` if equal.
#[must_use]
pub fn diff_traces(a: &TraceFile, b: &TraceFile) -> Option<TraceDiff> {
    if a.tics.len() != b.tics.len() {
        return Some(TraceDiff {
            tic: a.tics.len().min(b.tics.len()) as u32,
            field: "length".into(),
            a: a.tics.len().to_string(),
            b: b.tics.len().to_string(),
        });
    }
    for (ta, tb) in a.tics.iter().zip(b.tics.iter()) {
        if ta.tic != tb.tic {
            return Some(TraceDiff {
                tic: ta.tic.min(tb.tic),
                field: "tic".into(),
                a: ta.tic.to_string(),
                b: tb.tic.to_string(),
            });
        }
        if ta.randomseed != tb.randomseed {
            return Some(field_diff(
                ta.tic,
                "randomseed",
                &ta.randomseed,
                &tb.randomseed,
            ));
        }
        if ta.global_random != tb.global_random {
            return Some(field_diff(
                ta.tic,
                "global_random",
                &ta.global_random,
                &tb.global_random,
            ));
        }
        if ta.numsprites != tb.numsprites {
            return Some(field_diff(
                ta.tic,
                "numsprites",
                &ta.numsprites,
                &tb.numsprites,
            ));
        }
        if ta.players.len() != tb.players.len() {
            return Some(TraceDiff {
                tic: ta.tic,
                field: "players.length".into(),
                a: ta.players.len().to_string(),
                b: tb.players.len().to_string(),
            });
        }
        // Player pose before sprite_hash so XY/Z drift is not masked by hash.
        for (i, (pa, pb)) in ta.players.iter().zip(tb.players.iter()).enumerate() {
            if pa.posx != pb.posx {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].posx"),
                    &pa.posx,
                    &pb.posx,
                ));
            }
            if pa.posy != pb.posy {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].posy"),
                    &pa.posy,
                    &pb.posy,
                ));
            }
            if pa.posz != pb.posz {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].posz"),
                    &pa.posz,
                    &pb.posz,
                ));
            }
            if pa.ang != pb.ang {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].ang"),
                    &pa.ang,
                    &pb.ang,
                ));
            }
            if pa.horiz != pb.horiz {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].horiz"),
                    &pa.horiz,
                    &pb.horiz,
                ));
            }
            if pa.extra != pb.extra {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].extra"),
                    &pa.extra,
                    &pb.extra,
                ));
            }
            if pa.curr_weapon != pb.curr_weapon {
                return Some(field_diff(
                    ta.tic,
                    &format!("players[{i}].curr_weapon"),
                    &pa.curr_weapon,
                    &pb.curr_weapon,
                ));
            }
            if pa.ammo != pb.ammo {
                return Some(TraceDiff {
                    tic: ta.tic,
                    field: format!("players[{i}].ammo"),
                    a: format!("{:?}", pa.ammo),
                    b: format!("{:?}", pb.ammo),
                });
            }
        }
        if ta.sprite_hash != tb.sprite_hash {
            return Some(TraceDiff {
                tic: ta.tic,
                field: "sprite_hash".into(),
                a: ta.sprite_hash.clone(),
                b: tb.sprite_hash.clone(),
            });
        }
    }
    None
}

fn field_diff<T: ToString>(tic: u32, field: &str, a: &T, b: &T) -> TraceDiff {
    TraceDiff {
        tic,
        field: field.into(),
        a: a.to_string(),
        b: b.to_string(),
    }
}

/// Serialize a trace file to the on-disk format.
#[must_use]
pub fn format_trace(file: &TraceFile) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    for (k, v) in &file.meta {
        out.push_str("meta ");
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    for tic in &file.tics {
        if let Ok(line) = serde_json::to_string(tic) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_player(posx: i32) -> TracePlayer {
        TracePlayer {
            posx,
            posy: 0,
            posz: 0,
            ang: 0,
            horiz: 100,
            extra: 100,
            curr_weapon: 1,
            ammo: vec![48; MAX_WEAPONS],
        }
    }

    fn sample_tic(tic: u32, posx: i32) -> TraceTic {
        TraceTic {
            tic,
            randomseed: tic as i32,
            global_random: tic as i32 + 1,
            numsprites: 0,
            players: vec![sample_player(posx)],
            sprite_hash: hash_sprites(std::iter::empty()),
        }
    }

    #[test]
    fn empty_sprite_hash_is_sha256_of_empty() {
        assert_eq!(
            hash_sprites(std::iter::empty()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn round_trip_and_diff() {
        let file = TraceFile {
            meta: vec![
                ("demo".into(), "DEMO1".into()),
                ("players".into(), "1".into()),
            ],
            tics: vec![sample_tic(0, 0), sample_tic(1, 16)],
        };
        let text = format_trace(&file);
        let parsed = parse_trace(&text).expect("parse");
        assert_eq!(parsed.tics.len(), 2);
        assert!(diff_traces(&file, &parsed).is_none());

        let mut other = file.clone();
        other.tics[1].players[0].posx = 99;
        let d = diff_traces(&file, &other).expect("diff");
        assert_eq!(d.tic, 1);
        assert_eq!(d.field, "players[0].posx");
        assert_eq!(d.a, "16");
        assert_eq!(d.b, "99");
    }

    #[test]
    fn fixture_traces_diverge_on_posx() {
        let a = parse_trace(include_str!("../testdata/demo1-a.trace")).expect("a");
        let b = parse_trace(include_str!("../testdata/demo1-b.trace")).expect("b");
        let d = diff_traces(&a, &b).expect("diff");
        assert_eq!(d.field, "players[0].posx");
        assert_eq!(d.a, "16");
        assert_eq!(d.b, "99");
    }
}

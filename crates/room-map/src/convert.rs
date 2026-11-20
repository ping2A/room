//! Convert an editable Doom- or Hexen-format map to UDMF, so editors can use
//! the specials and fields only UDMF has (line portals, `ext` flags, user
//! fields, explicit line ids) on maps such as the IWAD levels.
//!
//! Lines go through the same Doom → Hexen translation the loader uses
//! ([`crate::xlat`]), so the converted map plays as before. Sector specials
//! are kept as they are: Room reads Doom / Boom sector types in every format.

use crate::geom::MapFormat;
use crate::spac;
use crate::udmf::ext;
use crate::write::RawMap;

/// Doom / Boom / MBF21 linedef flag bits that mean something else in UDMF.
const DOOM_PASSUSE: i16 = 0x200;
const MBF21_BLOCKLANDMONSTERS: i16 = 0x1000;
const MBF21_BLOCKPLAYERS: i16 = 0x2000;
/// Hexen SPAC bits (`flags` 10–12).
const ML_SPAC_MASK: i16 = 0x1c00;
/// UDMF `blockplayers` as Room stores it in `flags`.
const UDMF_BLOCKPLAYERS: i16 = 0x4000;

const LINE_SET_IDENTIFICATION: i16 = 121;

/// Hexen line id carried by a special (UZDoom `SetLineID`), if any.
fn hexen_line_id(special: i16, a: [i32; 5]) -> Option<i32> {
    match special {
        LINE_SET_IDENTIFICATION => Some(a[0] + 256 * a[4]),
        208 | 215 | 222 => Some(a[0]),
        1 => Some(a[3]),
        5 => Some(a[4]),
        181 => Some(a[2]),
        190 if a[1] == 4 => Some(a[0]),
        156 => Some(a[1]),
        _ => None,
    }
    .filter(|&id| id != 0)
}

impl RawMap {
    /// Rewrite the map as UDMF in place. Returns `false` when it already is.
    pub fn convert_to_udmf(&mut self) -> bool {
        let from = self.format;
        if from == MapFormat::Udmf {
            return false;
        }
        for l in &mut self.linedefs {
            match from {
                MapFormat::Doom => {
                    let orig = l.flags;
                    let mut base = l.clone();
                    // Low 9 bits mean the same in both formats.
                    base.flags = orig & 0x1ff;
                    let t = crate::xlat::translate_doom_linedef(&base);
                    l.flags = t.flags & !ML_SPAC_MASK;
                    if orig & MBF21_BLOCKPLAYERS != 0 {
                        l.flags |= UDMF_BLOCKPLAYERS;
                    }
                    if orig & DOOM_PASSUSE != 0 {
                        l.ext |= ext::LINE_PASSUSE;
                    }
                    if orig & MBF21_BLOCKLANDMONSTERS != 0 {
                        l.ext |= ext::LINE_BLOCKLANDMONSTERS;
                    }
                    l.special = t.special;
                    l.args = t.args;
                    l.activation = t.activation;
                }
                MapFormat::Hexen => {
                    let mut activation = spac::from_hexen_flags(l.flags);
                    if activation & spac::USETHROUGH != 0 {
                        activation = (activation & !spac::USETHROUGH) | spac::USE;
                        l.ext |= ext::LINE_PASSUSE;
                    }
                    if l.special == 0 {
                        activation = 0;
                    }
                    l.flags &= !ML_SPAC_MASK;
                    if let Some(id) = hexen_line_id(l.special, l.args) {
                        l.tag = i16::try_from(id).unwrap_or(0);
                    }
                    if l.special == LINE_SET_IDENTIFICATION {
                        // Its only job was the id (and a few flags UDMF has as keys).
                        let more = l.args[1];
                        if more & 1 != 0 {
                            l.ext |= ext::LINE_ZONEBOUNDARY;
                        }
                        l.clip_midtex |= more & 2 != 0;
                        l.wrap_midtex |= more & 4 != 0;
                        l.midtex3d |= more & 8 != 0;
                        l.check_switch_range |= more & 16 != 0;
                        l.first_side_only |= more & 32 != 0;
                        l.special = 0;
                        l.args = [0; 5];
                    }
                    l.activation = activation;
                }
                MapFormat::Udmf => {}
            }
            l.hexen = true;
        }
        for t in &mut self.things {
            let f = t.flags;
            match from {
                MapFormat::Doom => {
                    // Doom bits: 1/2/4 skills, 8 ambush, 16 not single,
                    // Boom 32 not DM, 64 not co-op, MBF 128 friend.
                    t.flags = f & (1 | 2 | 4 | 8 | 16 | 128);
                    if f & 32 != 0 {
                        t.ext |= ext::THING_NOTDM;
                    }
                    if f & 64 != 0 {
                        t.ext |= ext::THING_NOTCOOP;
                    }
                }
                MapFormat::Hexen => {
                    // Hexen bits: 1/2/4 skills, 8 ambush, 16 dormant,
                    // 32/64/128 classes, 256 single, 512 co-op, 1024 DM.
                    let mut n = f & (1 | 2 | 4 | 8);
                    if f & 16 != 0 {
                        n |= 32;
                    }
                    if f & 256 == 0 {
                        n |= 16;
                    }
                    t.flags = n;
                    if f & 512 == 0 {
                        t.ext |= ext::THING_NOTCOOP;
                    }
                    if f & 1024 == 0 {
                        t.ext |= ext::THING_NOTDM;
                    }
                }
                MapFormat::Udmf => {}
            }
            t.hexen = true;
        }
        self.format = MapFormat::Udmf;
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};

    /// "Monsters can activate" (translator, Hexen and UDMF `monsteractivate`).
    const ML_MONSTERS: i16 = 0x2000;

    fn doom_room() -> RawMap {
        let mut map = RawMap::starter_room("MAP01", MapFormat::Doom);
        // DR door on line 0, a WR teleport with tag 3 on line 1, Boom passuse.
        map.linedefs[0].special = 1;
        map.linedefs[1].special = 97;
        map.linedefs[1].tag = 3;
        map.linedefs[2].flags |= 0x200;
        map.things[0].flags = 7 | 32;
        map
    }

    #[test]
    fn doom_map_converts_and_keeps_its_specials() {
        let mut map = doom_room();
        let before = map.to_geometry().unwrap();
        assert!(map.convert_to_udmf());
        assert!(!map.convert_to_udmf());
        assert_eq!(map.format, MapFormat::Udmf);
        let after = map.to_geometry().unwrap();
        for (a, b) in before.lines.iter().zip(&after.lines) {
            assert_eq!(a.special, b.special);
            assert_eq!(a.args, b.args);
            assert_eq!(a.activation, b.activation, "line activation");
            assert_eq!(
                a.flags & (0x1ff | ML_MONSTERS),
                b.flags & (0x1ff | ML_MONSTERS)
            );
        }
        assert_eq!(map.linedefs[2].ext, ext::LINE_PASSUSE);
        assert_eq!(map.things[0].flags, 7);
        assert_eq!(map.things[0].ext, ext::THING_NOTDM);
        // And it survives a UDMF write / read.
        let text = crate::write::write_textmap(&map);
        let u = crate::udmf::parse_udmf(&text).unwrap();
        assert_eq!(u.linedefs[1].special, map.linedefs[1].special);
        assert_eq!(u.linedefs[1].activation, map.linedefs[1].activation);
        assert_eq!(u.linedefs[2].ext, ext::LINE_PASSUSE);
        assert_eq!(u.things[0].ext, ext::THING_NOTDM);
    }

    #[test]
    fn hexen_map_converts_line_ids_and_thing_modes() {
        let v = |x, y| RawVertex { x, y };
        let line = |v1, v2, side| RawLinedef {
            v1,
            v2,
            side_front: side,
            flags: 1,
            hexen: true,
            ..RawLinedef::default()
        };
        let mut map = RawMap {
            name: "MAP01".into(),
            format: MapFormat::Hexen,
            vertices: vec![v(0, 0), v(0, 128), v(128, 128), v(128, 0)],
            linedefs: vec![line(0, 1, 0), line(1, 2, 1), line(2, 3, 2), line(3, 0, 3)],
            sidedefs: (0..4).map(|_| RawSidedef::default()).collect(),
            sectors: vec![RawSector::default()],
            things: vec![RawThing {
                x: 64,
                y: 64,
                type_id: 1,
                flags: 7 | 16 | 256 | 512,
                hexen: true,
                ..RawThing::default()
            }],
            ..RawMap::default()
        };
        // Line_SetIdentification(7) with zone boundary; a use-repeatable
        // Door_Open (SPAC use = 1 << 10) monsters may use.
        map.linedefs[0].special = 121;
        map.linedefs[0].args = [7, 1, 0, 0, 0];
        map.linedefs[1].special = 11;
        map.linedefs[1].args = [5, 16, 0, 0, 0];
        map.linedefs[1].flags |= 0x200 | (1 << 10) | 0x2000;
        assert!(map.convert_to_udmf());
        let l0 = &map.linedefs[0];
        assert_eq!((l0.tag, l0.special, l0.ext), (7, 0, ext::LINE_ZONEBOUNDARY));
        let l1 = &map.linedefs[1];
        assert_eq!(l1.activation, spac::USE);
        assert_eq!(
            l1.flags & (0x200 | ML_MONSTERS),
            0x200 | ML_MONSTERS,
            "repeat, monsters kept"
        );
        assert_eq!(l1.flags & 0x1c00, 0);
        let t = &map.things[0];
        assert_eq!(t.flags, 7 | 32);
        assert_eq!(t.ext, ext::THING_NOTDM);
        assert!(map.to_geometry().is_ok());
    }
}

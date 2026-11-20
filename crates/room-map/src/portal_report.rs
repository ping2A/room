//! Editor-facing summary of a map's portals: which lines and sector planes
//! are portals, where they lead, linked portal groups, polyobjects and the
//! setup warnings (what the web editor overlays on its 2D view).

use crate::geom::MapGeometry;
use crate::portals::{
    PLANEF_BLOCKSOUND, PLANEF_DISABLED, PLANEF_LINKED, PLANEF_NOPASS, PLANEF_NORENDER,
    PORT_KIND_HORIZON, PORT_KIND_MIRROR, PORTS_HORIZON, PORTS_LINKEDPORTAL, PORTS_PLANE,
    PORTS_PORTAL, PORTS_SKYVIEWPOINT, PORTS_STACKEDSECTORTHING, PORTT_INTERACTIVE, PORTT_LINKED,
    PORTT_LINKEDEE, PORTT_TELEPORT, PORTT_VISUAL,
};
use serde::Serialize;

/// One line portal.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct LinePortalInfo {
    /// Origin linedef.
    pub line: u32,
    /// Destination linedef.
    pub target: Option<u32>,
    /// `visual`, `teleport`, `interactive`, `linked`, `horizon`, `mirror`.
    pub kind: &'static str,
    /// Drawn as a view of the destination.
    pub visible: bool,
    /// Actors can cross.
    pub passable: bool,
    /// Seamless crossing with collision on both sides.
    pub interactive: bool,
    /// Sound passes.
    pub sound: bool,
    /// `absolute`, `floor`, `ceiling` (`Line_SetPortal` arg 3).
    pub align: &'static str,
}

/// A sector plane drawn through a portal.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PlanePortalInfo {
    /// Sector.
    pub sector: u32,
    /// `floor` or `ceiling`.
    pub plane: &'static str,
    /// Index into the portal table.
    pub portal: u32,
    /// `skybox`, `stacked`, `portal`, `linked`, `plane`, `horizon`.
    pub kind: &'static str,
    /// Sector the portal leads to.
    pub destination: Option<u32>,
    /// Sky viewpoint / stack thing.
    pub thing: Option<usize>,
    /// Plane opacity over the portal (1 = opaque).
    pub alpha: f32,
    /// Working linked portal (actors cross it).
    pub linked: bool,
    /// UDMF `portal_*_disabled`.
    pub disabled: bool,
    /// UDMF `portal_*_norender`.
    pub norender: bool,
    /// UDMF `portal_*_nopass`.
    pub nopass: bool,
    /// UDMF `portal_*_blocksound`.
    pub blocksound: bool,
}

/// One polyobject.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PolyobjInfo {
    /// Poly id.
    pub id: i32,
    /// Its lines.
    pub lines: Vec<u32>,
    /// Lines of it that carry a line portal.
    pub portal_lines: Vec<u32>,
    /// Mirror poly id (0 = none).
    pub mirror: i32,
}

/// Everything the editor shows about portals.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct PortalReport {
    /// Line portals.
    pub lines: Vec<LinePortalInfo>,
    /// Lines showing a sector portal (`Sector_SetPortal` type 5): `(line, portal)`.
    pub line_copies: Vec<(u32, u32)>,
    /// Portal sector planes.
    pub planes: Vec<PlanePortalInfo>,
    /// Linked portal group per sector (empty when the map has none).
    pub groups: Vec<u32>,
    /// Number of linked portal groups (1 = no links).
    pub group_count: usize,
    /// Sky viewpoint / stack things: `(thing, sector)`.
    pub things: Vec<(usize, Option<u32>)>,
    /// Polyobjects.
    pub polyobjects: Vec<PolyobjInfo>,
    /// Setup problems.
    pub warnings: Vec<String>,
}

fn line_kind(kind: u8) -> &'static str {
    match kind {
        PORTT_VISUAL => "visual",
        PORTT_TELEPORT => "teleport",
        PORTT_INTERACTIVE => "interactive",
        PORTT_LINKED | PORTT_LINKEDEE => "linked",
        PORT_KIND_HORIZON => "horizon",
        PORT_KIND_MIRROR => "mirror",
        _ => "other",
    }
}

fn sector_kind(kind: u8) -> &'static str {
    match kind {
        PORTS_SKYVIEWPOINT => "skybox",
        PORTS_STACKEDSECTORTHING => "stacked",
        PORTS_PORTAL => "portal",
        PORTS_LINKEDPORTAL => "linked",
        PORTS_PLANE => "plane",
        PORTS_HORIZON => "horizon",
        _ => "other",
    }
}

/// Summarise the portals of a linked map (`MapGeometry::setup`).
#[must_use]
pub fn portal_report(map: &MapGeometry) -> PortalReport {
    let t = &map.setup.portals;
    let lines = t
        .lines
        .iter()
        .map(|p| LinePortalInfo {
            line: p.line.raw(),
            target: p.target.map(|l| l.raw()),
            kind: line_kind(p.kind),
            visible: p.is_visual(),
            passable: p.is_traversable(),
            interactive: p.is_interactive(),
            sound: p.passes_sound(),
            align: match p.align {
                1 => "floor",
                2 => "ceiling",
                _ => "absolute",
            },
        })
        .collect();
    let line_copies = t
        .line_copy
        .iter()
        .enumerate()
        .filter_map(|(l, p)| p.map(|p| (l as u32, p)))
        .collect();
    let mut planes = Vec::new();
    for (si, idx) in t.sector_planes.iter().enumerate() {
        for (pi, name) in [(0, "floor"), (1, "ceiling")] {
            let portal = idx[pi];
            let flags = t.flags(si, pi);
            // 0 and 1 are the default sky slots.
            if portal < 2 && flags & (PLANEF_DISABLED | PLANEF_NORENDER) == 0 {
                continue;
            }
            let Some(p) = t.sectors.get(portal as usize) else {
                continue;
            };
            planes.push(PlanePortalInfo {
                sector: si as u32,
                plane: name,
                portal,
                kind: if portal < 2 {
                    "sky"
                } else {
                    sector_kind(p.kind)
                },
                destination: p.destination.map(|s| s.raw()),
                thing: p.skybox,
                alpha: t.plane_alpha.get(si).map_or(1.0, |a| a[pi]),
                linked: flags & PLANEF_LINKED != 0,
                disabled: flags & PLANEF_DISABLED != 0,
                norender: flags & PLANEF_NORENDER != 0,
                nopass: flags & PLANEF_NOPASS != 0,
                blocksound: flags & PLANEF_BLOCKSOUND != 0,
            });
        }
    }
    let g = &map.setup.portal_groups;
    let polyobjects = map
        .setup
        .polyobjects
        .iter()
        .map(|po| PolyobjInfo {
            id: po.id,
            lines: po.lines.iter().map(|l| l.raw()).collect(),
            portal_lines: po
                .lines
                .iter()
                .filter(|l| t.line(**l).is_some())
                .map(|l| l.raw())
                .collect(),
            mirror: po.mirror,
        })
        .collect();
    PortalReport {
        lines,
        line_copies,
        planes,
        groups: if g.has_links() {
            g.sector_group.clone()
        } else {
            Vec::new()
        },
        group_count: g.size.max(1),
        things: t
            .things
            .values()
            .map(|th| (th.thing, th.sector.map(|s| s.raw())))
            .collect(),
        polyobjects,
        warnings: map.setup.portal_warnings.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
    use crate::udmf::ext;
    use crate::write::RawMap;

    /// Two rooms side by side; line 1 (east wall of room A) is a
    /// `Line_SetPortal` to line 5 (west wall of room B).
    fn two_rooms() -> RawMap {
        let v = |x, y| RawVertex { x, y };
        let mut vertices = vec![v(0, 0), v(0, 128), v(128, 128), v(128, 0)];
        vertices.extend([v(256, 0), v(256, 128), v(384, 128), v(384, 0)]);
        let line = |v1, v2, side| RawLinedef {
            v1,
            v2,
            side_front: side,
            flags: 1,
            hexen: true,
            ..RawLinedef::default()
        };
        let mut linedefs = vec![
            line(0, 1, 0),
            line(1, 2, 1),
            line(2, 3, 2),
            line(3, 0, 3),
            line(4, 5, 4),
            line(5, 6, 5),
            line(6, 7, 6),
            line(7, 4, 7),
        ];
        // East wall of A: v2→v3 (128,128)→(128,0). West wall of B: v4→v5.
        linedefs[2].special = 156;
        linedefs[2].args = [9, 0, 0, 0, 0];
        linedefs[4].tag = 9;
        let sidedefs = (0..8)
            .map(|i| RawSidedef {
                middle: "STARTAN3".into(),
                sector: u16::from(i >= 4),
                ..RawSidedef::default()
            })
            .collect();
        let sectors = vec![
            RawSector {
                ext: ext::SECTOR_PORTAL_CEIL_DISABLED,
                ..RawSector::default()
            },
            RawSector::default(),
        ];
        RawMap {
            name: "MAP01".into(),
            format: crate::geom::MapFormat::Udmf,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            things: vec![RawThing {
                x: 64,
                y: 64,
                type_id: 1,
                flags: 7,
                ..RawThing::default()
            }],
            ..RawMap::default()
        }
    }

    #[test]
    fn reports_line_portals_and_plane_flags() {
        let geom = two_rooms().to_geometry().unwrap();
        let r = portal_report(&geom);
        assert_eq!(r.lines.len(), 1, "{r:?}");
        assert_eq!(r.lines[0].line, 2);
        assert_eq!(r.lines[0].target, Some(4));
        assert_eq!(r.lines[0].kind, "visual");
        assert!(r.lines[0].visible);
        assert!(!r.lines[0].passable);
        let ceil = r
            .planes
            .iter()
            .find(|p| p.sector == 0 && p.plane == "ceiling")
            .unwrap();
        assert!(ceil.disabled);
        assert_eq!(ceil.kind, "sky");
        assert!(r.groups.is_empty());
        assert_eq!(r.group_count, 1);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"kind\":\"visual\""));
    }
}

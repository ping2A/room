//! Map model, Doom/Hexen/UDMF loaders, validation, blockmap, reject.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod blockmap;
pub mod boom_discrete;
pub mod boom_gen;
pub mod compat_db;
pub mod convert;
pub mod doom;
pub mod error;
pub mod geom;
pub mod level_fixes;
pub mod loader;
pub mod nodes;
pub mod portal_report;
pub mod portals;
pub mod raw;
pub mod reject;
pub mod setup;
pub mod sound_zones;
pub mod spac;
pub mod svg;
pub mod udmf;
pub mod write;
pub mod xlat;

pub use blockmap::{Blockmap, build_blockmap, parse_blockmap};
pub use boom_discrete::translate_boom_discrete;
pub use boom_gen::translate_generalized;
pub use compat_db::{CompatDatabase, format_md5, md5_hex};
pub use doom::{
    parse_linedefs, parse_linedefs_hexen, parse_sectors, parse_sidedefs, parse_things,
    parse_things_hexen, parse_vertexes,
};
pub use error::MapError;
pub use geom::{
    Line, MapFormat, MapGeometry, SectorStatic, Side, Vertex, link_geometry, link_geometry_at,
};
pub use level_fixes::{LevelFixOp, LevelFixesDatabase, apply_level_fixes};
pub use loader::{list_maps, load_map, load_udmf_text};
pub use nodes::{
    ExtendedNodes, GameNodes, GlNodes, GlSeg, NF_SUBSECTOR, Node, Seg, Subsector, gl_nodes_to_game,
    load_extended_nodes_lump, load_game_nodes, parse_nodes, parse_segs, parse_ssectors,
    parse_xnod_body,
};
pub use portal_report::{PortalReport, portal_report};
pub use portals::{
    BuildLink, LinePortal, PORTAL_KIND_FALL, PORTAL_KIND_WATER, PortalGroups, PortalTables,
    PortalThing, SectorPortal,
};
pub use raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
pub use reject::{Reject, build_reject, build_reject_zero, parse_reject};
pub use setup::{
    FFloorStatic, MidtexAttach, PolyobjStatic, SecPlane, SectorPlanes, SetupData,
    apply_attach_3d_midtex, build_setup,
};
pub use svg::map_to_svg;
pub use udmf::{UdmfLexer, UdmfMap, UdmfTok, UdmfUserBag, UdmfUserStore, parse_udmf};
pub use write::{NO_SIDE, RawMap, write_textmap};
pub use xlat::{TranslatedSpecial, translate_doom_linedef};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::{VfsBuilder, WadWriter};

    fn doom_map_wad() -> Vec<u8> {
        let mut vertexes = Vec::new();
        for (x, y) in [(0i16, 0i16), (256, 0), (256, 256), (0, 256)] {
            vertexes.extend_from_slice(&x.to_le_bytes());
            vertexes.extend_from_slice(&y.to_le_bytes());
        }
        let mut linedefs = Vec::new();
        for (v1, v2, side) in [(0u16, 1u16, 0u16), (1, 2, 1), (2, 3, 2), (3, 0, 3)] {
            linedefs.extend_from_slice(&v1.to_le_bytes());
            linedefs.extend_from_slice(&v2.to_le_bytes());
            linedefs.extend_from_slice(&1i16.to_le_bytes());
            linedefs.extend_from_slice(&0i16.to_le_bytes());
            linedefs.extend_from_slice(&0i16.to_le_bytes());
            linedefs.extend_from_slice(&side.to_le_bytes());
            linedefs.extend_from_slice(&0xFFFFu16.to_le_bytes());
        }
        let mut sidedefs = Vec::new();
        for _ in 0..4 {
            sidedefs.extend_from_slice(&0i16.to_le_bytes());
            sidedefs.extend_from_slice(&0i16.to_le_bytes());
            sidedefs.extend_from_slice(b"-\0\0\0\0\0\0\0");
            sidedefs.extend_from_slice(b"-\0\0\0\0\0\0\0");
            sidedefs.extend_from_slice(b"STARTAN2");
            sidedefs.extend_from_slice(&0u16.to_le_bytes());
        }
        let mut sectors = Vec::new();
        sectors.extend_from_slice(&0i16.to_le_bytes());
        sectors.extend_from_slice(&128i16.to_le_bytes());
        sectors.extend_from_slice(b"FLOOR0_1");
        sectors.extend_from_slice(b"CEIL1_1\0");
        sectors.extend_from_slice(&160i16.to_le_bytes());
        sectors.extend_from_slice(&0i16.to_le_bytes());
        sectors.extend_from_slice(&0i16.to_le_bytes());
        let mut things = Vec::new();
        things.extend_from_slice(&128i16.to_le_bytes());
        things.extend_from_slice(&128i16.to_le_bytes());
        things.extend_from_slice(&0i16.to_le_bytes());
        things.extend_from_slice(&1i16.to_le_bytes());
        things.extend_from_slice(&7i16.to_le_bytes());

        WadWriter::pwad()
            .add("MAP01", Vec::new())
            .add("THINGS", things)
            .add("LINEDEFS", linedefs)
            .add("SIDEDEFS", sidedefs)
            .add("VERTEXES", vertexes)
            .add("SEGS", Vec::new())
            .add("SSECTORS", Vec::new())
            .add("NODES", Vec::new())
            .add("SECTORS", sectors)
            .add("REJECT", Vec::new())
            .add("BLOCKMAP", Vec::new())
            .finish()
            .unwrap()
    }

    #[test]
    fn load_minimal_doom_map() {
        let wad = doom_map_wad();
        let vfs = VfsBuilder::new()
            .mount_memory("test.wad", wad)
            .unwrap()
            .build()
            .unwrap();
        let map = load_map(&vfs, "MAP01").unwrap();
        assert_eq!(map.format, MapFormat::Doom);
        assert_eq!(map.vertices.len(), 4);
        assert_eq!(map.lines.len(), 4);
        assert_eq!(map.sides.len(), 4);
        assert_eq!(map.sectors.len(), 1);
        assert_eq!(map.things.len(), 1);
        assert_eq!(map.sectors[0].ceiling_height, 128.0);
        assert!(map.sectors[0].lines.len() >= 4);
        assert!(!map.blockmap.blocks.is_empty());
    }

    #[test]
    fn load_freedoom_map01_if_present() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/freedoom/freedoom2.wad");
        if !path.is_file() {
            eprintln!("skip: freedoom2.wad missing");
            return;
        }
        let vfs = VfsBuilder::new()
            .mount_path(&path)
            .unwrap()
            .build()
            .unwrap();
        let map = load_map(&vfs, "MAP01").unwrap();
        assert!(map.vertices.len() > 10);
        assert!(map.lines.len() > 10);
        assert!(map.sectors.len() > 1);
        assert!(map.game_nodes.is_some());
        assert!(!map.blockmap.blocks.is_empty());
    }
}

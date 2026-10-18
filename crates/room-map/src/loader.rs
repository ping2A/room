//! Locate and load a map from a [`Vfs`].

use crate::blockmap::{build_blockmap, parse_blockmap};
use crate::doom::{
    parse_linedefs, parse_linedefs_hexen, parse_sectors, parse_sidedefs, parse_things,
    parse_things_hexen, parse_vertexes,
};
use crate::error::MapError;
use crate::geom::Vertex;
use crate::geom::{MapFormat, MapGeometry, link_geometry, link_geometry_at};
use crate::nodes::{ExtendedNodes, gl_nodes_to_game, load_extended_nodes_lump, load_game_nodes};
use crate::reject::{build_reject, parse_reject};
use crate::udmf::parse_udmf;
use md5::{Digest, Md5};
use room_vfs::{LumpId, Namespace, Vfs};

/// Load a named map marker from `vfs` (Doom, Hexen, or UDMF).
pub fn load_map(vfs: &Vfs, name: &str) -> Result<MapGeometry, MapError> {
    let marker = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))
        .ok_or_else(|| MapError::NotFound(name.to_owned()))?;

    let lumps = collect_map_lumps(vfs, marker)?;

    if let Some((_, textmap)) = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("TEXTMAP"))
    {
        let text = std::str::from_utf8(textmap)
            .map_err(|_| MapError::Message("TEXTMAP not UTF-8".into()))?;
        let udmf = parse_udmf(text)?;
        let md5 = {
            let mut h = Md5::new();
            h.update(textmap);
            let d = h.finalize();
            let mut out = [0u8; 16];
            out.copy_from_slice(&d);
            out
        };
        let mut geom = link_geometry_at(
            name,
            MapFormat::Udmf,
            udmf.vertex_pos,
            udmf.linedefs,
            udmf.sidedefs,
            udmf.sectors,
            udmf.things,
            md5,
        );
        geom.udmf_user = udmf.user;
        if let Some((_, data)) = lumps
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("BEHAVIOR"))
        {
            geom.behavior = Some(data.clone());
        }
        // UDMF may ship ZNODES (XNOD/XGL*/Z*) instead of classic lumps.
        if let Some((_, data)) = lumps
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("ZNODES") || n.eq_ignore_ascii_case("NODES"))
        {
            if let Some(ext) = load_extended_nodes_lump(data)? {
                apply_extended_nodes(&mut geom, ext);
            }
        }
        return Ok(geom);
    }

    let has_behavior = lumps
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("BEHAVIOR"));
    let format = if has_behavior {
        MapFormat::Hexen
    } else {
        MapFormat::Doom
    };

    let things_data = require_lump(&lumps, "THINGS")?;
    let linedefs_data = require_lump(&lumps, "LINEDEFS")?;
    let sidedefs_data = require_lump(&lumps, "SIDEDEFS")?;
    let vertexes_data = require_lump(&lumps, "VERTEXES")?;
    let sectors_data = require_lump(&lumps, "SECTORS")?;

    let md5 = map_checksum(&lumps, format);

    let (things, linedefs) = if format == MapFormat::Hexen {
        (
            parse_things_hexen(things_data)?,
            parse_linedefs_hexen(linedefs_data)?,
        )
    } else {
        (parse_things(things_data)?, parse_linedefs(linedefs_data)?)
    };
    let sidedefs = parse_sidedefs(sidedefs_data)?;
    let vertexes = parse_vertexes(vertexes_data)?;
    let sectors = parse_sectors(sectors_data)?;

    let mut geom = link_geometry(
        name, format, vertexes, linedefs, sidedefs, sectors, things, md5,
    );

    if let Some((_, data)) = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("BEHAVIOR"))
    {
        geom.behavior = Some(data.clone());
    }

    if let Ok(Some(bm)) = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("BLOCKMAP"))
        .map(|(_, d)| parse_blockmap(d, geom.lines.len()))
        .transpose()
        .map(|o| o.flatten())
    {
        geom.blockmap = bm;
    } else {
        geom.blockmap = build_blockmap(&geom.vertices, &geom.lines);
    }

    let segs = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("SEGS"))
        .map(|(_, d)| d.as_slice())
        .unwrap_or(&[]);
    let ssectors = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("SSECTORS"))
        .map(|(_, d)| d.as_slice())
        .unwrap_or(&[]);
    let nodes = lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("NODES"))
        .map(|(_, d)| d.as_slice())
        .unwrap_or(&[]);
    if let Some(gn) = load_game_nodes(segs, ssectors, nodes)? {
        apply_game_nodes(&mut geom, gn);
    }

    if let Some((_, data)) = lumps.iter().find(|(n, _)| n.eq_ignore_ascii_case("REJECT")) {
        match parse_reject(data, geom.sectors.len()) {
            Ok(r) => geom.reject = Some(r),
            Err(e) => {
                geom.warnings.push(format!("REJECT: {e}; rebuilding"));
                geom.reject = Some(build_reject(&geom.lines, geom.sectors.len()));
            }
        }
    } else {
        geom.reject = Some(build_reject(&geom.lines, geom.sectors.len()));
    }

    Ok(geom)
}

fn apply_game_nodes(geom: &mut MapGeometry, mut gn: crate::nodes::GameNodes) {
    for v in gn.extra_vertices.drain(..) {
        geom.vertices.push(Vertex { pos: v });
    }
    geom.game_nodes = Some(gn);
}

fn apply_extended_nodes(geom: &mut MapGeometry, ext: ExtendedNodes) {
    match ext {
        ExtendedNodes::Game(gn) => apply_game_nodes(geom, gn),
        ExtendedNodes::Gl(mut gl) => {
            for v in gl.extra_vertices.drain(..) {
                geom.vertices.push(Vertex { pos: v });
            }
            let game = gl_nodes_to_game(&gl);
            geom.gl_nodes = Some(gl);
            // Prefer existing classic game nodes; otherwise use GL-derived.
            if geom.game_nodes.is_none() {
                geom.game_nodes = Some(game);
            }
        }
    }
}

fn require_lump<'a>(lumps: &'a [(String, Vec<u8>)], name: &str) -> Result<&'a [u8], MapError> {
    lumps
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, d)| d.as_slice())
        .ok_or_else(|| MapError::NotFound(name.to_owned()))
}

pub(crate) fn collect_map_lumps(
    vfs: &Vfs,
    marker: LumpId,
) -> Result<Vec<(String, Vec<u8>)>, MapError> {
    let start = marker.0 as usize;
    if start >= vfs.lump_count() {
        return Err(MapError::Message("marker out of range".into()));
    }
    let marker_container = vfs
        .lump(marker)
        .map(|i| i.container)
        .ok_or_else(|| MapError::Message("marker missing".into()))?;

    let mut out = Vec::new();
    let mut saw_textmap = false;
    for id in vfs.iter_lumps().skip(start + 1) {
        let info = vfs
            .lump(id)
            .ok_or_else(|| MapError::Message("lump missing".into()))?;
        if info.container != marker_container {
            break;
        }
        let name = info.short_name.to_string();
        if name.is_empty() {
            continue;
        }
        if is_map_marker(&name) {
            break;
        }
        let upper = name.to_ascii_uppercase();
        if matches!(
            upper.as_str(),
            "P_START" | "F_START" | "S_START" | "PP_START" | "FF_START" | "SS_START" | "TX_START"
        ) {
            break;
        }
        if upper == "TEXTMAP" {
            saw_textmap = true;
        }
        if saw_textmap && upper == "ENDMAP" {
            let data = vfs.read(id)?.to_vec();
            out.push((name, data));
            break;
        }
        let data = vfs.read(id)?.to_vec();
        out.push((name, data));
        if out.len() > 64 {
            break;
        }
    }
    Ok(out)
}

fn is_map_marker(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    if u.starts_with("MAP") && u.len() == 5 && u.as_bytes()[3..].iter().all(u8::is_ascii_digit) {
        return true;
    }
    if u.len() == 4
        && u.as_bytes()[0] == b'E'
        && u.as_bytes()[2] == b'M'
        && u.as_bytes()[1].is_ascii_digit()
        && u.as_bytes()[3].is_ascii_digit()
    {
        return true;
    }
    false
}

fn map_checksum(lumps: &[(String, Vec<u8>)], format: MapFormat) -> [u8; 16] {
    let mut hasher = Md5::new();
    for key in ["THINGS", "LINEDEFS", "SIDEDEFS", "SECTORS"] {
        if let Some((_, d)) = lumps.iter().find(|(n, _)| n.eq_ignore_ascii_case(key)) {
            hasher.update(d);
        }
    }
    if format == MapFormat::Hexen {
        if let Some((_, d)) = lumps
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("BEHAVIOR"))
        {
            hasher.update(d);
        }
    }
    let dig = hasher.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&dig);
    out
}

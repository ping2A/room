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

/// Names of every loadable map in `vfs`: a lump directly followed (in the same
/// container) by `THINGS` or `TEXTMAP`, so custom names (`MYMAP`, MAPINFO
/// names) count too. Later containers win on duplicates; the list is sorted
/// `E1M1 … E4M9`, then `MAP01 …`, then other names alphabetically.
#[must_use]
pub fn list_maps(vfs: &Vfs) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut prev: Option<(String, room_vfs::ContainerId)> = None;
    for id in vfs.iter_lumps() {
        let Some(info) = vfs.lump(id) else {
            continue;
        };
        let name = info.short_name.to_string();
        if let Some((marker, container)) = prev.take() {
            if container == info.container
                && (name.eq_ignore_ascii_case("THINGS") || name.eq_ignore_ascii_case("TEXTMAP"))
                && !marker.is_empty()
            {
                let up = marker.to_ascii_uppercase();
                if !names.contains(&up) {
                    names.push(up);
                }
            }
        }
        prev = Some((name, info.container));
    }
    names.sort_by(|a, b| map_sort_key(a).cmp(&map_sort_key(b)));
    names
}

fn map_sort_key(name: &str) -> (u8, u32, u32, String) {
    let u = name.to_ascii_uppercase();
    let b = u.as_bytes();
    if b.len() == 4 && b[0] == b'E' && b[2] == b'M' && b[1].is_ascii_digit() && b[3].is_ascii_digit() {
        return (0, u32::from(b[1] - b'0'), u32::from(b[3] - b'0'), u);
    }
    if let Some(n) = u.strip_prefix("MAP").and_then(|d| d.parse::<u32>().ok()) {
        return (1, n, 0, u);
    }
    (2, 0, 0, u)
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

#[cfg(test)]
mod list_maps_tests {
    #![allow(clippy::unwrap_used)]

    use super::list_maps;
    use room_vfs::VfsBuilder;

    fn wad(lumps: &[(&str, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut dir = Vec::new();
        for (name, body) in lumps {
            let off = 12 + data.len();
            data.extend_from_slice(body);
            dir.push((off as u32, body.len() as u32, *name));
        }
        let mut out = b"PWAD".to_vec();
        out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
        out.extend_from_slice(&((12 + data.len()) as u32).to_le_bytes());
        out.extend_from_slice(&data);
        for (off, len, name) in dir {
            out.extend_from_slice(&off.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            let mut n = [0u8; 8];
            n[..name.len()].copy_from_slice(name.as_bytes());
            out.extend_from_slice(&n);
        }
        out
    }

    #[test]
    fn lists_doom_udmf_and_custom_named_maps_in_play_order() {
        let a = wad(&[
            ("MAP10", b""),
            ("TEXTMAP", b"x"),
            ("ENDMAP", b""),
            ("E1M2", b""),
            ("THINGS", b""),
            ("LINEDEFS", b""),
            ("PLAYPAL", b"p"),
            ("COLORMAP", b"c"),
        ]);
        let b = wad(&[
            ("MYMAP", b""),
            ("TEXTMAP", b"y"),
            ("ENDMAP", b""),
            ("MAP02", b""),
            ("THINGS", b""),
            ("E1M2", b""),
            ("THINGS", b""),
        ]);
        let vfs = VfsBuilder::new()
            .mount_memory("a.wad", a)
            .unwrap()
            .mount_memory("b.wad", b)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(list_maps(&vfs), vec!["E1M2", "MAP02", "MAP10", "MYMAP"]);
    }
}

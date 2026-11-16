//! Play solo on generated maps: build a PWAD of one or more `room-mapgen`
//! maps for the IWAD being played (its textures, flats and monsters).

use room_vfs::{Namespace, VfsBuilder, WadWriter};
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// `MAP01` → `MAP02`…, `E1M1` → `E1M2`…; other names get a numeric suffix.
fn nth_name(first: &str, i: u8) -> String {
    let up = first.trim().to_ascii_uppercase();
    if let Some(n) = up.strip_prefix("MAP").and_then(|n| n.parse::<u32>().ok()) {
        return format!("MAP{:02}", n + u32::from(i));
    }
    let b = up.as_bytes();
    if b.len() == 4
        && b[0] == b'E'
        && b[2] == b'M'
        && b[1].is_ascii_digit()
        && b[3].is_ascii_digit()
    {
        return format!(
            "E{}M{}",
            b[1] as char,
            u32::from(b[3] - b'0') + u32::from(i)
        );
    }
    if i == 0 {
        up
    } else {
        format!("{}{}", &up[..up.len().min(6)], i + 1)
    }
}

/// Generate `count` maps (`options.name` first, then the following map
/// names, each one step further into the episode) as a PWAD for `iwad`.
/// The report of every map is logged to the console.
#[wasm_bindgen(js_name = roomGenerateWad)]
pub fn room_generate_wad(options_json: &str, iwad: &[u8], count: u8) -> Result<Vec<u8>, JsValue> {
    let opts: room_mapgen::Options = serde_json::from_str(options_json).map_err(err)?;
    let vfs = VfsBuilder::new()
        .mount_memory("game.wad", iwad.to_vec())
        .and_then(VfsBuilder::build)
        .map_err(err)?;
    let mut mats = room_render::MaterialCatalog::new();
    mats.load_texturex(&vfs);
    let textures: std::collections::HashSet<String> =
        mats.texture_names().map(str::to_ascii_uppercase).collect();
    let has = |kind: room_mapgen::Kind, name: &str| match kind {
        room_mapgen::Kind::Texture => textures.contains(&name.to_ascii_uppercase()),
        room_mapgen::Kind::Flat => vfs.find(name, Namespace::Flats).is_some(),
    };
    let has_sprite = |prefix: &str| {
        ["A1", "A2A8", "A0"].iter().any(|f| {
            vfs.find(&format!("{prefix}{f}"), Namespace::Sprites)
                .is_some()
        })
    };
    let mut wad = WadWriter::pwad();
    for i in 0..count.clamp(1, 9) {
        let name = nth_name(&opts.name, i);
        let o = room_mapgen::Options {
            seed: opts.seed.wrapping_add(u64::from(i)),
            name: name.clone(),
            progress: opts.progress.saturating_add(i),
            ..opts.clone()
        };
        let g = room_mapgen::generate_for_game(&o, &has, &has_sprite)
            .map_err(|e| err(format!("{name}: {e}")))?;
        web_sys::console::info_1(&JsValue::from_str(&format!(
            "[Room mapgen] {name}: seed {} · {} rooms · {} monsters · {} weapons · {} sectors · {} lines",
            g.report.seed,
            g.report.rooms,
            g.report.monsters,
            g.report.weapons,
            g.report.sectors,
            g.report.lines
        )));
        for (lump, data) in g.map.to_lumps().map_err(err)? {
            wad.add(&lump, data);
        }
    }
    wad.finish().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::nth_name;

    #[test]
    fn map_names_follow_the_game() {
        assert_eq!(nth_name("MAP01", 2), "MAP03");
        assert_eq!(nth_name("e1m1", 3), "E1M4");
        assert_eq!(nth_name("ROOMGEN", 0), "ROOMGEN");
    }
}

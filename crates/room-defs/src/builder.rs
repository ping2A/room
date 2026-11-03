//! Build compiled definitions from a mounted [`Vfs`].

use crate::animdefs::{AnimDefs, parse_animdefs};
use crate::complevel::{Complevel, parse_complvl};
use crate::cvarinfo::{CvarDecl, parse_cvarinfo};
use crate::decaldef::{DecalInfo, parse_decaldef};
use crate::error::DefsError;
use crate::fontdefs::{FontDef, parse_fontdefs};
use crate::gameinfo::{GameInfo, parse_gameinfo};
use crate::gldefs::{GlDefs, parse_gldefs};
use crate::keyconf::{KeyConf, parse_keyconf};
use crate::language::{Language, parse_language};
use crate::loadacs::{LoadAcs, parse_loadacs};
use crate::id24::{GameConf, Id24Defs, parse_demoloop, parse_json_lump, parse_skydefs};
use crate::lockdefs::{LockDefs, parse_lockdefs};
use crate::mapinfo::{MapInfo, parse_mapinfo};
use crate::menudef::{MenuDef, parse_menudef};
use crate::modeldef::{ModelDefs, parse_modeldef};
use crate::sbarinfo::{SbarInfo, parse_sbarinfo};
use crate::sndinfo::{SoundRegistry, parse_sndinfo};
use crate::sndseq::{SoundSeqRegistry, parse_sndseq};
use crate::terrain::{TerrainInfo, parse_terrain};
use crate::textures::{TextureRegistry, merge_texturex, parse_pnames, parse_textures};
use crate::umapinfo::{merge_umapinfo, parse_umapinfo};
use crate::voxeldef::{VoxelDefs, parse_voxeldef};
use room_formats::{
    Colormap, Playpal, Tranmap, parse_animated, parse_playpal, parse_switches, parse_texturex,
};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Immutable compiled definitions (M2 subset of the architecture `Definitions`).
#[derive(Clone, Debug, Default)]
pub struct Definitions {
    /// GAMEINFO (first PWAD).
    pub gameinfo: Option<GameInfo>,
    /// MAPINFO + UMAPINFO.
    pub mapinfo: MapInfo,
    /// SNDINFO.
    pub sounds: SoundRegistry,
    /// SNDSEQ.
    pub sndseq: SoundSeqRegistry,
    /// LANGUAGE tables.
    pub language: Language,
    /// TEXTURES + TEXTURE1/2.
    pub textures: TextureRegistry,
    /// PNAMES patch list.
    pub pnames: Vec<String>,
    /// ANIMDEFS (+ merged Boom ANIMATED).
    pub animdefs: AnimDefs,
    /// CVARINFO decls.
    pub cvars: Vec<CvarDecl>,
    /// FONTDEFS.
    pub fonts: BTreeMap<String, FontDef>,
    /// SBARINFO (last lump wins).
    pub sbarinfo: Option<SbarInfo>,
    /// TERRAIN (additive across lumps).
    pub terrain: TerrainInfo,
    /// DECALDEF (additive across lumps).
    pub decaldef: DecalInfo,
    /// LOCKDEFS (additive; `ClearLocks` resets within the concatenated stream).
    pub lockdefs: LockDefs,
    /// KEYCONF (additive command list across lumps).
    pub keyconf: KeyConf,
    /// MENUDEF (additive across lumps).
    pub menudef: MenuDef,
    /// LOADACS library names (additive across lumps).
    pub loadacs: LoadAcs,
    /// GLDEFS (additive; skyboxes for now).
    pub gldefs: GlDefs,
    /// MODELDEF (later actor wins).
    pub modeldefs: ModelDefs,
    /// VOXELDEF (later name wins).
    pub voxeldefs: VoxelDefs,
    /// PLAYPAL if present.
    pub playpal: Option<Playpal>,
    /// COLORMAP if present.
    pub colormap: Option<Colormap>,
    /// Boom `TRANMAP` (or Heretic `TINTTAB` / Strife `XLATAB`) if present.
    pub tranmap: Option<Tranmap>,
    /// Boom SWITCHES.
    pub switches: Vec<room_formats::SwitchRecord>,
    /// `COMPLVL` lump preset (default when absent).
    pub complevel: Complevel,
    /// ID24 JSON lumps (`SKYDEFS`, `DEMOLOOP`, `GAMECONF`, raw `TRAKINFO`/`SBARDEF`).
    pub id24: Id24Defs,
    /// Warnings collected during build.
    pub warnings: Vec<String>,
}

/// Compiles lumps from a [`Vfs`] in load order.
pub struct DefinitionsBuilder<'a> {
    vfs: &'a Vfs,
}

impl<'a> DefinitionsBuilder<'a> {
    /// Wrap a mounted VFS.
    #[must_use]
    pub fn new(vfs: &'a Vfs) -> Self {
        Self { vfs }
    }

    /// Run phases 0–6 and 9 (M2 subset).
    pub fn build(self) -> Result<Definitions, DefsError> {
        let mut defs = Definitions::default();

        if let Some(text) = self.read_all_text("GAMEINFO") {
            match parse_gameinfo(&text) {
                Ok(g) => defs.gameinfo = Some(g),
                Err(e) => defs.warnings.push(format!("GAMEINFO: {e}")),
            }
        }

        if let Some(text) = self.read_all_text("LANGUAGE") {
            match parse_language(&text) {
                Ok(l) => defs.language = l,
                Err(e) => defs.warnings.push(format!("LANGUAGE: {e}")),
            }
        }

        if let Some(bytes) = self.read_bytes("PLAYPAL") {
            match parse_playpal(&bytes) {
                Ok(p) => defs.playpal = Some(p),
                Err(e) => defs.warnings.push(format!("PLAYPAL: {e}")),
            }
        }
        if let Some(bytes) = self.read_bytes("COLORMAP") {
            match Colormap::parse(&bytes) {
                Ok(c) => defs.colormap = Some(c),
                Err(e) => defs.warnings.push(format!("COLORMAP: {e}")),
            }
        }
        if let Some(bytes) = self
            .read_bytes("TRANMAP")
            .or_else(|| self.read_bytes("TINTTAB"))
            .or_else(|| self.read_bytes("XLATAB"))
        {
            match Tranmap::parse(&bytes) {
                Ok(t) => defs.tranmap = Some(t),
                Err(e) => defs.warnings.push(format!("TRANMAP: {e}")),
            }
        }

        if let Some(bytes) = self.read_bytes("PNAMES") {
            match parse_pnames(&bytes) {
                Ok(n) => defs.pnames = n,
                Err(e) => defs.warnings.push(format!("PNAMES: {e}")),
            }
        }
        for lump in ["TEXTURE1", "TEXTURE2"] {
            if let Some(bytes) = self.read_bytes(lump) {
                match parse_texturex(&bytes) {
                    Ok(tx) => merge_texturex(&mut defs.textures, &tx, &defs.pnames),
                    Err(e) => defs.warnings.push(format!("{lump}: {e}")),
                }
            }
        }
        if let Some(text) = self.read_all_text("TEXTURES") {
            match parse_textures(&text) {
                Ok(t) => {
                    // TEXTURES text overrides binary TEXTUREx (last wins per name).
                    for (k, v) in t.by_name {
                        defs.textures.by_name.insert(k, v);
                    }
                }
                Err(e) => defs.warnings.push(format!("TEXTURES: {e}")),
            }
        }

        if let Some(text) = self.read_mapinfo_text() {
            match parse_mapinfo(&text) {
                Ok(m) => defs.mapinfo = m,
                Err(e) => defs.warnings.push(format!("MAPINFO: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("UMAPINFO") {
            match parse_umapinfo(&text) {
                Ok(u) => merge_umapinfo(&mut defs.mapinfo, u),
                Err(e) => defs.warnings.push(format!("UMAPINFO: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("COMPLVL") {
            match parse_complvl(&text) {
                Ok(c) => defs.complevel = c,
                Err(e) => defs.warnings.push(format!("COMPLVL: {e}")),
            }
        }

        if let Some(text) = self.read_all_text("SNDINFO") {
            match parse_sndinfo(&text) {
                Ok(s) => defs.sounds = s,
                Err(e) => defs.warnings.push(format!("SNDINFO: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("SNDSEQ") {
            match parse_sndseq(&text) {
                Ok(s) => defs.sndseq = s,
                Err(e) => defs.warnings.push(format!("SNDSEQ: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("ANIMDEFS") {
            match parse_animdefs(&text) {
                Ok(a) => defs.animdefs = a,
                Err(e) => defs.warnings.push(format!("ANIMDEFS: {e}")),
            }
        }
        if let Some(bytes) = self.read_bytes("ANIMATED") {
            if let Ok(recs) = parse_animated(&bytes) {
                for rec in recs {
                    defs.animdefs.anims.push(crate::animdefs::AnimDef {
                        kind: if rec.kind == 0 {
                            "flat".into()
                        } else {
                            "texture".into()
                        },
                        name: rec.first.clone(),
                        optional: false,
                        oscillate: false,
                        allow_decals: rec.kind == 2,
                        pics: vec![
                            crate::animdefs::AnimPic {
                                name: rec.first,
                                tics: Some(i64::from(rec.speed)),
                                rand: None,
                            },
                            crate::animdefs::AnimPic {
                                name: format!("RANGE:{}", rec.last),
                                tics: Some(i64::from(rec.speed)),
                                rand: None,
                            },
                        ],
                    });
                }
            }
        }
        if let Some(bytes) = self.read_bytes("SWITCHES") {
            match parse_switches(&bytes) {
                Ok(s) => defs.switches = s,
                Err(e) => defs.warnings.push(format!("SWITCHES: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("CVARINFO") {
            match parse_cvarinfo(&text) {
                Ok(c) => defs.cvars = c,
                Err(e) => defs.warnings.push(format!("CVARINFO: {e}")),
            }
        }
        if let Some(text) = self.read_all_text("FONTDEFS") {
            match parse_fontdefs(&text) {
                Ok(f) => defs.fonts = f,
                Err(e) => defs.warnings.push(format!("FONTDEFS: {e}")),
            }
        }
        // SBARINFO: last lump wins (GZDoom).
        if let Some(text) = self.read_last_text("SBARINFO") {
            match parse_sbarinfo(&text) {
                Ok(s) => defs.sbarinfo = Some(s),
                Err(e) => defs.warnings.push(format!("SBARINFO: {e}")),
            }
        }
        // TERRAIN: additive (concat all lumps; Doom `if*` filter).
        if let Some(text) = self.read_all_text("TERRAIN") {
            match parse_terrain(&text) {
                Ok(t) => {
                    for (k, v) in t.splashes {
                        defs.terrain.splashes.insert(k, v);
                    }
                    for (k, v) in t.terrains {
                        defs.terrain.terrains.insert(k, v);
                    }
                    for (k, v) in t.floors {
                        defs.terrain.floors.insert(k, v);
                    }
                    if t.default_terrain.is_some() {
                        defs.terrain.default_terrain = t.default_terrain;
                    }
                }
                Err(e) => defs.warnings.push(format!("TERRAIN: {e}")),
            }
        }
        // DECALDEF: additive.
        if let Some(text) = self.read_all_text("DECALDEF") {
            match parse_decaldef(&text) {
                Ok(d) => {
                    for (k, v) in d.decals {
                        defs.decaldef.decals.insert(k, v);
                    }
                    for (k, v) in d.groups {
                        defs.decaldef.groups.insert(k, v);
                    }
                    for (k, v) in d.animators {
                        defs.decaldef.animators.insert(k, v);
                    }
                    for (k, v) in d.generators {
                        defs.decaldef.generators.insert(k, v);
                    }
                }
                Err(e) => defs.warnings.push(format!("DECALDEF: {e}")),
            }
        }
        // LOCKDEFS: additive; ClearLocks in the stream resets.
        if let Some(text) = self.read_all_text("LOCKDEFS") {
            match parse_lockdefs(&text) {
                Ok(l) => defs.lockdefs = l,
                Err(e) => defs.warnings.push(format!("LOCKDEFS: {e}")),
            }
        }
        // ID24 JSON lumps. DEMOLOOP / SKYDEFS: the last lump wins; GAMECONF merges in
        // load order (IWAD first); TRAKINFO / SBARDEF are kept as validated raw JSON.
        for text in self.read_each_text("SKYDEFS") {
            match parse_skydefs(&text) {
                Ok(s) => defs.id24.skydefs = Some(s),
                Err(e) => defs.warnings.push(format!("{e}")),
            }
        }
        for text in self.read_each_text("DEMOLOOP") {
            match parse_demoloop(&text) {
                Ok(d) => defs.id24.demoloop = Some(d),
                Err(e) => defs.warnings.push(format!("{e}")),
            }
        }
        for text in self.read_each_text("GAMECONF") {
            match GameConf::parse(&text) {
                Ok(c) => defs.id24.gameconf.merge(c),
                Err(e) => defs.warnings.push(format!("{e}")),
            }
        }
        for name in ["TRAKINFO", "SBARDEF"] {
            for text in self.read_each_text(name) {
                match parse_json_lump(&text, None) {
                    Ok(r) => {
                        defs.id24.raw.insert(name.to_owned(), r);
                    }
                    Err(e) => defs.warnings.push(format!("{name}: {e}")),
                }
            }
        }
        // KEYCONF: additive command list (GZDoom executes each lump in order).
        if let Some(text) = self.read_all_text("KEYCONF") {
            match parse_keyconf(&text) {
                Ok(k) => {
                    defs.warnings.extend(k.warnings.iter().cloned());
                    defs.keyconf.commands.extend(k.commands);
                }
                Err(e) => defs.warnings.push(format!("KEYCONF: {e}")),
            }
        }
        // MENUDEF: additive (later blocks override same name in by_name).
        if let Some(text) = self.read_all_text("MENUDEF") {
            match parse_menudef(&text) {
                Ok(m) => {
                    for b in m.blocks {
                        if let Some(ref name) = b.name {
                            defs.menudef
                                .by_name
                                .insert(name.clone(), defs.menudef.blocks.len());
                        }
                        defs.menudef.blocks.push(b);
                    }
                }
                Err(e) => defs.warnings.push(format!("MENUDEF: {e}")),
            }
        }
        // LOADACS: additive library name list.
        if let Some(text) = self.read_all_text("LOADACS") {
            match parse_loadacs(&text) {
                Ok(l) => {
                    for name in l.libraries {
                        let key = name.to_ascii_lowercase();
                        if !defs
                            .loadacs
                            .libraries
                            .iter()
                            .any(|n| n.eq_ignore_ascii_case(&key))
                        {
                            defs.loadacs.libraries.push(name);
                        }
                    }
                }
                Err(e) => defs.warnings.push(format!("LOADACS: {e}")),
            }
        }
        // GLDEFS (+ game-specific aliases): additive skyboxes + dyn lights.
        for lump in ["GLDEFS", "DOOMDEFS", "HTICDEFS", "HEXNDEFS", "STRFDEFS"] {
            if let Some(text) = self.read_all_text(lump) {
                match parse_gldefs(&text) {
                    Ok(g) => {
                        for (k, v) in g.skyboxes {
                            defs.gldefs.skyboxes.insert(k, v);
                        }
                        for (k, v) in g.lights {
                            defs.gldefs.lights.insert(k, v);
                        }
                        defs.gldefs.object_lights.extend(g.object_lights);
                        defs.gldefs.brightmaps.extend(g.brightmaps);
                        defs.gldefs.glow_textures.extend(g.glow_textures);
                        defs.gldefs.glow_flats.extend(g.glow_flats);
                        defs.gldefs.glow_walls.extend(g.glow_walls);
                        defs.gldefs.materials.extend(g.materials);
                    }
                    Err(e) => defs.warnings.push(format!("{lump}: {e}")),
                }
            }
        }
        // MODELDEF: additive; later definition for the same actor replaces.
        if let Some(text) = self.read_all_text("MODELDEF") {
            match parse_modeldef(&text) {
                Ok(m) => {
                    for (k, v) in m.by_actor {
                        defs.modeldefs.by_actor.insert(k, v);
                    }
                }
                Err(e) => defs.warnings.push(format!("MODELDEF: {e}")),
            }
        }
        // VOXELDEF: additive; later name replaces.
        if let Some(text) = self.read_all_text("VOXELDEF") {
            match parse_voxeldef(&text) {
                Ok(v) => {
                    for (k, def) in v.by_name {
                        defs.voxeldefs.by_name.insert(k, def);
                    }
                }
                Err(e) => defs.warnings.push(format!("VOXELDEF: {e}")),
            }
        }

        Ok(defs)
    }

    fn read_bytes(&self, name: &str) -> Option<Vec<u8>> {
        let id = self
            .vfs
            .find(name, Namespace::Global)
            .or_else(|| self.vfs.find_any_ns(name))?;
        self.vfs.read(id).ok().map(|b| b.to_vec())
    }

    fn read_last_text(&self, name: &str) -> Option<String> {
        let ids: Vec<_> = self.vfs.find_all(name, Namespace::Global).collect();
        if let Some(&id) = ids.last() {
            return self
                .vfs
                .read(id)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned());
        }
        self.read_bytes(name)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// Every archive's ZMAPINFO, or its MAPINFO when it has none, in load
    /// order (GZDoom picks per archive, so an IWAD/PWAD MAPINFO still layers
    /// over the engine's ZMAPINFO).
    fn read_mapinfo_text(&self) -> Option<String> {
        let z: Vec<_> = self.vfs.find_all("ZMAPINFO", Namespace::Global).collect();
        let has_z = |id| {
            let c = self.vfs.lump(id).map(|l| l.container);
            z.iter()
                .any(|&zid| self.vfs.lump(zid).map(|l| l.container) == c)
        };
        let mut ids: Vec<_> = self
            .vfs
            .find_all("MAPINFO", Namespace::Global)
            .filter(|&id| !has_z(id))
            .chain(z.iter().copied())
            .collect();
        if ids.is_empty() {
            return self
                .read_all_text("ZMAPINFO")
                .or_else(|| self.read_all_text("MAPINFO"));
        }
        ids.sort_by_key(|id| id.0);
        let mut out = String::new();
        for id in ids {
            if let Ok(b) = self.vfs.read(id) {
                out.push('\n');
                out.push_str(&String::from_utf8_lossy(&b));
            }
        }
        Some(out)
    }

    /// Every lump named `name` (load order), each decoded separately.
    fn read_each_text(&self, name: &str) -> Vec<String> {
        self.vfs
            .find_all(name, Namespace::Global)
            .filter_map(|id| self.vfs.read(id).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .collect()
    }

    fn read_all_text(&self, name: &str) -> Option<String> {
        let ids: Vec<_> = self.vfs.find_all(name, Namespace::Global).collect();
        if ids.is_empty() {
            return self
                .read_bytes(name)
                .map(|b| String::from_utf8_lossy(&b).into_owned());
        }
        let mut out = String::new();
        for id in ids {
            if let Ok(b) = self.vfs.read(id) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&String::from_utf8_lossy(&b));
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::{VfsBuilder, WadWriter};

    #[test]
    fn build_from_wad() {
        let wad = WadWriter::pwad()
            .add("MAPINFO", br#"map MAP01 "Test" { next = MAP02 }"#)
            .add("SNDINFO", b"weapons/pistol DSPISTOL\n")
            .add(
                "SNDSEQ",
                b":Door\n\tplayuntildone doors/dr1_open\n\tdoor 0\nend\n",
            )
            .add("COMPLVL", b"mbf21\n")
            .finish()
            .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("test.wad", wad)
            .unwrap()
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        assert!(defs.mapinfo.maps.contains_key("MAP01"));
        assert!(defs.sounds.sounds.contains_key("weapons/pistol"));
        assert!(defs.sndseq.sequences.contains_key("DOOR"));
        assert_eq!(
            defs.sndseq.door_slots.get(&0).map(String::as_str),
            Some("DOOR")
        );
        assert_eq!(defs.complevel, Complevel::Mbf21);
    }

    #[test]
    fn loads_tranmap_lump() {
        let mut table = vec![0u8; 65536];
        table[0] = 42;
        let wad = WadWriter::pwad().add("TRANMAP", table).finish().unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("tm.wad", wad)
            .unwrap()
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        let tm = defs.tranmap.as_ref().unwrap();
        assert_eq!(tm.blend(0, 0), 42);
    }

    #[test]
    fn id24_lumps_load_and_merge_in_load_order() {
        let sky = |n: &str| {
            format!(r#"{{"type":"skydefs","version":"1.0.0","data":{{"skies":[{{"type":0,"name":"{n}"}}]}}}}"#)
        };
        let gc = |d: &str| format!(r#"{{"type":"gameconf","version":"1.0.0","data":{d}}}"#);
        let iwad = WadWriter::pwad()
            .add("SKYDEFS", sky("FIRST").as_bytes())
            .add("GAMECONF", gc(r#"{"title":"Base","pwads":["a.wad"],"executable":"mbf","options":"x 1"}"#).as_bytes())
            .add("DEMOLOOP", br#"{"type":"demoloop","version":"1.0.0","data":{"entries":[{"primarylump":"D1","type":1}]}}"#)
            .add("TRAKINFO", br#"{"type":"trakinfo","version":"1.0.0","data":{"tracks":[]}}"#)
            .finish()
            .unwrap();
        let pwad = WadWriter::pwad()
            .add("SKYDEFS", sky("SECOND").as_bytes())
            .add("GAMECONF", gc(r#"{"pwads":["b.wad"],"executable":"boom2.02","options":"y 2"}"#).as_bytes())
            .add("DEMOLOOP", b"{ not json")
            .finish()
            .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("base.wad", iwad)
            .unwrap()
            .mount_memory("mod.wad", pwad)
            .unwrap()
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        let sd = defs.id24.skydefs.as_ref().unwrap();
        assert_eq!(sd.skies.len(), 1);
        assert_eq!(sd.skies[0].name, "SECOND"); // last lump wins
        assert_eq!(defs.id24.demoloop.as_ref().unwrap().entries[0].primary, "D1"); // bad later lump ignored
        assert!(defs.warnings.iter().any(|w| w.starts_with("DEMOLOOP:")));
        let g = &defs.id24.gameconf;
        assert_eq!(g.title.as_deref(), Some("Base"));
        assert_eq!(g.pwads, vec!["a.wad", "b.wad"]);
        assert_eq!(g.executable, Some(crate::Executable::Mbf)); // max(mbf, boom2.02)
        assert_eq!(g.option_lines().collect::<Vec<_>>(), ["x 1", "y 2"]);
        assert_eq!(defs.id24.raw["TRAKINFO"].kind, "trakinfo");
    }

    #[test]
    fn definitions_dump_snapshot() {
        let wad = WadWriter::pwad()
            .add(
                "MAPINFO",
                br#"map MAP01 "Snap"
{
    next = MAP02
    music = D_RUNNIN
}
"#,
            )
            .add(
                "SNDINFO",
                b"weapons/pistol DSPISTOL\n$limit weapons/pistol 1\n",
            )
            .add("CVARINFO", b"server int my_cvar = 3;\n")
            .finish()
            .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("snap.wad", wad)
            .unwrap()
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        insta::assert_snapshot!(crate::dump_debug(&defs));
    }
}

//! Palette-indexed native Duke scene rendered to a CPU RGBA frame.
//!
//! The classic soft renderer is a bootstrap path; it keeps simulation and
//! rendering read-only separated and gives desktop/web shells one portable
//! image to upload. Pixel parity and advanced Build surfaces remain D3 work.

use room_build::ConData;
use room_build::art::{MAX_TILES, TileSet};
use room_build::grp::GrpStack;
use room_build::palette::Palette;
use room_build_engine::soft::{SoftFrame, SoftTiles, render_fp_textured_at};
use room_build_engine::view::BuildView;
use room_build_engine::world::BuildWorld;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::player::Player;

fn identity_brightness_table() -> [[u8; 64]; 16] {
    std::array::from_fn(|_| std::array::from_fn(|channel| channel as u8))
}

/// Rotate the palette frame around its center using Build-angle fixed-point
/// trig. Outside pixels use palette index zero, matching a clipped screen edge.
fn rotate_palette_frame(pixels: &[u8], width: u32, height: u32, angle: i16) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let Some(expected) = width.checked_mul(height) else {
        return pixels.to_vec();
    };
    if width == 0 || height == 0 || pixels.len() != expected || angle == 0 {
        return pixels.to_vec();
    }
    let (sin, cos) = room_build_engine::helpers::sincos_q14(i32::from(angle));
    let mut rotated = vec![0; expected];
    let center_x2 = width as i64 - 1;
    let center_y2 = height as i64 - 1;
    for y in 0..height {
        for x in 0..width {
            let dx2 = 2 * x as i64 - center_x2;
            let dy2 = 2 * y as i64 - center_y2;
            let src_dx2 = (dx2 * i64::from(cos) + dy2 * i64::from(sin)) >> 14;
            let src_dy2 = (-dx2 * i64::from(sin) + dy2 * i64::from(cos)) >> 14;
            let src_x = (src_dx2 + center_x2).div_euclid(2);
            let src_y = (src_dy2 + center_y2).div_euclid(2);
            if src_x >= 0 && src_y >= 0 && src_x < width as i64 && src_y < height as i64 {
                rotated[y * width + x] = pixels[src_y as usize * width + src_x as usize];
            }
        }
    }
    rotated
}

/// Reusable CPU renderer with the palette and ART tiles owned by an Atomic GRP.
pub struct DukeSoftRenderer {
    palette: Palette,
    base_palettes: Option<room_build::lookup::BasePalettes>,
    /// Full-screen palette variants generated from LOOKUP.DAT remap tables.
    game_palette_tables: BTreeMap<u8, [u8; 768]>,
    /// Atomic `BRITABLE`, indexed by brightness (0..15) and six-bit channel.
    brightness_table: [[u8; 64]; 16],
    /// Atomic's default `ud.brightness` is 16, i.e. BRITABLE row 4.
    brightness: Cell<u8>,
    tiles: RefCell<SoftTiles>,
    /// Render-only cache for Chocolate's twelve-tic camera-screen refresh.
    camera_feed: RefCell<Option<CameraFeedCache>>,
    /// Previous/current geometry for render-only interpolation of moving
    /// Build walls. The playsim world remains untouched.
    geometry_interpolation: RefCell<Option<GeometryInterpolation>>,
    weapon_tiles: [Option<i16>; 12],
    access_hand_tile: Option<i16>,
    laser_hand_tile: Option<i16>,
    /// Active `fta_quotes` text for this frame (Chocolate `operatefta`).
    overlay_quote: RefCell<Option<String>>,
    /// Chocolate `ud.screen_size`: `true` draws the full BOTTOMSTATUSBAR
    /// (health, armor, weapon ammo list, inventory, keys); `false` the
    /// full-screen mini HUD.
    full_status_bar: Cell<bool>,
    map_hack_defs: Vec<(String, Vec<u8>)>,
    map_hack_files: Vec<(String, Vec<u8>)>,
    map_hack_sprite_angles: RefCell<BTreeMap<usize, i16>>,
}

#[derive(Clone)]
struct GeometryInterpolation {
    tic: u32,
    previous_walls: Vec<(i32, i32)>,
    current_walls: Vec<(i32, i32)>,
    previous_planes: Vec<(i32, i32)>,
    current_planes: Vec<(i32, i32)>,
}

struct CameraFeedCache {
    camera_index: usize,
    last_refresh_tic: u32,
    indexed_pixels: Vec<u8>,
}

impl DukeSoftRenderer {
    /// Discard the render-only monitor image after replacing or restoring a
    /// Build world. Camera sprite indices and map tics can repeat across both
    /// operations, so neither is sufficient to identify a cached feed.
    pub fn invalidate_camera_feed(&self) {
        *self.camera_feed.borrow_mut() = None;
    }

    /// Load `PALETTE.DAT` and all valid tiles from the GRP's `TILES*.ART` lumps.
    pub fn from_grp(grp_bytes: &[u8]) -> Result<Self, String> {
        Self::from_grp_layers(&[grp_bytes])
    }

    /// Load palette and ART assets from a base GRP plus ordered add-ons.
    pub fn from_grp_layers(grp_layers: &[&[u8]]) -> Result<Self, String> {
        Self::from_grp_layers_with_tables(grp_layers, None)
    }

    /// Load assets while optionally supplying the installation's standalone
    /// `TABLES.DAT` (Chocolate ships this beside the GRP on some installs).
    pub fn from_grp_layers_with_tables(
        grp_layers: &[&[u8]],
        tables_dat: Option<&[u8]>,
    ) -> Result<Self, String> {
        let grp = GrpStack::parse_layers(grp_layers).map_err(|e| format!("GRP: {e}"))?;
        let palette = Palette::parse(grp.get("PALETTE.DAT").ok_or("missing PALETTE.DAT")?)
            .map_err(|e| format!("PALETTE.DAT: {e}"))?;
        let lookups = grp
            .get("LOOKUP.DAT")
            .map(room_build::lookup::Lookups::parse)
            .transpose()
            .map_err(|error| format!("LOOKUP.DAT: {error}"))?;
        let base_palettes = lookups
            .as_ref()
            .and_then(|lookups| lookups.base_palettes.clone());
        let game_palette_tables = lookups.as_ref().map_or_else(BTreeMap::new, |lookups| {
            lookups
                .iter()
                .map(|(palette_id, lookup)| {
                    let mut rgb = [0; 768];
                    for (index, &source_index) in lookup.iter().enumerate() {
                        let source = usize::from(source_index) * 3;
                        let destination = index * 3;
                        rgb[destination..destination + 3]
                            .copy_from_slice(&palette.rgb[source..source + 3]);
                    }
                    (palette_id, rgb)
                })
                .collect()
        });
        let brightness_table = tables_dat
            .or_else(|| grp.get("TABLES.DAT"))
            .and_then(room_build_engine::helpers::britable_from_tables_dat)
            .unwrap_or_else(identity_brightness_table);
        let definitions = grp
            .get("DEFS.CON")
            .map(|data| ConData::parse(&String::from_utf8_lossy(data)));
        let mut set = TileSet::new();
        for name in grp.names() {
            let upper = name.to_ascii_uppercase();
            if upper.starts_with("TILES") && upper.ends_with(".ART") {
                if let Some(data) = grp.get(name) {
                    set.add_art(data).map_err(|e| format!("{name}: {e}"))?;
                }
            }
        }
        if set.is_empty() {
            return Err("GRP contains no valid TILES*.ART assets".into());
        }
        let pics = (0..MAX_TILES as i32)
            .map(|pic| pic as i16)
            .collect::<Vec<_>>();
        let mut tiles = SoftTiles::from_tile_set_pics(&set, &pics);
        tiles.set_shade_tables(palette.numshades, palette.shades.clone());
        tiles.set_translucency_table(palette.translucency.clone());
        if let Some(lookups) = lookups.as_ref() {
            for (pal, table) in lookups.iter() {
                tiles.set_palette_lookup(pal, *table);
            }
        }
        if let Some(defs) = definitions.as_ref() {
            for name in ["MOONSKY1", "BIGORBIT1", "LA"] {
                let Some(base) = defs.value(name) else {
                    continue;
                };
                let Ok(base_tile) = i16::try_from(base) else {
                    continue;
                };
                let offsets = room_build::wad::duke_sky_offsets(defs, base)
                    .map(|offset| i16::try_from(offset).unwrap_or(0));
                tiles.set_sky_offsets(base_tile, offsets);
            }
            // Chocolate `setupbackdrop`: LA follows the horizon least,
            // CLOUDYOCEAN fully, every other sky at half (`parallaxyscale`).
            for (name, scale) in [("LA", 16384 + 1024), ("CLOUDYOCEAN", 65536)] {
                if let Some(base) = defs.value(name).and_then(|b| i16::try_from(b).ok()) {
                    tiles.set_sky_yscale(base, scale);
                }
            }
            for name in ["MOONSKY1", "BIGORBIT1"] {
                if let Some(base) = defs.value(name).and_then(|b| i16::try_from(b).ok()) {
                    tiles.set_sky_yscale(base, 32768);
                }
            }
        }
        let weapon_names = [
            "KNEE",
            "FIRSTGUN",
            "SHOTGUN",
            "CHAINGUN",
            "RPGGUN",
            "HANDTHROW",
            "SHRINKER",
            "DEVISTATOR",
            "HANDHOLDINGLASER",
            "FREEZE",
            "HANDREMOTE",
            "SHRINKER",
        ];
        let weapon_tiles = weapon_names.map(|name| {
            definitions
                .as_ref()
                .and_then(|defs| defs.value(name))
                .and_then(|tile| i16::try_from(tile).ok())
                .filter(|tile| tiles.dimensions(*tile).is_some())
        });
        let hud_tile = |name: &str| {
            definitions
                .as_ref()
                .and_then(|defs| defs.value(name))
                .and_then(|tile| i16::try_from(tile).ok())
                .filter(|tile| tiles.dimensions(*tile).is_some())
        };
        let access_hand_tile = hud_tile("HANDHOLDINGACCESS");
        let laser_hand_tile = hud_tile("HANDHOLDINGLASER");
        drop(hud_tile);
        Ok(Self {
            palette,
            base_palettes,
            game_palette_tables,
            brightness_table,
            brightness: Cell::new(4),
            tiles: RefCell::new(tiles),
            camera_feed: RefCell::new(None),
            geometry_interpolation: RefCell::new(None),
            weapon_tiles,
            access_hand_tile,
            laser_hand_tile,
            overlay_quote: RefCell::new(None),
            full_status_bar: Cell::new(true),
            map_hack_defs: Vec::new(),
            map_hack_files: Vec::new(),
            map_hack_sprite_angles: RefCell::new(BTreeMap::new()),
        })
    }

    /// Load a base GRP plus loose DEF and image assets supplied by a host.
    /// This keeps filesystem access outside `room-duke` while allowing web and
    /// desktop hosts to pass the files they have already mounted from a mod.
    pub fn from_grp_layers_with_mod_assets(
        grp_layers: &[&[u8]],
        tables_dat: Option<&[u8]>,
        def_files: &[(&str, &[u8])],
        tile_assets: &[(&str, &[u8])],
    ) -> Result<Self, String> {
        Self::from_grp_layers_with_mod_assets_and_mhks(
            grp_layers,
            tables_dat,
            def_files,
            tile_assets,
            &[],
        )
    }

    /// Load mod assets and `.mhk` files for map-specific render adjustments.
    pub fn from_grp_layers_with_mod_assets_and_mhks(
        grp_layers: &[&[u8]],
        tables_dat: Option<&[u8]>,
        def_files: &[(&str, &[u8])],
        tile_assets: &[(&str, &[u8])],
        mhk_files: &[(&str, &[u8])],
    ) -> Result<Self, String> {
        let renderer = Self::from_grp_layers_with_tables(grp_layers, tables_dat)?;
        let grp = GrpStack::parse_layers(grp_layers).map_err(|error| format!("GRP: {error}"))?;
        let mut all_defs: Vec<(&str, &[u8])> = grp
            .names()
            .filter(|name| name.to_ascii_lowercase().ends_with(".def"))
            .filter_map(|name| grp.get(name).map(|bytes| (name, bytes)))
            .collect();
        all_defs.extend_from_slice(def_files);
        let mut all_assets: Vec<(&str, &[u8])> = grp
            .names()
            .filter_map(|name| grp.get(name).map(|bytes| (name, bytes)))
            .collect();
        all_assets.extend_from_slice(tile_assets);
        renderer.load_def_tile_assets(&all_defs, &all_assets)?;
        let mut all_mhks: Vec<(&str, &[u8])> = grp
            .names()
            .filter(|name| name.to_ascii_lowercase().ends_with(".mhk"))
            .filter_map(|name| grp.get(name).map(|bytes| (name, bytes)))
            .collect();
        all_mhks.extend_from_slice(mhk_files);
        let mut renderer = renderer;
        renderer.map_hack_defs = all_defs
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec()))
            .collect();
        renderer.map_hack_files = all_mhks
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec()))
            .collect();
        Ok(renderer)
    }

    /// Match and load a map-specific MHK by its DEF map filename and MD4.
    /// Returns the selected MHK resource name, if the mod provides one.
    pub fn load_map_hack_for_map(
        &self,
        map_file: &str,
        map_bytes: &[u8],
    ) -> Result<Option<String>, String> {
        let def_refs = self
            .map_hack_defs
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>();
        let mhk_refs = self
            .map_hack_files
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>();
        let selected =
            room_build::map_hack::select_map_hack(&def_refs, &mhk_refs, map_file, map_bytes)
                .map_err(|error| error.to_string())?;
        let Some((name, document)) = selected else {
            self.map_hack_sprite_angles.borrow_mut().clear();
            return Ok(None);
        };
        let offsets = document
            .sprite_angle_offsets()
            .map_err(|error| error.to_string())?;
        *self.map_hack_sprite_angles.borrow_mut() = offsets;
        Ok(Some(name))
    }

    fn load_def_tile_assets(
        &self,
        def_files: &[(&str, &[u8])],
        tile_assets: &[(&str, &[u8])],
    ) -> Result<(), String> {
        enum TileAction {
            Import(room_build::def::ImportedTile),
            Operation(room_build::def::TileOperation),
        }
        let mut actions = Vec::new();
        for (name, bytes) in def_files {
            let source = String::from_utf8_lossy(bytes);
            let document = room_build::def::DefDocument::parse(&source)
                .map_err(|error| format!("{name}: {error}"))?;
            let imported = document
                .import_high_res_tiles(tile_assets, &self.palette)
                .map_err(|error| format!("{name}: {error}"))?;
            actions.extend(imported.into_iter().map(TileAction::Import));
            let operations = document
                .tile_operations()
                .map_err(|error| format!("{name}: {error}"))?;
            actions.extend(operations.into_iter().map(TileAction::Operation));
        }
        let mut soft_tiles = self.tiles.borrow_mut();
        for action in actions {
            match action {
                TileAction::Import(tile) => soft_tiles.insert_with_offset(
                    tile.tile,
                    tile.width,
                    tile.height,
                    tile.pixels,
                    tile.x_offset,
                    tile.y_offset,
                ),
                TileAction::Operation(room_build::def::TileOperation::Copy {
                    destination,
                    source,
                }) => {
                    if !soft_tiles.copy_tile(source, destination) {
                        return Err(format!("DEF copytile source tile {source} is not loaded"));
                    }
                }
                TileAction::Operation(room_build::def::TileOperation::Dummy {
                    tile,
                    width,
                    height,
                }) => soft_tiles.insert_dummy(tile, width, height),
                TileAction::Operation(room_build::def::TileOperation::DummyRange {
                    start,
                    end,
                    width,
                    height,
                }) => {
                    for tile in start..=end {
                        soft_tiles.insert_dummy(tile, width, height);
                    }
                }
                TileAction::Operation(room_build::def::TileOperation::AnimationRange {
                    start,
                    end,
                    speed,
                    animation,
                }) => soft_tiles.set_animation_range(start, end, speed, animation),
            }
        }
        Ok(())
    }

    /// Set Atomic `curbrightness` (0..15). The native F11 control uses rows
    /// 0..14 in steps of two; the setup menu can reach row 15.
    pub fn set_brightness(&self, row: u8) {
        self.brightness.set(row.min(15));
    }

    /// Current Atomic BRITABLE row.
    #[must_use]
    pub fn brightness(&self) -> u8 {
        self.brightness.get()
    }

    fn brighten_channel(&self, value: u8) -> u8 {
        let six_bit = usize::from(value >> 2);
        self.brightness_table[usize::from(self.brightness.get())][six_bit].saturating_mul(4)
    }

    /// Chocolate `player.c::setpal`'s full-screen palette for the player view.
    fn screen_palette_for_player<'a>(
        &'a self,
        world: &BuildWorld,
        player: &Player,
    ) -> Option<&'a [u8; 768]> {
        if player.game_palette != 0 {
            let palette_id = u8::try_from(player.game_palette).ok();
            if let Some(palettes) = self.base_palettes.as_ref() {
                match palette_id {
                    Some(1) => return Some(&palettes.water),
                    Some(2) => return Some(&palettes.slime),
                    Some(3) => return Some(&palettes.drealms),
                    Some(4) => return Some(&palettes.title),
                    Some(5) => return Some(&palettes.ending),
                    _ => {}
                }
            }
            if let Some(palette_id) = palette_id
                && let Some(palette) = self.game_palette_tables.get(&palette_id)
            {
                return Some(palette);
            }
            // Unknown or unavailable explicit palettes fall back to the base
            // PALETTE.DAT instead of reapplying the sector water/heat effect.
            return None;
        }
        let palettes = self.base_palettes.as_ref()?;
        if player.heat_on {
            return Some(&palettes.slime);
        }
        let sector = world
            .sectors
            .get(usize::try_from(player.cursectnum).ok()?)?;
        if (200..=202).contains(&sector.ceilingpicnum) {
            Some(&palettes.slime)
        } else if sector.lotag == 2 {
            Some(&palettes.water)
        } else {
            None
        }
    }

    /// Remap one shell/UI RGB sample through the active full-screen palette.
    /// Returns `None` when the player is using Atomic's base palette.
    #[must_use]
    pub fn remap_screen_rgb(
        &self,
        world: &BuildWorld,
        player: &Player,
        rgb: [u8; 3],
    ) -> Option<[u8; 3]> {
        let selected = self.screen_palette_for_player(world, player)?;
        let index = self.palette.nearest(rgb.map(u32::from));
        let start = usize::from(index) * 3;
        Some([
            self.brighten_channel(selected[start]),
            self.brighten_channel(selected[start + 1]),
            self.brighten_channel(selected[start + 2]),
        ])
    }

    /// Render the current Build world and player camera as row-major RGBA8.
    ///
    /// Width and height must be non-zero. The output preserves the ART's
    /// palette colors and uses nearest-neighbor sampling in the soft renderer.
    #[must_use]
    pub fn render_rgba(
        &self,
        world: &BuildWorld,
        player: &Player,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.render_rgba_at(world, player, width, height, 0)
    }

    /// Render at a deterministic 30 Hz Duke tic; ART animation advances on
    /// Build's 120 Hz clock, four animation clocks per game tic.
    #[must_use]
    pub fn render_rgba_at(
        &self,
        world: &BuildWorld,
        player: &Player,
        width: u32,
        height: u32,
        tic: u32,
    ) -> Vec<u8> {
        self.render_rgba_at_with_quake(world, player, width, height, tic, 0)
    }

    /// Render with previous-to-current tic interpolation for moving geometry.
    /// This is a presentation-only copy: simulation positions are never
    /// modified. `fraction_milli` is clamped to the inclusive range 0..=1000.
    #[must_use]
    pub fn render_rgba_interpolated(
        &self,
        world: &BuildWorld,
        player: &Player,
        width: u32,
        height: u32,
        tic: u32,
        earthquake_time: u8,
        fraction_milli: u32,
    ) -> Vec<u8> {
        let wall_count = world.numwalls.max(0) as usize;
        let sector_count = world.numsectors.max(0) as usize;
        let live_walls: Vec<_> = world
            .walls
            .iter()
            .take(wall_count)
            .map(|wall| (wall.x, wall.y))
            .collect();
        let live_planes: Vec<_> = world
            .sectors
            .iter()
            .take(sector_count)
            .map(|sector| (sector.floorz, sector.ceilingz))
            .collect();
        let interpolation = {
            let mut cache = self.geometry_interpolation.borrow_mut();
            let cache = cache.get_or_insert_with(|| GeometryInterpolation {
                tic,
                previous_walls: live_walls.clone(),
                current_walls: live_walls.clone(),
                previous_planes: live_planes.clone(),
                current_planes: live_planes.clone(),
            });
            if tic != cache.tic
                || cache.current_walls.len() != live_walls.len()
                || cache.current_planes.len() != live_planes.len()
            {
                let discontinuity = tic < cache.tic
                    || cache.current_walls.len() != live_walls.len()
                    || cache.current_planes.len() != live_planes.len();
                cache.previous_walls = if discontinuity {
                    live_walls.clone()
                } else {
                    cache.current_walls.clone()
                };
                cache.previous_planes = if discontinuity {
                    live_planes.clone()
                } else {
                    cache.current_planes.clone()
                };
                cache.current_walls = live_walls;
                cache.current_planes = live_planes;
                cache.tic = tic;
            } else {
                // Same-tic actor/sprite updates can still change a wall or
                // plane before the next draw; retain the completed-tic pose.
                cache.current_walls.clone_from(&live_walls);
                cache.current_planes.clone_from(&live_planes);
            }
            cache.clone()
        };
        let fraction = i64::from(fraction_milli.min(1000));
        let blend = |a: i32, b: i32| {
            let delta = i64::from(b) - i64::from(a);
            (i64::from(a) + delta * fraction / 1000) as i32
        };
        let mut render_world = world.clone();
        for (index, wall) in render_world.walls.iter_mut().take(wall_count).enumerate() {
            if let (Some(&(px, py)), Some(&(cx, cy))) = (
                interpolation.previous_walls.get(index),
                interpolation.current_walls.get(index),
            ) {
                wall.x = blend(px, cx);
                wall.y = blend(py, cy);
            }
        }
        for (index, sector) in render_world
            .sectors
            .iter_mut()
            .take(sector_count)
            .enumerate()
        {
            if let (Some(&(pf, pc)), Some(&(cf, cc))) = (
                interpolation.previous_planes.get(index),
                interpolation.current_planes.get(index),
            ) {
                sector.floorz = blend(pf, cf);
                sector.ceilingz = blend(pc, cc);
            }
        }
        self.render_rgba_at_with_quake(&render_world, player, width, height, tic, earthquake_time)
    }

    /// Render with Chocolate's grounded-player earthquake camera shake.
    #[must_use]
    pub fn render_rgba_at_with_quake(
        &self,
        world: &BuildWorld,
        player: &Player,
        width: u32,
        height: u32,
        tic: u32,
        earthquake_time: u8,
    ) -> Vec<u8> {
        let original_tiles = player.remote_camera.map(|_| self.tiles.borrow().clone());
        let mut camera_world = None;
        if let Some((camera_x, camera_y, camera_z, camera_ang, camera_sector)) =
            player.remote_camera
        {
            let Some(camera_index) = world
                .sprites
                .iter()
                .enumerate()
                .find(|(_, sprite)| {
                    sprite.picnum == crate::spawn::CAMERA1
                        && sprite.x == camera_x
                        && sprite.y == camera_y
                        && sprite.z == camera_z
                        && sprite.ang == camera_ang
                        && sprite.sectnum == camera_sector
                })
                .map(|(index, _)| index)
            else {
                return self.render_rgba_at_with_quake(
                    world,
                    &Player {
                        remote_camera: None,
                        ..player.clone()
                    },
                    width,
                    height,
                    tic,
                    earthquake_time,
                );
            };
            let mut monitor_world = world.clone();
            for sprite in monitor_world.sprites.iter_mut() {
                if matches!(
                    sprite.picnum,
                    crate::spawn::VIEWSCREEN | crate::spawn::VIEWSCREEN2
                ) && sprite.owner == camera_index as i16
                {
                    sprite.picnum = crate::spawn::STATIC;
                }
            }
            let should_refresh = self.camera_feed.borrow().as_ref().is_none_or(|cache| {
                cache.camera_index != camera_index
                    || tic < cache.last_refresh_tic
                    || tic.wrapping_sub(cache.last_refresh_tic) >= 12
            });
            if should_refresh {
                let mut camera_player = player.clone();
                camera_player.posx = camera_x;
                camera_player.posy = camera_y;
                camera_player.posz = camera_z;
                camera_player.ang = camera_ang;
                camera_player.cursectnum = camera_sector;
                camera_player.remote_camera = None;
                camera_player.over_shoulder_on = false;
                camera_player.tip_timer = camera_player.tip_timer.max(1);
                let feed =
                    self.render_rgba_at_with_quake(&monitor_world, &camera_player, 64, 48, tic, 0);
                let mut indexed = vec![0_u8; 64 * 48];
                for y in 0..48_usize {
                    for x in 0..64_usize {
                        let rgba = (y * 64 + x) * 4;
                        indexed[x * 48 + y] = self.palette.nearest([
                            u32::from(feed[rgba]),
                            u32::from(feed[rgba + 1]),
                            u32::from(feed[rgba + 2]),
                        ]);
                    }
                }
                *self.camera_feed.borrow_mut() = Some(CameraFeedCache {
                    camera_index,
                    last_refresh_tic: tic,
                    indexed_pixels: indexed,
                });
            }
            if let Some(cache) = self.camera_feed.borrow().as_ref() {
                self.tiles.borrow_mut().insert(
                    crate::spawn::STATIC,
                    64,
                    48,
                    cache.indexed_pixels.clone(),
                );
            }
            camera_world = Some(monitor_world);
        }
        let render_world = camera_world.as_ref().unwrap_or(world);
        let mut frame = SoftFrame::new(width, height);
        let mut view_z = player.posz;
        let mut view_ang = player.view_ang();
        let mut view_x = player.posx;
        let mut view_y = player.posy;
        let mut view_sector = player.cursectnum;
        if let Some((x, y, z, angle, sector)) = player.remote_camera {
            view_x = x;
            view_y = y;
            view_z = z;
            view_ang = angle;
            view_sector = sector;
        } else if player.over_shoulder_on {
            let walls = world.wall_xy();
            let sectors = world.sector_walls();
            // Chocolate `view()` starts at the player and hitscans along the
            // rear camera ray. Its `cameradist` is Q16, capped at one Build
            // unit (65536); wall hits shorten that distance before projection.
            let (sin, cos) = room_build_engine::helpers::sincos_q14(i32::from(view_ang));
            let ray_x = -(cos >> 4);
            let ray_y = -(sin >> 4);
            let ray_z = (i32::from(player.view_horizon()) - 100) * 128;
            let sector_z = world
                .sectors
                .iter()
                .take(world.numsectors.max(0) as usize)
                .map(room_build_engine::types::SectorT::z)
                .collect::<Vec<_>>();
            let sprites = world
                .sprites
                .iter()
                .enumerate()
                .filter_map(|(index, sprite)| {
                    if sprite.sectnum < 0
                        || world.lists.sprite_stat.get(index).is_none_or(|status| {
                            *status >= room_build_engine::world::MAXSTATUS as i16
                        })
                    {
                        return None;
                    }
                    let (width, height) = self
                        .tiles
                        .borrow()
                        .dimensions(sprite.picnum)
                        .unwrap_or_default();
                    let mut cstat = sprite.cstat;
                    if i16::try_from(index).ok() == Some(player.sprite_i) {
                        // Chocolate `view()` clears the camera player's face
                        // and blocking bits before the rearward CLIPMASK1 ray.
                        cstat &= !0x101;
                    }
                    Some(room_build_engine::geom::SpriteHit {
                        x: sprite.x,
                        y: sprite.y,
                        z: sprite.z,
                        sectnum: sprite.sectnum,
                        clipdist: i32::from(sprite.clipdist) << 2,
                        cstat,
                        face_span: i32::from(width) * i32::from(sprite.xrepeat),
                        face_height: (i32::from(height) * i32::from(sprite.yrepeat)) << 2,
                    })
                })
                .collect::<Vec<_>>();
            let hit = room_build_engine::geom::hitscan_full(
                player.posx,
                player.posy,
                player.posz,
                player.cursectnum,
                ray_x,
                ray_y,
                ray_z,
                (256 << 16) + 64,
                &sectors,
                &walls,
                &sprites,
                Some(&sector_z),
            );
            let has_hit = hit.hitsect >= 0;
            let (dx, dy) = if has_hit {
                (
                    hit.hitx.wrapping_sub(player.posx),
                    hit.hity.wrapping_sub(player.posy),
                )
            } else {
                (ray_x, ray_y)
            };
            let dominant_ray = if ray_x.unsigned_abs() >= ray_y.unsigned_abs() {
                ray_x
            } else {
                ray_y
            };
            let dominant_delta = if ray_x.unsigned_abs() >= ray_y.unsigned_abs() {
                dx
            } else {
                dy
            };
            let hit_distance = if has_hit && dominant_ray != 0 {
                (i64::from(dominant_delta) << 16) / i64::from(dominant_ray)
            } else {
                i64::MAX
            };
            // Chocolate reduces cameradist when the traced surface lies
            // before the one-unit Q16 maximum. Stay four map units short.
            let camera_distance = if (0..65_536).contains(&hit_distance) {
                hit_distance.saturating_sub(256).max(0)
            } else {
                65_536
            };
            let camera_distance =
                camera_distance.min(i64::from(player.shoulder_camera_distance.clamp(0, 65_536)));
            view_x = player.posx.wrapping_add(
                i32::try_from(i64::from(ray_x).saturating_mul(camera_distance) >> 16)
                    .unwrap_or_default(),
            );
            view_y = player.posy.wrapping_add(
                i32::try_from(i64::from(ray_y).saturating_mul(camera_distance) >> 16)
                    .unwrap_or_default(),
            );
            view_z = player.posz.wrapping_add(
                i32::try_from(i64::from(ray_z).saturating_mul(camera_distance) >> 16)
                    .unwrap_or_default(),
            );
            room_build_engine::geom::updatesector(
                view_x,
                view_y,
                &mut view_sector,
                &sectors,
                &walls,
            );
        }
        if earthquake_time > 0 && player.on_ground {
            view_z = view_z.wrapping_add(256 - ((i32::from(earthquake_time) & 1) << 9));
            view_ang = view_ang.wrapping_add(((2 - (i32::from(earthquake_time) & 2)) << 2) as i16);
        }
        let view: BuildView = render_world.build_view(
            view_x,
            view_y,
            view_z,
            view_ang,
            player.view_horizon(),
            view_sector,
        );
        let map_hack_angles = self.map_hack_sprite_angles.borrow();
        room_build_engine::soft::render_fp_textured_at_with_sprite_angle_offsets(
            render_world,
            &view,
            &mut frame,
            &self.tiles.borrow(),
            tic.wrapping_mul(4),
            (!map_hack_angles.is_empty()).then_some(&map_hack_angles),
        );
        // Chocolate's `displayweapon` returns as soon as `animatetip` draws
        // the TIP tile, so the weapon is hidden for the complete tip pose.
        if player.tip_timer == 0 && !player.over_shoulder_on && player.remote_camera.is_none() {
            if !self.draw_access_hand(&mut frame, render_world, player) {
                self.draw_weapon(&mut frame, render_world, player);
            }
        }
        // Chocolate's dead-player `processinput` keeps the camera tilted by
        // combining `dead_flag` with the player's floor-relative height. The
        // simulation stores the random death direction in `dead_flag`; derive
        // the live roll here so rendering remains a read-only view operation.
        let screen_roll = if player.dead_flag != 0 && player.remote_camera.is_none() {
            render_world
                .sectors
                .get(usize::try_from(player.cursectnum).unwrap_or(usize::MAX))
                .map(|sector| {
                    (i32::from(player.dead_flag)
                        .wrapping_add(sector.floorz.wrapping_add(player.posz) >> 7)
                        & 2047) as i16
                })
                .unwrap_or(player.rotscrnang)
        } else {
            player.rotscrnang
        };
        let mut palette_pixels = rotate_palette_frame(&frame.pixels, width, height, screen_roll);
        // The status HUD is screen space: it never rolls with the view.
        if player.remote_camera.is_none() {
            let mut hud = SoftFrame {
                width,
                height,
                pixels: std::mem::take(&mut palette_pixels),
            };
            self.draw_mini_hud(&mut hud, player);
            palette_pixels = hud.pixels;
        }
        // Chocolate `player.c::setpal` chooses the screen DAC palette from
        // heat vision, the FLOORSLIME ceiling, or an underwater sector.
        // Keep this render-only: the snapshot and playsim remain untouched.
        let screen_palette = self.screen_palette_for_player(render_world, player);
        let mut rgba = Vec::with_capacity(palette_pixels.len() * 4);
        for index in palette_pixels {
            let i = usize::from(index) * 3;
            let rgb = screen_palette.unwrap_or(&self.palette.rgb);
            rgba.extend_from_slice(&[
                self.brighten_channel(rgb[i]),
                self.brighten_channel(rgb[i + 1]),
                self.brighten_channel(rgb[i + 2]),
                255,
            ]);
        }
        if let Some(tiles) = original_tiles {
            *self.tiles.borrow_mut() = tiles;
        }
        rgba
    }

    /// ART tile size, `(0, 0)` when the tile is absent (editor picking).
    #[must_use]
    pub fn tile_dimensions(&self, picnum: i16) -> (u16, u16) {
        self.tiles.borrow().dimensions(picnum).unwrap_or_default()
    }

    /// Editor preview: draw `world` from a free camera with no weapon, HUD or
    /// player effects. Sprites are drawn as stored in the map (markers such as
    /// SE/ACTIVATOR are left visible so they can be selected).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn render_editor_view(
        &self,
        world: &BuildWorld,
        x: i32,
        y: i32,
        z: i32,
        ang: i16,
        horiz: i16,
        sector: i16,
        width: u32,
        height: u32,
        tic: u32,
    ) -> Vec<u8> {
        let mut frame = SoftFrame::new(width, height);
        let view: BuildView = world.build_view(x, y, z, ang, horiz, sector);
        render_fp_textured_at(
            world,
            &view,
            &mut frame,
            &self.tiles.borrow(),
            tic.wrapping_mul(4),
        );
        let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
        for index in frame.pixels {
            let i = usize::from(index) * 3;
            let rgb = &self.palette.rgb;
            rgba.extend_from_slice(&[
                self.brighten_channel(rgb[i]),
                self.brighten_channel(rgb[i + 1]),
                self.brighten_channel(rgb[i + 2]),
                255,
            ]);
        }
        rgba
    }

    /// Render with a local shoulder-camera override without changing the
    /// deterministic player/session state. Used by shells whose camera
    /// controls are presentation-local (notably browser lockstep clients).
    #[must_use]
    pub fn render_rgba_at_with_quake_view(
        &self,
        world: &BuildWorld,
        player: &Player,
        width: u32,
        height: u32,
        tic: u32,
        earthquake_time: u8,
        shoulder_view: bool,
    ) -> Vec<u8> {
        let mut view_player = player.clone();
        view_player.over_shoulder_on = shoulder_view;
        self.render_rgba_at_with_quake(world, &view_player, width, height, tic, earthquake_time)
    }

    /// Draw a full-screen 2D page (bonus screen, front-end pages) in
    /// Chocolate's 320×200 `rotatesprite` plane and return it as RGBA8.
    #[must_use]
    pub fn render_ui_page(
        &self,
        width: u32,
        height: u32,
        draw: impl FnOnce(&mut crate::ui_page::UiPage<'_>),
    ) -> Vec<u8> {
        let tiles = self.tiles.borrow();
        let mut frame = SoftFrame {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize)],
        };
        {
            let mut page = crate::ui_page::UiPage::new(&tiles, &mut frame);
            draw(&mut page);
        }
        let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
        for index in frame.pixels {
            let i = usize::from(index) * 3;
            rgba.extend_from_slice(&[
                self.brighten_channel(self.palette.rgb[i]),
                self.brighten_channel(self.palette.rgb[i + 1]),
                self.brighten_channel(self.palette.rgb[i + 2]),
                255,
            ]);
        }
        rgba
    }

    /// Like [`Self::render_ui_page`] but transparent where nothing was drawn,
    /// for menus laid over the game view.
    #[must_use]
    pub fn render_ui_overlay(
        &self,
        width: u32,
        height: u32,
        draw: impl FnOnce(&mut crate::ui_page::UiPage<'_>),
    ) -> Vec<u8> {
        let tiles = self.tiles.borrow();
        let mut frame = SoftFrame {
            width,
            height,
            pixels: vec![room_build::art::TRANSPARENT; (width as usize) * (height as usize)],
        };
        {
            let mut page = crate::ui_page::UiPage::new(&tiles, &mut frame);
            draw(&mut page);
        }
        let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
        for index in frame.pixels {
            if index == room_build::art::TRANSPARENT {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let i = usize::from(index) * 3;
            rgba.extend_from_slice(&[
                self.brighten_channel(self.palette.rgb[i]),
                self.brighten_channel(self.palette.rgb[i + 1]),
                self.brighten_channel(self.palette.rgb[i + 2]),
                255,
            ]);
        }
        rgba
    }

    /// Render one frame of the native Duke menu over the game (`in_game`)
    /// or over the MENUSCREEN backdrop at the title.
    #[must_use]
    pub fn render_menu(
        &self,
        width: u32,
        height: u32,
        menu: &crate::menu::DukeMenu,
        clock: i64,
        backdrop: bool,
    ) -> Vec<u8> {
        let draw = |page: &mut crate::ui_page::UiPage<'_>| {
            if backdrop {
                page.clear(0);
                // Chocolate title backdrop: MENUSCREEN, shade 16, centred.
                page.sprite(160, 100, 2456, 16, 0, false, true);
            }
            menu.draw(page, clock);
        };
        if backdrop {
            self.render_ui_page(width, height, draw)
        } else {
            self.render_ui_overlay(width, height, draw)
        }
    }

    /// Render one frame of the single-player bonus screen as RGBA8.
    #[must_use]
    pub fn render_bonus_screen(
        &self,
        width: u32,
        height: u32,
        bonus: &crate::bonus::BonusScreen,
    ) -> Vec<u8> {
        self.render_ui_page(width, height, |page| bonus.draw(page))
    }

    /// Choose the full status bar (`true`) or the mini HUD (`false`).
    pub fn set_full_status_bar(&self, on: bool) {
        self.full_status_bar.set(on);
    }

    /// Whether the full status bar is drawn.
    #[must_use]
    pub fn full_status_bar(&self) -> bool {
        self.full_status_bar.get()
    }

    /// Set the on-screen message drawn with the next frames (`None` clears).
    pub fn set_overlay_quote(&self, quote: Option<&str>) {
        *self.overlay_quote.borrow_mut() = quote.map(str::to_owned);
    }

    /// Native-size palette indices of an ART tile, row-major
    /// (`room_build::art::TRANSPARENT` = hole). For shells that build UI
    /// fonts/patches from Duke's own art (menus, quotes).
    #[must_use]
    pub fn tile_indices(&self, picnum: i16) -> Option<(u16, u16, Vec<u8>)> {
        let tiles = self.tiles.borrow();
        let (w, h) = tiles.dimensions(picnum)?;
        let mut out = Vec::with_capacity(usize::from(w) * usize::from(h));
        for y in 0..u32::from(h) {
            for x in 0..u32::from(w) {
                out.push(
                    tiles
                        .sample(picnum, x, y)
                        .unwrap_or(room_build::art::TRANSPARENT),
                );
            }
        }
        Some((w as u16, h as u16, out))
    }

    /// The game palette (with the current brightness) as RGB triples.
    #[must_use]
    pub fn ui_palette_rgb(&self) -> [[u8; 3]; 256] {
        std::array::from_fn(|i| {
            [
                self.brighten_channel(self.palette.rgb[i * 3]),
                self.brighten_channel(self.palette.rgb[i * 3 + 1]),
                self.brighten_channel(self.palette.rgb[i * 3 + 2]),
            ]
        })
    }

    /// Rasterize a palette-indexed Atomic UI tile into framebuffer-sized RGBA8.
    ///
    /// Duke's `rotatesprite` HUD uses a 320×240 coordinate plane; preserve the
    /// ART pixels with nearest-neighbor enlargement and Build's transparent 255.
    #[must_use]
    pub fn ui_tile_rgba(
        &self,
        picnum: i16,
        framebuffer_width: u32,
        framebuffer_height: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        self.ui_tile_rgba_with_palette(picnum, framebuffer_width, framebuffer_height, None)
    }

    /// Rasterize an Atomic UI ART tile using the active player full-screen
    /// palette, as Chocolate's hardware palette does for the whole frame.
    #[must_use]
    pub fn ui_tile_rgba_for_player(
        &self,
        picnum: i16,
        framebuffer_width: u32,
        framebuffer_height: u32,
        world: &BuildWorld,
        player: &Player,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let palette = self.screen_palette_for_player(world, player);
        self.ui_tile_rgba_with_palette(picnum, framebuffer_width, framebuffer_height, palette)
    }

    fn ui_tile_rgba_with_palette(
        &self,
        picnum: i16,
        framebuffer_width: u32,
        framebuffer_height: u32,
        screen_palette: Option<&[u8; 768]>,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let tiles = self.tiles.borrow();
        let (tile_width, tile_height) = tiles.dimensions(picnum)?;
        let width = u32::try_from(u64::from(tile_width) * u64::from(framebuffer_width) / 320)
            .ok()?
            .max(1);
        let height = u32::try_from(u64::from(tile_height) * u64::from(framebuffer_height) / 240)
            .ok()?
            .max(1);
        let capacity = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
        let mut rgba = Vec::with_capacity(capacity);
        for y in 0..height {
            let source_y =
                u32::try_from(u64::from(y) * u64::from(tile_height) / u64::from(height)).ok()?;
            for x in 0..width {
                let source_x =
                    u32::try_from(u64::from(x) * u64::from(tile_width) / u64::from(width)).ok()?;
                let index = tiles.sample(picnum, source_x, source_y)?;
                let rgb_index = usize::from(index) * 3;
                let palette = screen_palette.map_or(&self.palette.rgb, |palette| palette);
                rgba.extend_from_slice(&[
                    self.brighten_channel(palette[rgb_index]),
                    self.brighten_channel(palette[rgb_index + 1]),
                    self.brighten_channel(palette[rgb_index + 2]),
                    if index == 255 { 0 } else { 255 },
                ]);
            }
        }
        Some((width, height, rgba))
    }

    /// Chocolate `coolgaugetext` full-screen mini status bar (`screen_size`
    /// 4): health and ammo boxes with DIGITALNUM counters, plus the selected
    /// inventory item with its THREEBYFIVE amount. Coordinates are Atomic's
    /// 320×200 `rotatesprite` plane with top-left anchoring (flag 16).
    fn draw_mini_hud(&self, frame: &mut SoftFrame, player: &Player) {
        const HEALTHBOX: i16 = 30;
        const AMMOBOX: i16 = 31;
        const INVENTORYBOX: i16 = 33;
        const DIGITALNUM: i16 = 2472;
        const THREEBYFIVE: i16 = 3010;
        const HANDBOMB_WEAPON: usize = 5;
        const HANDREMOTE_WEAPON: i16 = 10;
        let tiles = self.tiles.borrow();
        let sx = f64::from(frame.width) / 320.0;
        let sy = f64::from(frame.height) / 200.0;
        let blit = |frame: &mut SoftFrame, pic: i16, x: i32, y: i32, shade: i8, pal: u8| {
            let Some((tw, th)) = tiles.dimensions(pic) else {
                return;
            };
            let left = (f64::from(x) * sx).round() as i32;
            let top = (f64::from(y) * sy).round() as i32;
            let dw = (f64::from(tw) * sx).round().max(1.0) as i32;
            let dh = (f64::from(th) * sy).round().max(1.0) as i32;
            for yy in 0..dh {
                let dy = top + yy;
                if dy < 0 || dy >= frame.height as i32 {
                    continue;
                }
                let v = (i64::from(yy) * i64::from(th) / i64::from(dh)) as u32;
                for xx in 0..dw {
                    let dx = left + xx;
                    if dx < 0 || dx >= frame.width as i32 {
                        continue;
                    }
                    let u = (i64::from(xx) * i64::from(tw) / i64::from(dw)) as u32;
                    let Some(p) = tiles.sample(pic, u, v) else {
                        continue;
                    };
                    if p == room_build::art::TRANSPARENT {
                        continue;
                    }
                    let p = tiles.shade_palette(shade, tiles.remap_palette(pal, p));
                    frame.pixels[dy as usize * frame.width as usize + dx as usize] = p;
                }
            }
        };
        // Chocolate `ud.crosshair`: CROSSHAIR centred at (160 - look_ang/2, 100).
        if player.dead_flag == 0 {
            const CROSSHAIR: i16 = 2523;
            if let Some((tw, th)) = tiles.dimensions(CROSSHAIR) {
                let cx = 160 - (i32::from(player.look_ang) >> 1);
                blit(
                    frame,
                    CROSSHAIR,
                    cx - i32::from(tw) / 2,
                    100 - i32::from(th) / 2,
                    0,
                    0,
                );
            }
        }
        // `digitalnumber`: centred on x, glyph advance = width + 1.
        let digital = |frame: &mut SoftFrame, x: i32, y: i32, n: i32| {
            let text = n.to_string();
            let glyph = |c: u8| {
                if c == b'-' {
                    None
                } else {
                    Some(DIGITALNUM + i16::from(c - b'0'))
                }
            };
            let width: i32 = text
                .bytes()
                .filter_map(glyph)
                .map(|p| tiles.dimensions(p).map_or(0, |(w, _)| i32::from(w)) + 1)
                .sum();
            let mut cx = x - (width >> 1);
            for p in text.bytes().filter_map(glyph) {
                blit(frame, p, cx, y, -16, 0);
                cx += tiles.dimensions(p).map_or(0, |(w, _)| i32::from(w)) + 1;
            }
        };
        // `minitext`: MINIFONT glyphs, 4 px advance, spaces 5 px.
        let minitext = |frame: &mut SoftFrame, mut x: i32, y: i32, text: &str, pal: u8| {
            const MINIFONT: i16 = 3072;
            for c in text.bytes().map(|c| c.to_ascii_uppercase()) {
                if c == b' ' {
                    x += 5;
                    continue;
                }
                blit(
                    frame,
                    MINIFONT + i16::from(c) - i16::from(b'!'),
                    x,
                    y,
                    0,
                    pal,
                );
                x += 4;
            }
        };
        let inv_icon = |icon: u8| match icon {
            1 => Some(2460), // FIRSTAID_ICON
            2 => Some(2469), // STEROIDS_ICON
            3 => Some(2470), // HOLODUKE_ICON
            4 => Some(2467), // JETPACK_ICON
            5 => Some(2461), // HEAT_ICON
            6 => Some(2468), // AIRTANK_ICON
            7 => Some(2463), // BOOT_ICON
            _ => None,
        };
        let inv_amount = |icon: u8| match icon {
            1 => i32::from(player.firstaid_amount),
            2 => (i32::from(player.steroids_amount) + 3) >> 2,
            3 => (i32::from(player.holoduke_amount) + 15) / 24,
            4 => (i32::from(player.jetpack_amount) + 15) >> 4,
            5 => i32::from(player.heat_amount) / 12,
            6 => (i32::from(player.scuba_amount) + 63) >> 6,
            7 => i32::from(player.boot_amount) >> 1,
            _ => 0,
        };
        // Chocolate `invennum`: THREEBYFIVE digits right-aligned at x+4,
        // drawn without flag 16 (centre-anchored).
        let invennum = |frame: &mut SoftFrame, x: i32, y: i32, amount: i32| {
            let digits = (amount.clamp(0, 255) as u8).to_string();
            let start = match digits.len() {
                3 => x - 4,
                2 => x,
                _ => x + 4,
            };
            for (k, c) in digits.bytes().enumerate() {
                let pic = THREEBYFIVE + i16::from(c - b'0');
                let (w, h) = tiles
                    .dimensions(pic)
                    .map_or((0, 0), |(w, h)| (i32::from(w), i32::from(h)));
                let (ox, oy) = tiles.sprite_offset(pic);
                blit(
                    frame,
                    pic,
                    start + 4 * k as i32 - (w >> 1) - i32::from(ox),
                    y - (h >> 1) - i32::from(oy),
                    0,
                    0,
                );
            }
        };
        let inv_on = |icon: u8| -> Option<bool> {
            match icon {
                3 => Some(player.holoduke_on >= 0),
                4 => Some(player.jetpack_on != 0),
                5 => Some(player.heat_on),
                _ => None,
            }
        };
        if self.full_status_bar.get() {
            // Chocolate `coolgaugetext` full status bar (`screen_size` 8).
            const BOTTOMSTATUSBAR: i16 = 2462;
            const ACCESS_ICON: i16 = 2471;
            let bar_h = tiles
                .dimensions(BOTTOMSTATUSBAR)
                .map_or(34, |(_, h)| i32::from(h));
            blit(frame, BOTTOMSTATUSBAR, 0, 200 - bar_h, 0, 0);
            if player.got_access & 4 != 0 {
                blit(frame, ACCESS_ICON, 275, 182, 0, 23);
            }
            if player.got_access & 2 != 0 {
                blit(frame, ACCESS_ICON, 288, 182, 0, 21);
            }
            if player.got_access & 1 != 0 {
                blit(frame, ACCESS_ICON, 281, 189, 0, 0);
            }
            // `weapon_amounts` (x 96, y 182): index digit in pal 7, current
            // weapon bright, unowned weapons dimmed.
            let cw = player.curr_weapon;
            let shade = |w: i16, bright: i32| -> i32 {
                let owned = player.gotweapon.get(w as usize).copied().unwrap_or(false);
                let cur = cw == w || (w == 5 && cw == 10);
                (if owned || w == 1 { 0 } else { 9 }) + 12 - if cur { bright } else { 0 }
            };
            // `weaponnum` draws with flags 10+128 (no 16): (x, y) is the
            // glyph centre plus its picanm offset, not its top-left corner.
            let tbf = |frame: &mut SoftFrame, d: u8, x: i32, y: i32, ha: i32, pal: u8| {
                let pic = THREEBYFIVE + i16::from(d);
                let (w, h) = tiles
                    .dimensions(pic)
                    .map_or((0, 0), |(w, h)| (i32::from(w), i32::from(h)));
                let (ox, oy) = tiles.sprite_offset(pic);
                blit(
                    frame,
                    pic,
                    x - (w >> 1) - i32::from(ox),
                    y - (h >> 1) - i32::from(oy),
                    ha.clamp(-128, 127) as i8,
                    pal,
                );
            };
            let num = |frame: &mut SoftFrame, n: i32, x: i32, y: i32, ha: i32, slots: i32| {
                let t = n.max(0).to_string();
                let x0 = x + 4 * (slots - t.len() as i32);
                for (k, c) in t.bytes().enumerate() {
                    tbf(frame, c - b'0', x0 + 4 * k as i32, y, ha, 0);
                }
            };
            let weaponnum = |frame: &mut SoftFrame,
                             ind: i32,
                             x: i32,
                             y: i32,
                             n1: i32,
                             n2: i32,
                             ha: i32,
                             wide: bool| {
                tbf(frame, (ind + 1) as u8, x - 7, y, ha - 10, 7);
                if wide {
                    tbf(frame, 10, x - 4, y, ha, 0);
                    tbf(frame, 11, x + 13, y, ha, 0);
                    num(frame, n1, x, y, ha, 3);
                    num(frame, n2, x + 17, y, ha, 3);
                } else {
                    tbf(frame, 10, x - 3, y, ha, 0);
                    tbf(frame, 11, x + 9, y, ha, 0);
                    let (a, b) = (n1.min(99), n2.min(99));
                    num(frame, a, x, y, ha, 2);
                    let t = b.to_string();
                    for (k, c) in t.bytes().enumerate() {
                        tbf(frame, c - b'0', x + 13 + 4 * k as i32, y, ha, 0);
                    }
                }
            };
            let am = |w: usize| i32::from(player.ammo.get(w).copied().unwrap_or(0));
            let mx = |w: i16| i32::from(crate::weapons::max_ammo_amount(w));
            let (x, y) = (96, 182);
            weaponnum(
                frame,
                1,
                x,
                y,
                am(1),
                mx(1),
                12 - 20 * i32::from(cw == 1),
                true,
            );
            weaponnum(frame, 2, x, y + 6, am(2), mx(2), shade(2, 18), true);
            weaponnum(frame, 3, x, y + 12, am(3), mx(3), shade(3, 18), true);
            weaponnum(frame, 4, x + 39, y, am(4), mx(4), shade(4, 19), false);
            let bomb_dim = if am(5) == 0 { 9 } else { 0 };
            weaponnum(
                frame,
                5,
                x + 39,
                y + 6,
                am(5),
                mx(5),
                shade(5, 19).max(12 - 19 * i32::from(cw == 5 || cw == 10))
                    + bomb_dim * i32::from(player.gotweapon[5]),
                false,
            );
            if cw == 11 {
                weaponnum(
                    frame,
                    6,
                    x + 39,
                    y + 12,
                    am(11),
                    mx(11),
                    shade(11, 18),
                    false,
                );
            } else {
                weaponnum(frame, 6, x + 39, y + 12, am(6), mx(6), shade(6, 18), false);
            }
            weaponnum(frame, 7, x + 70, y, am(7), mx(7), shade(7, 18), false);
            weaponnum(frame, 8, x + 70, y + 6, am(8), mx(8), shade(8, 18), false);
            weaponnum(frame, -1, x + 70, y + 12, am(9), mx(9), shade(9, 18), false);
            // Health, armor, current ammo.
            digital(frame, 32, 200 - 17, i32::from(player.last_extra.max(0)));
            digital(frame, 64, 200 - 17, i32::from(player.shield_amount.max(0)));
            if cw != 0 {
                let w = if cw == HANDREMOTE_WEAPON {
                    HANDBOMB_WEAPON
                } else {
                    cw.clamp(0, 11) as usize
                };
                digital(frame, 230 - 22, 200 - 17, i32::from(player.ammo[w]));
            }
            if player.inventory_icon != 0 {
                if let Some(icon) = inv_icon(player.inventory_icon) {
                    blit(frame, icon, 231, 200 - 21, 0, 0);
                }
                minitext(frame, 292 - 30, 190, "%", 6);
                if player.inventory_icon >= 6 {
                    minitext(frame, 284 - 35, 180, "AUTO", 2);
                }
                match inv_on(player.inventory_icon) {
                    Some(true) => minitext(frame, 288 - 30, 180, "ON", 0),
                    Some(false) => minitext(frame, 284 - 30, 180, "OFF", 2),
                    None => {}
                }
                invennum(frame, 284 - 30, 200 - 6, inv_amount(player.inventory_icon));
            }
        } else {
            if player.inventory_icon != 0 {
                blit(frame, INVENTORYBOX, 69, 200 - 30, 0, 21);
            }
            blit(frame, HEALTHBOX, 5, 200 - 28, 0, 21);
            digital(frame, 20, 200 - 17, i32::from(player.last_extra.max(0)));
            blit(frame, AMMOBOX, 37, 200 - 28, 0, 21);
            let weapon = if player.curr_weapon == HANDREMOTE_WEAPON {
                HANDBOMB_WEAPON
            } else {
                player.curr_weapon.clamp(0, 11) as usize
            };
            digital(frame, 53, 200 - 17, i32::from(player.ammo[weapon]));
            if player.inventory_icon != 0 {
                let o = 158;
                let icon = match player.inventory_icon {
                    1 => Some(2460), // FIRSTAID_ICON
                    2 => Some(2469), // STEROIDS_ICON
                    3 => Some(2470), // HOLODUKE_ICON
                    4 => Some(2467), // JETPACK_ICON
                    5 => Some(2461), // HEAT_ICON
                    6 => Some(2468), // AIRTANK_ICON
                    7 => Some(2463), // BOOT_ICON
                    _ => None,
                };
                if let Some(icon) = icon {
                    blit(frame, icon, 231 - o, 200 - 21, 0, 0);
                }
                let amount = match player.inventory_icon {
                    1 => i32::from(player.firstaid_amount),
                    2 => (i32::from(player.steroids_amount) + 3) >> 2,
                    3 => (i32::from(player.holoduke_amount) + 15) / 24,
                    4 => (i32::from(player.jetpack_amount) + 15) >> 4,
                    5 => i32::from(player.heat_amount) / 12,
                    6 => (i32::from(player.scuba_amount) + 63) >> 6,
                    7 => i32::from(player.boot_amount) >> 1,
                    _ => 0,
                };
                invennum(frame, 284 - 30 - o, 200 - 6, amount);
                minitext(frame, 292 - 30 - o, 190, "%", 6);
            }
        }
        // Chocolate `operatefta` → `gametext(160, 0, quote)`: STARTALPHANUM
        // glyphs, centred, digits on an 8 px pitch, spaces 5 px.
        if let Some(quote) = self.overlay_quote.borrow().as_deref() {
            const STARTALPHANUM: i16 = 2822;
            let glyph = |c: u8| {
                (b'!'..=b'~')
                    .contains(&c)
                    .then(|| STARTALPHANUM + i16::from(c - b'!'))
            };
            let advance = |c: u8| -> i32 {
                if c == b' ' {
                    5
                } else if c.is_ascii_digit() {
                    8
                } else {
                    glyph(c)
                        .and_then(|p| tiles.dimensions(p))
                        .map_or(0, |(w, _)| i32::from(w))
                }
            };
            let width: i32 = quote.bytes().map(advance).sum();
            let mut x = 160 - (width >> 1);
            for c in quote.bytes() {
                if let Some(p) = glyph(c) {
                    blit(frame, p, x, 0, 0, 0);
                }
                x += advance(c);
            }
        }
    }

    /// Draw the selected Atomic weapon from its GRP ART tiles.
    fn draw_weapon(&self, frame: &mut SoftFrame, world: &BuildWorld, player: &Player) {
        let body = usize::try_from(player.sprite_i)
            .ok()
            .and_then(|index| world.sprites.get(index))
            .copied()
            .unwrap_or_default();
        let floorpal = world
            .sectors
            .get(usize::try_from(player.cursectnum).unwrap_or(usize::MAX))
            .map_or(0, |sector| sector.floorpal);
        let env = crate::weapon_view::WeaponViewEnv {
            body_shade: body.shade,
            body_pal: body.pal,
            body_xrepeat: if player.sprite_i < 0 {
                42
            } else {
                body.xrepeat
            },
            body_xvel: player.body_xvel,
            body_extra: if player.sprite_i < 0 {
                player.extra
            } else {
                body.extra
            },
            floorpal,
            rand: u32::from(player.kickback_pic as u16).wrapping_mul(2_654_435_761),
        };
        let draws = crate::weapon_view::display_weapon(player, &env);
        // With the full status bar Chocolate's view window is 34 px shorter;
        // `rotatesprite` flag 2 recentres psprites in it (17 px higher).
        let y_shift = if self.full_status_bar.get() { -17 } else { 0 };
        let tiles = self.tiles.borrow();
        let sx = f64::from(frame.width) / 320.0;
        let sy = f64::from(frame.height) / 200.0;
        for d in draws {
            let Some((tw, th)) = tiles.dimensions(d.tile) else {
                continue;
            };
            let (ox, oy) = tiles.sprite_offset(d.tile);
            let (tw_i, th_i) = (i32::from(tw), i32::from(th));
            let xoff = i32::from(ox) + (tw_i >> 1);
            let yoff = i32::from(oy) + (th_i >> 1);
            let left = if d.flip {
                d.x - (tw_i - xoff)
            } else {
                d.x - xoff
            };
            let top = d.y + y_shift - yoff;
            let x0 = (f64::from(left) * sx).round() as i32;
            let y0 = (f64::from(top) * sy).round() as i32;
            let x1 = (f64::from(left + tw_i) * sx).round() as i32;
            let y1 = (f64::from(top + th_i) * sy).round() as i32;
            let (dw, dh) = ((x1 - x0).max(1), (y1 - y0).max(1));
            for yy in 0..dh {
                let dy = y0 + yy;
                if dy < 0 || dy >= frame.height as i32 {
                    continue;
                }
                let v = (i64::from(yy) * i64::from(th) / i64::from(dh)) as u32;
                for xx in 0..dw {
                    let dx = x0 + xx;
                    if dx < 0 || dx >= frame.width as i32 {
                        continue;
                    }
                    let mut u = (i64::from(xx) * i64::from(tw) / i64::from(dw)) as u32;
                    if d.flip {
                        u = u32::from(tw) - 1 - u;
                    }
                    let Some(pixel) = tiles.sample(d.tile, u, v) else {
                        continue;
                    };
                    if pixel == room_build::art::TRANSPARENT {
                        continue;
                    }
                    let pixel = tiles.shade_palette(d.shade, tiles.remap_palette(d.pal, pixel));
                    frame.pixels[dy as usize * frame.width as usize + dx as usize] = pixel;
                }
            }
        }
    }

    /// Draw Chocolate's `animateaccess` first-person key-card animation.
    /// Returns true for every live insertion tic, which suppresses the gun.
    fn draw_access_hand(&self, frame: &mut SoftFrame, world: &BuildWorld, player: &Player) -> bool {
        if player.access_incs == 0 || player.extra <= 0 {
            return false;
        }
        let Some(body) = usize::try_from(player.sprite_i)
            .ok()
            .and_then(|index| world.sprites.get(index))
        else {
            return true;
        };
        let access_y = [
            0_i32, -8, -16, -32, -64, -84, -108, -108, -108, -108, -108, -108, -108, -108, -108,
            -108, -96, -72, -64, -32, -16,
        ];
        let inc = usize::from(player.access_incs).min(access_y.len() - 1);
        let card_y = access_y[inc];
        let looking_arc = card_y + i32::from(player.look_ang).unsigned_abs() as i32 / 9
            - (i32::from(player.hard_landing) << 3);
        let late_laser_frame = inc > 3 && ((inc - 3) >> 3) != 0;
        let pal = usize::try_from(player.access_spritenum)
            .ok()
            .and_then(|index| world.sprites.get(index))
            .map_or(0, |sprite| sprite.pal);
        let tile = if late_laser_frame {
            self.laser_hand_tile.map(|tile| tile + (inc as i16 >> 3))
        } else {
            self.access_hand_tile
        };
        if let Some(tile) = tile {
            let tiles = self.tiles.borrow();
            if let Some((source_w, source_h)) = tiles.dimensions(tile) {
                let center_x = 170 + (i32::from(player.last_avel) >> 4)
                    - (i32::from(player.look_ang) >> 1)
                    + (card_y >> 2);
                let center_y = looking_arc + 266
                    - ((i32::from(player.horiz) - i32::from(player.horizoff)) >> 4);
                let scale_x = f64::from(frame.width) / 320.0;
                // Chocolate places the psprite in the 320×240 Build view;
                // the renderer crops that view to its 320×200 play window.
                let scale_y = f64::from(frame.height) / 240.0;
                let center_x = (f64::from(center_x) * scale_x).round() as i32;
                let center_y = (f64::from(center_y) * scale_y).round() as i32;
                let dest_w = (f64::from(source_w) * scale_x).round().max(1.0) as i32;
                let dest_h = (f64::from(source_h) * scale_y).round().max(1.0) as i32;
                let left = center_x - dest_w / 2;
                let top = center_y - dest_h / 2;
                for y in 0..dest_h {
                    let sy = (i64::from(y) * i64::from(source_h) / i64::from(dest_h)) as u32;
                    let dy = top + y;
                    if dy < 0 || dy >= frame.height as i32 {
                        continue;
                    }
                    for x in 0..dest_w {
                        let dx = left + x;
                        if dx < 0 || dx >= frame.width as i32 {
                            continue;
                        }
                        let sx = (i64::from(x) * i64::from(source_w) / i64::from(dest_w)) as u32;
                        let Some(pixel) = tiles.sample(tile, sx, sy) else {
                            continue;
                        };
                        if pixel != room_build::art::TRANSPARENT {
                            let pixel = tiles.remap_palette(pal, pixel);
                            let pixel = tiles.shade_palette(body.shade.min(24), pixel);
                            frame.pixels[dy as usize * frame.width as usize + dx as usize] = pixel;
                        }
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn mod_def_image_replaces_art_tile_in_renderer() {
        let grp = mini_grp();
        let def = b"tilefromtexture 9000 { file \"resources\\\\tile.bmp\" xoff 3 yoff -2 }\n\
            texture 9005 { pal 0 { file \"resources/texture.bmp\" alphacut 0.32 } }\n\
            copytile 9001 { tile 9000 }\n\
            dummytile 9002 1 1\n\
            dummytilerange 9003 9004 1 1";
        let bmp = bmp_2x1();
        let texture_bmp = bmp_2x1();
        let renderer = DukeSoftRenderer::from_grp_layers_with_mod_assets(
            &[&grp],
            None,
            &[("resources.def", def.as_slice())],
            &[
                ("resources/tile.bmp", bmp.as_slice()),
                ("resources/texture.bmp", texture_bmp.as_slice()),
            ],
        )
        .unwrap();

        assert_eq!(renderer.tile_dimensions(9000), (2, 1));
        assert_eq!(renderer.tile_dimensions(9001), (2, 1));
        assert_eq!(renderer.tile_dimensions(9002), (1, 1));
        assert_eq!(renderer.tile_dimensions(9004), (1, 1));
        assert_eq!(renderer.tile_dimensions(9005), (2, 1));
    }

    #[test]
    fn map_mhk_is_selected_by_map_name_and_hash() {
        let grp = mini_grp();
        let def = br#"mapinfo {
            maptitle "Fixture map"
            mapfile E1L1.MAP
            mapmd4 a448017aaf21d8525fc10ae87aa6729d
            mhkfile resources/maphacks/e1l1.mhk
        }"#;
        let mhk = b"sprite 2 angoff -256\nsprite 4 pitch 2\n";
        let renderer = DukeSoftRenderer::from_grp_layers_with_mod_assets_and_mhks(
            &[&grp],
            None,
            &[("maps.def", def.as_slice())],
            &[],
            &[("mods/resources/maphacks/E1L1.MHK", mhk.as_slice())],
        )
        .unwrap();

        assert_eq!(
            renderer
                .load_map_hack_for_map("maps/e1l1.map", b"abc")
                .unwrap()
                .as_deref(),
            Some("mods/resources/maphacks/E1L1.MHK")
        );
        assert_eq!(
            renderer.map_hack_sprite_angles.borrow().get(&2),
            Some(&1792)
        );
        assert_eq!(
            renderer
                .load_map_hack_for_map("E1L1.MAP", b"different map bytes")
                .unwrap(),
            None
        );
        assert!(renderer.map_hack_sprite_angles.borrow().is_empty());
    }

    #[test]
    fn explicit_game_palette_uses_matching_lookup_table() {
        let grp = mini_grp_with_lookup(Some(240));
        let renderer = DukeSoftRenderer::from_grp(&grp).unwrap();
        let mut player = Player::default();
        player.game_palette = 240;

        let selected = renderer
            .screen_palette_for_player(&BuildWorld::new(), &player)
            .expect("lookup palette is present");
        assert_eq!(&selected[..3], &renderer.palette.rgb[255 * 3..255 * 3 + 3]);
    }

    fn mini_grp() -> Vec<u8> {
        mini_grp_with_lookup(None)
    }

    fn mini_grp_with_lookup(game_palette: Option<u8>) -> Vec<u8> {
        let mut palette = vec![0; 768];
        if game_palette.is_some() {
            palette[..3].copy_from_slice(&[1, 2, 3]);
            palette[255 * 3..255 * 3 + 3].copy_from_slice(&[8, 9, 10]);
        }
        palette.extend_from_slice(&1u16.to_le_bytes());
        palette.extend(0..=255);
        let mut art = Vec::new();
        for value in [1u32, 1, 1, 1] {
            art.extend_from_slice(&value.to_le_bytes());
        }
        art.extend_from_slice(&1i16.to_le_bytes());
        art.extend_from_slice(&1i16.to_le_bytes());
        art.extend_from_slice(&0u32.to_le_bytes());
        art.push(0);
        let mut files = vec![("PALETTE.DAT", palette), ("TILES000.ART", art)];
        if let Some(palette_id) = game_palette {
            let mut lookup = vec![1, palette_id];
            lookup.extend((0..=255_u8).rev());
            files.push(("LOOKUP.DAT", lookup));
        }
        let mut grp = b"KenSilverman".to_vec();
        grp.extend_from_slice(&(files.len() as u32).to_le_bytes());
        for (name, bytes) in &files {
            let mut entry_name = [0; 12];
            entry_name[..name.len()].copy_from_slice(name.as_bytes());
            grp.extend_from_slice(&entry_name);
            grp.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        }
        for (_, bytes) in &files {
            grp.extend_from_slice(bytes);
        }
        grp
    }

    fn bmp_2x1() -> Vec<u8> {
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&62u32.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&2i32.to_le_bytes());
        bmp.extend_from_slice(&(-1i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&8u32.to_le_bytes());
        bmp.extend_from_slice(&[0; 16]);
        bmp.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        bmp
    }
}

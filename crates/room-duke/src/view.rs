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
            brightness_table,
            brightness: Cell::new(4),
            tiles: RefCell::new(tiles),
            camera_feed: RefCell::new(None),
            geometry_interpolation: RefCell::new(None),
            weapon_tiles,
            access_hand_tile,
            laser_hand_tile,
        })
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
        render_fp_textured_at(
            render_world,
            &view,
            &mut frame,
            &self.tiles.borrow(),
            tic.wrapping_mul(4),
        );
        // Chocolate's `displayweapon` returns as soon as `animatetip` draws
        // the TIP tile, so the weapon is hidden for the complete tip pose.
        if player.tip_timer == 0 && !player.over_shoulder_on && player.remote_camera.is_none() {
            if !self.draw_access_hand(&mut frame, render_world, player) {
                self.draw_weapon(&mut frame, player);
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
        let palette_pixels = rotate_palette_frame(&frame.pixels, width, height, screen_roll);
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

    /// Draw the selected Atomic weapon from its GRP ART tiles.
    fn draw_weapon(&self, frame: &mut SoftFrame, player: &Player) {
        // `last_weapon` stays selected visually while `weapon_pos` is negative;
        // at -9 the simulation switches to the new weapon and starts its rise
        // from `weapon_pos == 10`.
        let weapon = if player.weapon_pos < 0 && (0..=11).contains(&player.last_weapon) {
            player.last_weapon
        } else {
            player.curr_weapon
        };
        let slot = weapon.clamp(0, 11) as usize;
        let Some(base) = self.weapon_tiles[slot] else {
            return;
        };
        let kick = player.kickback_pic.max(0);
        let tile = match slot {
            0 => base + if (5..=9).contains(&kick) { 1 } else { 0 },
            1 => match kick {
                1 => base + 1,
                2 => base + 2,
                5..=9 => base + 4,
                _ => base,
            },
            2 => match kick {
                1 | 2 => base + 2,
                3..=12 => base + 3,
                13..=15 | 28..=30 => base + 4,
                16..=19 | 24..=27 => base + 5,
                20..=23 => base + 6,
                _ => base,
            },
            3 => {
                if kick == 0 {
                    base + 1
                } else {
                    base + 2 + (kick / 5).min(1)
                }
            }
            4 => base + (kick / 2).clamp(0, 3),
            5 => base + if (5..=20).contains(&kick) { 1 } else { 0 },
            6 | 11 | 7 | 9 => base + if kick > 0 { 1 } else { 0 },
            8 => base + (kick / 4).clamp(0, 3),
            10 => base + if (1..=5).contains(&kick) { 1 } else { 0 },
            _ => base,
        };
        let tiles = self.tiles.borrow();
        let tile = if tiles.dimensions(tile).is_some() {
            tile
        } else {
            base
        };
        let Some((source_w, source_h)) = tiles.dimensions(tile) else {
            return;
        };
        // `DukeContent` uses these FIRSTGUN HUD centers for its desktop view.
        let (virtual_x, virtual_y) = match slot {
            0 => (160.0, 186.0),
            1 => (195.0, 164.0),
            2 => (150.0, 122.0),
            3 => (180.0, 180.0),
            4 => (176.0, 96.0),
            5 => (190.0, 164.0),
            6 | 11 => (196.0, 160.0),
            7 => (150.0, 158.0),
            8 => (140.0, 175.0),
            9 => (210.0, 165.0),
            10 => (150.0, 170.0),
            _ => (195.0, 164.0),
        };
        let (scale_x, scale_y) = (
            f64::from(frame.width) / 320.0,
            f64::from(frame.height) / 200.0,
        );
        let center_x = (virtual_x * scale_x).round() as i32;
        // Chocolate keeps the previous weapon on screen while lowering it,
        // moves the newly selected weapon up from below the viewport, then
        // returns `weapon_pos` to zero. The simulation encodes the lower and
        // raise phases as -1..=-9 and 10..=1 respectively.
        let weapon_transition = i32::from(player.weapon_pos).unsigned_abs().min(10);
        let transition_y = (weapon_transition as f64 * 10.0 * scale_y).round() as i32;
        let center_y = (virtual_y * scale_y).round() as i32 + transition_y;
        let dest_w = (f64::from(source_w) * scale_x).round().max(1.0) as i32;
        let dest_h = (f64::from(source_h) * scale_y).round().max(1.0) as i32;
        let left = center_x - dest_w / 2;
        let top = center_y - dest_h / 2;
        for y in 0..dest_h {
            let sy = (y as i64 * i64::from(source_h) / i64::from(dest_h)) as u32;
            let dy = top + y;
            if dy < 0 || dy >= frame.height as i32 {
                continue;
            }
            for x in 0..dest_w {
                let dx = left + x;
                if dx < 0 || dx >= frame.width as i32 {
                    continue;
                }
                let sx = (x as i64 * i64::from(source_w) / i64::from(dest_w)) as u32;
                let Some(pixel) = tiles.sample(tile, sx, sy) else {
                    continue;
                };
                if pixel != room_build::art::TRANSPARENT {
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

//! Browser Doom / Freedoom play session (Headless + materials).

use crate::feed::{EventFeed, Scores, kill_line, stcfn_safe};
use crate::netplay::NetPlay;
use crate::web_audio::WebAudio;
use bytes::Bytes;
use glam::Vec3;
use room_audio::ENGINE_SAMPLE_RATE;
use room_defs::{DefinitionsBuilder, GlDefs};
use room_game::{
    EngineData, GameOptions, GameState, Headless, Skill, SoundSystem,
    intermission_music_candidates, load_embedded_engine_data, mount_embedded_engine,
    play_first_music_lump, play_map_music, play_music_lump, title_music_candidates,
};
use room_input::{InputSample, Key, build_ticcmd};
use room_map::{MapGeometry, load_map};
use room_music::{GenMidiBank, MidiDevice};
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, Canvas, DynLightInstance, GpuContext, MaterialCatalog, OffscreenTarget, SpriteInstance,
    SpriteOrient, UnlitPass, build_scene, render_offscreen, sprite_lump_candidates_rot,
    sprite_rotation_digit,
};
use room_sim::{
    Buttons, FLAG_COUNTKILL, FLAG_DONTDRAW, FLAG_FLOAT, FLAG_NOGRAVITY, FLAG_SPAWNCEILING, TicCmd,
};
use room_ui::{
    CanvasSink, DoomStatusBar, DrawCmd, DukeStatusBar, HudState, MenuCommand, MenuStack,
    PatchCache, Rect, Rgba, StatusBar, UiDrawList, blit_draw_list_full, draw_stcfn_string,
    draw_title, draw_weapon, measure_stcfn_string, parse_titledef, splash_ms, title_screen_pic,
};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Running IWAD play session (sim + render map + input).
pub struct PlaySession {
    /// Mounted engine + IWAD.
    pub vfs: Vfs,
    /// Playsim.
    pub game: Headless,
    /// Geometry for the renderer (synced from the world each tic).
    pub map: MapGeometry,
    /// Baseline sidedefs / sectors (switch texture restore).
    map_base: MapGeometry,
    /// GPU materials.
    pub materials: MaterialCatalog,
    /// Soft patches for weapon / HUD overlays.
    pub patches: PatchCache,
    /// GLDEFS (brightmaps / object lights) from the mounted VFS.
    gldefs: GlDefs,
    /// ANIMDEFS texture / flat animations.
    tex_anim: room_render::TextureAnimator,
    /// ANIMDEFS `cameratexture` sizes.
    camera_tex_sizes: BTreeMap<String, (u32, u32)>,
    /// Frames since the last security-camera canvas refresh.
    camera_tex_frame: u32,
    /// Dedicated RGBA8 pass for camera canvases.
    cam_pass: Option<UnlitPass>,
    /// Held movement / action keys (`pub(crate)` for [`crate::input_hub`]).
    pub(crate) held: BTreeSet<&'static str>,
    /// One-shot weapon slot `1..=9`.
    pub(crate) weapon: u8,
    /// Queued weapon cycles (positive = next). One step per sampled tic.
    pub(crate) weapon_cycle: i32,
    /// Mouse yaw counts this tic (public for mid-tic look preview).
    pub mouse_yaw: i32,
    /// Mouse pitch counts this tic (public for mid-tic look preview).
    pub mouse_pitch: i32,
    /// Attack held (LMB / Ctrl).
    pub(crate) attack: bool,
    /// Fractional tic accumulator (seconds).
    pub tic_accum: f32,
    /// Timestamp of last frame (ms).
    pub last_ms: f64,
    /// Camera yaw (radians), locked to sim after each tic.
    pub yaw: f64,
    /// Camera pitch (radians).
    pub pitch: f64,
    /// Idle tics spent on intermission before auto-advance.
    intermission_wait: u32,
    /// Web Audio output (created on first user gesture).
    pub audio: Option<WebAudio>,
    /// Lockstep coop (WebSocket relay); `None` for single-player.
    net: Option<NetPlay>,
    /// Local player seat (camera / HUD / input).
    local_seat: u8,
    /// Per-seat eye after the previous / latest tic (render interpolation).
    prev_eyes: [Option<Vec3>; VIEW_SEATS],
    cur_eyes: [Option<Vec3>; VIEW_SEATS],
    /// Kills / deaths / frags per seat.
    scores: Scores,
    /// Bottom-left event lines.
    feed: EventFeed,
    /// One-shot "toggle spectator grid" (from [`crate::input_hub`]).
    pub(crate) view_grid_toggle: bool,
    /// Spectator: all players at once vs. following `local_seat`.
    spectator_grid: bool,
    /// Attack state last frame (spectator click = next player).
    prev_attack: bool,
    /// Spectator is fast-forwarding through the match history.
    catching_up: bool,
    /// Editor free-fly camera; while set the sim is frozen.
    pub edit_cam: Option<EditCam>,
    /// Skill the level was started with (editor restarts).
    skill: Skill,
    /// The match result went to the server already.
    match_reported: bool,
    /// MAPINFO of the mounted content (sky per map).
    mapinfo: Arc<room_defs::MapInfo>,
    /// Title / pause MENUDEF stack.
    menu: Option<MenuStack>,
    /// Frames spent on the current title (16 ms steps for ANM).
    title_frames: u64,
    title_skip_ms: u64,
    prev_title_up: bool,
    prev_title_down: bool,
    prev_title_ok: bool,
}

/// Editor fly camera (map units / radians).
#[derive(Clone, Copy, Debug)]
pub struct EditCam {
    /// Eye position.
    pub pos: Vec3,
    /// Yaw (0 = east, counter-clockwise).
    pub yaw: f64,
    /// Pitch (positive = up).
    pub pitch: f64,
}

/// Editor fly speed (map units per second; Shift = ×3).
const EDIT_FLY_SPEED: f32 = 320.0;

/// One spectator grid viewport.
pub struct SpectatorPane {
    /// Seat shown in this pane.
    pub seat: u8,
    /// Interpolated eye.
    pub eye: Vec3,
    /// Look direction.
    pub forward: Vec3,
    /// `[x, y, w, h]` in framebuffer pixels.
    pub rect: [f32; 4],
}

/// Eye jumps larger than this between tics are teleports (no interpolation).
const TELEPORT_DIST: f32 = 64.0;

/// Seats the sim supports (and the spectator grid shows).
const VIEW_SEATS: usize = 4;

/// Spectators fast-forward while this many frames are queued.
const CATCHUP_BACKLOG: u32 = 35;

/// Wall-clock budget per frame for spectator fast-forward.
const CATCHUP_BUDGET_MS: f64 = 10.0;

/// Gap between spectator panes (pixels).
const PANE_GAP: f32 = 2.0;

/// `STCFN` scale for feed / scoreboard / pane labels (virtual 200-line units).
const SMALL_TEXT: f32 = 0.6;

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// Coop / skill overrides when starting a play session.
pub struct PlayStartOpts {
    /// Map lump name (empty → E1M1/MAP01).
    pub map_name: Option<String>,
    /// Skill 1..=5.
    pub skill: u8,
    /// RNG seed.
    pub rng_seed: u32,
    /// Coop seats (1..=4).
    pub coop_players: u8,
    /// Local seat index.
    pub local_seat: u8,
    /// Active lockstep session.
    pub net: Option<NetPlay>,
    /// PWADs (name, bytes) mounted after the IWAD, in order.
    pub pwads: Vec<(String, Vec<u8>)>,
    /// Room match time limit in seconds (0 = none).
    pub time_limit_secs: u32,
    /// Stay on the title / MENUDEF until New Game (no `?map=`).
    pub stay_on_title: bool,
}

impl Default for PlayStartOpts {
    fn default() -> Self {
        Self {
            map_name: None,
            skill: 3,
            rng_seed: 1,
            coop_players: 1,
            local_seat: 0,
            net: None,
            pwads: Vec::new(),
            time_limit_secs: 0,
            stay_on_title: false,
        }
    }
}

impl PlaySession {
    /// Mount embedded `room.pk3` + IWAD bytes and start (skill, map, coop /
    /// lockstep options).
    pub fn start_with(iwad: &[u8], opts: PlayStartOpts) -> Result<Self, String> {
        let builder = mount_embedded_engine(VfsBuilder::new()).map_err(|e| e.to_string())?;
        // Duke Nukem 3D GRP → converted UDMF PWAD (room-build).
        let mut builder = if room_build::is_grp(iwad) {
            let wad = room_build::grp_to_wad(iwad).map_err(|e| format!("GRP: {e}"))?;
            builder.mount_memory("duke3d.wad", Bytes::from(wad))
        } else {
            builder.mount_memory("doom1.wad", Bytes::copy_from_slice(iwad))
        }
        .map_err(|e| e.to_string())?;
        for (name, bytes) in &opts.pwads {
            builder = builder
                .mount_memory(name, Bytes::copy_from_slice(bytes))
                .map_err(|e| format!("{name}: {e}"))?;
        }
        let vfs = builder.build().map_err(|e| e.to_string())?;

        let mut opts = opts;
        let requested = opts.map_name.as_deref().unwrap_or("").trim().to_owned();
        if requested.is_empty() && opts.net.is_none() {
            opts.stay_on_title = true;
        }
        let map_name = resolve_start_map(&vfs, &requested);
        let mut map = load_map(&vfs, &map_name).map_err(|e| e.to_string())?;
        ensure_nodes(&mut map);
        let data = room_game::load_layered_engine_data(&vfs)
            .or_else(|e| {
                web_sys::console::warn_1(&format!("room-web: PWAD DECORATE: {e}").into());
                load_embedded_engine_data()
            })
            .map_err(|e| e.to_string())?;
        Self::from_parts(vfs, map, opts, data)
    }

    /// Start `map` (nodes already built) on an already-mounted `vfs`.
    pub fn from_parts(
        vfs: Vfs,
        map: MapGeometry,
        opts: PlayStartOpts,
        data: EngineData,
    ) -> Result<Self, String> {
        let skill = Skill::new(opts.skill);
        let coop = opts.coop_players.clamp(1, 4);
        let mut game = Headless::new(GameOptions {
            skill,
            rng_seed: opts.rng_seed,
            record_demo: false,
            coop_players: coop,
            time_limit_tics: opts.time_limit_secs.saturating_mul(35),
            ..GameOptions::default()
        });

        let mut gldefs = GlDefs::default();
        let mut tex_anim = room_render::TextureAnimator::default();
        let mut camera_tex_sizes = BTreeMap::new();
        let mut mapinfo = Arc::new(room_defs::MapInfo::default());
        if let Ok(defs) = DefinitionsBuilder::new(&vfs).build() {
            game.set_complevel(defs.complevel);
            mapinfo = Arc::new(defs.mapinfo);
            game.set_mapinfo(Arc::clone(&mapinfo));
            game.set_terrain(&defs.terrain);
            game.set_sndseq(&defs.sndseq);
            game.set_lockdefs(&defs.lockdefs, Some("Doom"));
            game.set_decaldef(&defs.decaldef);
            game.set_keyconf(&defs.keyconf);
            if let Some(id) = vfs.find_any_ns("CONEVT") {
                if let Ok(bytes) = vfs.read(id) {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        game.set_conevt(text);
                    }
                }
            }
            game.set_menudef(defs.menudef.clone());
            gldefs = defs.gldefs;
            tex_anim = room_render::TextureAnimator::new(&defs.animdefs);
            for cam in &defs.animdefs.camera_textures {
                let w = cam.width.clamp(1, 256) as u32;
                let h = cam.height.clamp(1, 256) as u32;
                camera_tex_sizes.insert(cam.name.clone(), (w, h));
            }
            let mut libs = Vec::new();
            for name in &defs.loadacs.libraries {
                if let Some(id) = vfs
                    .find(name, Namespace::Acs)
                    .or_else(|| vfs.find_any_ns(name))
                {
                    if let Ok(bytes) = vfs.read(id) {
                        libs.push(bytes.to_vec());
                    }
                }
            }
            if !libs.is_empty() {
                game.set_acs_libraries(libs);
            }
        }

        game.set_defs(data.classes, data.states);
        game.drop_weapons_without_sprites(&vfs);
        let (mods, mod_errors) = room_game::discover_mods(&vfs);
        for e in mod_errors {
            web_sys::console::warn_1(&format!("room-web: mod: {e}").into());
        }
        if !mods.is_empty() {
            match game.set_script_mods(mods) {
                Ok(names) => web_sys::console::log_1(
                    &format!("room-web: WASM mods: {}", names.join(", ")).into(),
                ),
                Err(e) => web_sys::console::warn_1(&format!("room-web: {e}").into()),
            }
        }
        if let Some(db) = data.compat_db {
            game.set_compat_db(db);
        }
        if let Some(fixes) = data.level_fixes {
            game.set_level_fixes(fixes);
        }

        let mut sounds = SoundSystem::with_sample_rate(ENGINE_SAMPLE_RATE);
        match sounds.load_sndinfo_from_vfs(&vfs) {
            Ok(()) => game.set_sounds(sounds),
            Err(e) => {
                web_sys::console::warn_1(&format!("room-web: SNDINFO: {e}").into());
            }
        }

        if opts.stay_on_title {
            game.set_pending_map(map.clone());
        } else {
            game.start_custom_map(map.clone(), skill)
                .map_err(|e| e.to_string())?;
        }

        let patches = PatchCache::from_vfs(&vfs);
        let mut materials = MaterialCatalog::new();
        {
            let mut bm = std::collections::BTreeMap::new();
            for (k, v) in &gldefs.brightmaps {
                bm.insert(k.clone(), v.map.clone());
            }
            for (k, v) in &gldefs.materials {
                if let Some(map) = &v.brightmap {
                    bm.insert(k.clone(), map.clone());
                }
            }
            materials.set_brightmaps(bm);
        }
        let menu = opts.stay_on_title.then(|| {
            MenuStack::from_menudef_or_doom(game.menudef())
                .with_skills(mapinfo.skills.as_slice())
                .with_episodes(mapinfo.episodes.as_slice())
        });
        let mut session = Self {
            vfs,
            game,
            map_base: map.clone(),
            map,
            materials,
            patches,
            gldefs,
            tex_anim,
            camera_tex_sizes,
            camera_tex_frame: 0,
            cam_pass: None,
            held: BTreeSet::new(),
            weapon: 0,
            weapon_cycle: 0,
            mouse_yaw: 0,
            mouse_pitch: 0,
            attack: false,
            tic_accum: 0.0,
            last_ms: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            intermission_wait: 0,
            audio: None,
            net: opts.net,
            local_seat: opts.local_seat.min(coop.saturating_sub(1)),
            prev_eyes: [None; VIEW_SEATS],
            cur_eyes: [None; VIEW_SEATS],
            scores: Scores::default(),
            feed: EventFeed::default(),
            view_grid_toggle: false,
            spectator_grid: true,
            prev_attack: false,
            catching_up: false,
            edit_cam: None,
            skill,
            match_reported: false,
            mapinfo,
            menu,
            title_frames: 0,
            title_skip_ms: 0,
            prev_title_up: false,
            prev_title_down: false,
            prev_title_ok: false,
        };
        session.sync_camera_from_player();
        Ok(session)
    }

    /// Whether this session is lockstep coop.
    #[must_use]
    pub fn is_net(&self) -> bool {
        self.net.is_some()
    }

    /// Attach lockstep after a solo map prepare (v2 lobby).
    pub fn attach_net(&mut self, net: NetPlay) {
        self.net = Some(net);
    }

    /// Coarse map outline as flat i16 segments `[x1,y1,x2,y2,…]` for admin minimap.
    #[must_use]
    pub fn map_outline_i16(&self) -> Vec<i16> {
        // Keep well under the 4 KiB JSON control frame limit.
        const MAX_SEGS: usize = 96;
        let lines = &self.map.lines;
        let verts = &self.map.vertices;
        let mut out = Vec::with_capacity(MAX_SEGS.saturating_mul(4));
        for line in lines.iter().take(MAX_SEGS) {
            let Some(v1) = verts.get(line.v1.raw() as usize) else {
                continue;
            };
            let Some(v2) = verts.get(line.v2.raw() as usize) else {
                continue;
            };
            out.push(clamp_i16(v1.pos.x()));
            out.push(clamp_i16(v1.pos.y()));
            out.push(clamp_i16(v2.pos.x()));
            out.push(clamp_i16(v2.pos.y()));
        }
        out
    }

    /// Local player pose in map units / degrees, if spawned.
    #[must_use]
    pub fn local_pose_i16(&self) -> Option<(i32, i32, i16)> {
        let world = self.game.world()?;
        let p = world.players.get(usize::from(self.local_seat))?.as_ref()?;
        let mo = p.mo.and_then(|id| world.actors.get(id))?;
        Some((
            mo.pos.x().round() as i32,
            mo.pos.y().round() as i32,
            mo.angle.degrees().round() as i16,
        ))
    }

    /// Start Web Audio on a user gesture (autoplay policy).
    pub fn ensure_audio(&mut self) {
        if self.audio.is_some() {
            if let Some(a) = self.audio.as_ref() {
                a.resume();
            }
            return;
        }
        match WebAudio::start() {
            Ok(audio) => {
                let rate = audio.sample_rate();
                if rate != ENGINE_SAMPLE_RATE {
                    if let Some(sounds) = self.game.sounds.as_mut() {
                        *sounds = SoundSystem::with_sample_rate(rate);
                        let _ = sounds.load_sndinfo_from_vfs(&self.vfs);
                    }
                }
                // GM SoundFont when `roomSetSoundfont` delivered one, else OPL + GENMIDI.
                if let Ok(mut music) = audio.music.try_borrow_mut() {
                    music.set_sample_rate(rate);
                    let genmidi = load_genmidi(&self.vfs);
                    let has_genmidi = genmidi.is_some();
                    if let Some(bank) = genmidi {
                        music.set_genmidi(bank);
                    }
                    let device = crate::web_audio::preferred_midi_device(&music);
                    if device == MidiDevice::Opl && !has_genmidi {
                        web_sys::console::warn_1(
                            &"room-web: no SoundFont and no GENMIDI — music silent".into(),
                        );
                    }
                    music.set_mididevice(device);
                    if matches!(self.game.state, GameState::Title) {
                        match play_first_music_lump(&mut music, &self.vfs, title_music_candidates())
                        {
                            Ok(lump) => {
                                web_sys::console::log_1(
                                    &format!("room-web: title music {lump} ({device:?})").into(),
                                );
                            }
                            Err(e) => {
                                web_sys::console::warn_1(
                                    &format!("room-web: title music: {e}").into(),
                                );
                            }
                        }
                    } else {
                        let map = self.game.map_name().to_owned();
                        match play_map_music(&mut music, &self.vfs, &map, self.game.mapinfo()) {
                            Ok(lump) => {
                                web_sys::console::log_1(
                                    &format!("room-web: music {lump} ({device:?})").into(),
                                );
                            }
                            Err(e) => {
                                web_sys::console::warn_1(&format!("room-web: music: {e}").into());
                            }
                        }
                    }
                }
                self.audio = Some(audio);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("room-web: audio: {e:?}").into());
            }
        }
    }

    /// GLDEFS object-frame lights for the current tic (muzzle flashes, etc.).
    pub fn collect_dyn_lights(&self) -> Vec<DynLightInstance> {
        let mut out = Vec::new();
        let Some(world) = self.game.world() else {
            return out;
        };
        if self.gldefs.object_lights.is_empty() {
            return out;
        }
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed {
                continue;
            }
            let Some(class) = world.classes.get(a.class) else {
                continue;
            };
            let frame_ch = char::from(b'A' + a.frame.min(25));
            let frame_s = frame_ch.to_string();
            for assoc in &self.gldefs.object_lights {
                if !assoc.class.eq_ignore_ascii_case(&class.name) {
                    continue;
                }
                if !assoc.frame.is_empty() && !assoc.frame.eq_ignore_ascii_case(&frame_s) {
                    continue;
                }
                let Some(def) = self.gldefs.lights.get(&assoc.light.to_ascii_lowercase()) else {
                    continue;
                };
                out.push(DynLightInstance {
                    pos: Vec3::new(
                        a.pos.x() as f32 + def.offset[0],
                        a.pos.y() as f32 + def.offset[1],
                        a.pos.z() as f32 + def.offset[2],
                    ),
                    color: def.color,
                    radius: def.size.max(1.0),
                    subtractive: def.subtractive,
                    spot_dir: None,
                    spot_outer_cos: 1.0,
                    spot_inner_cos: 1.0,
                });
            }
        }
        out
    }

    /// Upload common wall/flat/sprite names (reduces first-frame stalls).
    pub fn preload_materials(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Without PLAYPAL, paletted lumps expand as greyscale; without TEXTUREX,
        // composed wall names miss and draw as the default white albedo.
        self.materials.load_playpal(&self.vfs);
        self.materials.load_texturex(&self.vfs);
        self.materials.ensure_sampler(device);
        let mut names = BTreeSet::new();
        for s in &self.map.sides {
            for n in [&s.middle, &s.upper, &s.lower] {
                let t = n.trim();
                if !t.is_empty() && t != "-" {
                    names.insert(t.to_ascii_uppercase());
                }
            }
        }
        for sec in &self.map.sectors {
            for n in [&sec.floor_pic, &sec.ceiling_pic] {
                let t = n.trim();
                if !t.is_empty() && t != "-" {
                    names.insert(t.to_ascii_uppercase());
                }
            }
        }
        names.insert(self.sky_texture());
        for base in ["POSS", "SPOS", "TROO", "BAL1", "PLAY", "PUFF"] {
            for frame in b'A'..=b'H' {
                let letter = frame as char;
                for rot in ['0', '1', '2', '3', '4', '5'] {
                    names.insert(format!("{base}{letter}{rot}"));
                }
            }
        }
        for name in names {
            let _ = self
                .materials
                .get_or_upload(device, queue, &self.vfs, &name);
        }
    }

    /// Render security-camera canvases into materials (throttled; GPU copy).
    pub fn refresh_camera_textures(&mut self, gpu: &GpuContext) {
        self.camera_tex_frame = self.camera_tex_frame.wrapping_add(1);
        if self.camera_tex_frame % 8 != 1 {
            return;
        }
        let binds: Vec<(String, room_sim::CameraTextureBind)> = {
            let Some(world) = self.game.world() else {
                return;
            };
            if world.camera_textures.is_empty() {
                return;
            }
            world
                .camera_textures
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        if self.cam_pass.is_none() {
            self.cam_pass = Some(UnlitPass::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb));
        }
        let Some(pass) = self.cam_pass.as_mut() else {
            return;
        };
        for (name, bind) in binds {
            let Some(actor) = self
                .game
                .world()
                .and_then(|w| w.actors.get(bind.camera).cloned())
            else {
                continue;
            };
            let (w, h) = self
                .camera_tex_sizes
                .get(&name)
                .copied()
                .unwrap_or((64, 64));
            let aspect = f64::from(w) / f64::from(h.max(1));
            let cam = Camera::from_actor_view(
                actor.pos.x(),
                actor.pos.y(),
                actor.pos.z() + 41.0,
                actor.angle.radians(),
                bind.fov,
                aspect,
            );
            let scene = build_scene(&self.map, &cam, None);
            let target = OffscreenTarget::new(gpu, w, h);
            render_offscreen(gpu, pass, &target, &cam, &scene);
            let _ = self
                .materials
                .upsert_copy(&gpu.device, &gpu.queue, &name, &target.color, w, h);
        }
    }

    /// Swap animated wall / flat materials of `scene` to the frame due now.
    pub fn animate_textures(&self, scene: &mut room_render::SceneLists) {
        let tic = self.game.world().map_or(0, |w| w.level.maptime);
        self.tex_anim.apply(scene, tic);
    }

    /// Sky texture: ACS `ChangeSky`, else MAPINFO `sky1` of this map, else `SKY1`.
    #[must_use]
    pub fn sky_texture(&self) -> String {
        if let Some(s) = self.game.world().and_then(|w| w.sky1_override.clone()) {
            return s;
        }
        self.mapinfo
            .maps
            .get(&self.map.name.to_ascii_uppercase())
            .and_then(|m| m.sky1.clone())
            .unwrap_or_else(|| "SKY1".into())
    }

    /// Local seat (debug).
    #[must_use]
    pub fn local_seat_debug(&self) -> u8 {
        self.local_seat
    }

    /// Lockstep wait info (`tic/pending` or `None` if solo).
    #[must_use]
    pub fn net_debug(&self) -> Option<String> {
        let net = self.net.as_ref()?;
        let (rx, tx) = net.data_stats();
        let gone = net.gone_peers();
        Some(format!(
            "tic={} pend={} wait={} ready={} gone={gone:?} rx={rx} tx={tx}",
            net.waiting_tic(),
            net.has_pending(),
            net.frames_waiting(),
            net.peers_ready()
        ))
    }

    /// Structured net stats for the HUD graph (`None` if solo).
    #[must_use]
    pub fn net_stats_js(&self) -> Option<js_sys::Object> {
        let net = self.net.as_ref()?;
        let (rx, tx) = net.data_stats();
        let obj = js_sys::Object::new();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&obj, &wasm_bindgen::JsValue::from_str(k), &v);
        };
        set(
            "tic",
            wasm_bindgen::JsValue::from(f64::from(net.waiting_tic())),
        );
        set(
            "pending",
            wasm_bindgen::JsValue::from_bool(net.has_pending()),
        );
        set(
            "waitFrames",
            wasm_bindgen::JsValue::from(f64::from(net.frames_waiting())),
        );
        set("ready", wasm_bindgen::JsValue::from_bool(net.peers_ready()));
        if let Some(rtt) = net.rtt_ms() {
            set("rttMs", wasm_bindgen::JsValue::from(rtt));
        }
        set(
            "backlog",
            wasm_bindgen::JsValue::from(f64::from(net.backlog())),
        );
        set("rx", wasm_bindgen::JsValue::from(f64::from(rx)));
        set("tx", wasm_bindgen::JsValue::from(f64::from(tx)));
        set(
            "peers",
            wasm_bindgen::JsValue::from(f64::from(net.peer_count)),
        );
        set("v2", wasm_bindgen::JsValue::from_bool(net.is_v2()));
        set(
            "seat",
            wasm_bindgen::JsValue::from(f64::from(self.local_seat)),
        );
        set(
            "held",
            wasm_bindgen::JsValue::from_str(&crate::input_hub::debug_held()),
        );
        set("pos", wasm_bindgen::JsValue::from_str(&self.pos_debug()));
        Some(obj)
    }

    /// Player XY for debug.
    #[must_use]
    pub fn pos_debug(&self) -> String {
        let Some(world) = self.game.world() else {
            return "?".into();
        };
        let seat = usize::from(self.local_seat);
        let Some(p) = world.players.get(seat).and_then(|p| p.as_ref()) else {
            return "?".into();
        };
        let Some(mo) = p.mo.and_then(|id| world.actors.get(id)) else {
            return "?".into();
        };
        let ammo = world.player_hud(seat).map_or(-1, |h| h.ammo);
        format!(
            "{:.1},{:.1} angle={:.1} ammo={ammo}",
            mo.pos.x(),
            mo.pos.y(),
            mo.angle.0
        )
    }

    /// Advance sim at 35 Hz; returns `true` if at least one tic ran.
    pub fn tick_play(&mut self, dt: f32) -> bool {
        const TIC: f32 = 1.0 / 35.0;
        if self.edit_cam.is_some() {
            self.fly_edit_cam(dt);
            return false;
        }
        let now = now_ms();
        self.feed.prune(now);
        if let Some(net) = self.net.as_mut() {
            for line in net.take_feed() {
                self.feed.push(line, now);
            }
        }
        let spectator = self.is_spectator();
        if spectator {
            self.spectator_input();
        }
        let backlog = self.net.as_ref().map_or(0, NetPlay::backlog);
        if spectator && backlog > CATCHUP_BACKLOG && matches!(self.game.state, GameState::Level) {
            return self.fast_forward(now);
        }
        self.catching_up = false;
        self.tic_accum = (self.tic_accum + dt).min(TIC * 3.0);
        // Drain a jitter backlog gradually instead of staying behind forever.
        if backlog > 2 {
            self.tic_accum += TIC * (backlog - 2).min(3) as f32;
        }
        let mut stepped = false;
        while self.tic_accum >= TIC {
            match self.game.state {
                GameState::Level => {
                    let cmds = if let Some(net) = self.net.as_mut() {
                        let Some(cmds) = net.try_advance(
                            &self.held,
                            self.attack,
                            &mut self.weapon,
                            &mut self.weapon_cycle,
                            &mut self.mouse_yaw,
                            &mut self.mouse_pitch,
                        ) else {
                            break;
                        };
                        cmds
                    } else {
                        let sample = self.sample_input();
                        let bits = build_ticcmd(&sample);
                        vec![TicCmd {
                            buttons: Buttons(bits.buttons),
                            pitch: bits.pitch,
                            yaw: bits.yaw,
                            roll: bits.roll,
                            forward_move: bits.forward_move,
                            side_move: bits.side_move,
                            up_move: bits.up_move,
                            weapon_select: bits.weapon_select,
                        }]
                    };
                    self.tic_accum -= TIC;
                    stepped = true;
                    self.game.tick(&cmds);
                    self.process_tic_events(false);
                    self.pump_audio();
                    if matches!(self.game.state, GameState::Intermission) {
                        self.start_intermission_music();
                    }
                    self.sync_map_from_world();
                    self.sync_camera_from_player();
                    let pose = self.local_pose_i16().filter(|_| !spectator);
                    if let (Some(net), Some((x, y, angle))) = (self.net.as_mut(), pose) {
                        net.maybe_send_pose(x, y, angle);
                    }
                }
                GameState::Intermission => {
                    self.tic_accum -= TIC;
                    stepped = true;
                    self.intermission_wait = self.intermission_wait.saturating_add(1);
                    // ~1.5s then enter next map (or stay if episode end).
                    if self.intermission_wait >= 52 {
                        self.intermission_wait = 0;
                        self.advance_intermission();
                    }
                }
                GameState::Title => {
                    self.tic_accum -= TIC;
                    self.title_frames = self.title_frames.saturating_add(1);
                    self.tick_title_menu();
                }
                _ => {
                    self.tic_accum -= TIC;
                }
            }
        }
        stepped
    }

    /// Spectator replay of buffered history: many tics per frame, no audio.
    fn fast_forward(&mut self, start: f64) -> bool {
        self.catching_up = true;
        let mut ran = 0_u32;
        while ran < 4096 && now_ms() - start < CATCHUP_BUDGET_MS {
            if !matches!(self.game.state, GameState::Level) {
                break;
            }
            let Some(net) = self.net.as_mut() else {
                break;
            };
            let Some(cmds) = net.try_advance(
                &self.held,
                false,
                &mut self.weapon,
                &mut self.weapon_cycle,
                &mut self.mouse_yaw,
                &mut self.mouse_pitch,
            ) else {
                break;
            };
            self.game.tick(&cmds);
            self.process_tic_events(true);
            ran += 1;
        }
        if ran > 0 {
            self.sync_map_from_world();
            self.sync_camera_from_player();
            self.prev_eyes = self.cur_eyes;
        }
        self.tic_accum = 0.0;
        ran > 0
    }

    /// Fold this tic's deaths into scores / feed; route sim messages.
    fn process_tic_events(&mut self, quiet: bool) {
        self.report_match();
        let Some(world) = self.game.world_mut() else {
            return;
        };
        let kills = world.drain_kill_events();
        let msgs = world.drain_hud_messages();
        for ev in &kills {
            self.scores.apply(ev);
        }
        if quiet {
            return;
        }
        let now = now_ms();
        for ev in &kills {
            if let Some(line) = kill_line(ev, |s| self.seat_name(s)) {
                self.feed.push(line, now);
            }
        }
        // Pickup / ACS messages are not per-player in the sim; only solo shows them.
        let solo = self.net.is_none();
        for m in msgs {
            let mine = match m.to {
                room_sim::MessageTo::Unscoped => solo,
                room_sim::MessageTo::All => true,
                room_sim::MessageTo::Seat(s) => solo || s == self.local_seat,
            };
            if mine {
                self.feed.push(m.text, now);
            }
        }
    }

    /// Send the final standings to the server once the sim decides the match.
    fn report_match(&mut self) {
        if self.match_reported {
            return;
        }
        let Some(world) = self.game.world() else {
            return;
        };
        let Some(over) = world.match_over else {
            return;
        };
        self.match_reported = true;
        let standings = world
            .standings()
            .iter()
            .map(|s| room_proto::ResultRow {
                seat: s.slot,
                place: s.place,
                frags: s.frags,
                deaths: s.deaths,
            })
            .collect();
        let msg = room_proto::ControlMsg::Result {
            winner: over.winner,
            time_up: over.time_up,
            standings,
        };
        if let Some(net) = self.net.as_mut() {
            net.send_result(&msg);
        }
    }

    /// Countdown + final screen state for the page overlay (JSON): time left,
    /// the result once the match is over, and why the server closed the room.
    #[must_use]
    pub fn match_state_json(&self) -> String {
        let Some(world) = self.game.world() else {
            return "null".into();
        };
        let closed = self.net.as_ref().and_then(NetPlay::closed_reason);
        let rows: Vec<serde_json::Value> = if world.match_over.is_some() || closed.is_some() {
            world
                .standings()
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "seat": s.slot,
                        "name": self.seat_name(s.slot),
                        "place": s.place,
                        "frags": s.frags,
                        "deaths": s.deaths,
                        "alive": s.alive,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let over = world.match_over.map(|m| {
            serde_json::json!({
                "winner": m.winner,
                "winner_name": m.winner.map(|w| self.seat_name(w)),
                "time_up": m.time_up,
            })
        });
        serde_json::json!({
            "time_left": (world.match_tics_left > 0).then(|| world.match_tics_left.div_ceil(35)),
            "over": over,
            "closed": closed,
            "rows": rows,
            "local_seat": self.local_seat,
            "spectator": self.is_spectator(),
            "solo": self.net.is_none(),
        })
        .to_string()
    }

    /// Spectator keys: 1-4 follow a player, G/0 toggles the grid, click = next.
    fn spectator_input(&mut self) {
        self.mouse_yaw = 0;
        self.mouse_pitch = 0;
        let followed = self.local_seat;
        self.pick_spectated_seat();
        if self.local_seat != followed {
            self.sync_listener();
        }
    }

    fn pick_spectated_seat(&mut self) {
        let seats = self.seats_in_game();
        if !seats.is_empty() && !seats.contains(&self.local_seat) {
            self.local_seat = seats[0];
        }
        if (1..=VIEW_SEATS as u8).contains(&self.weapon) {
            let seat = self.weapon - 1;
            if seats.contains(&seat) {
                self.local_seat = seat;
                self.spectator_grid = false;
            }
        }
        self.weapon = 0;
        if std::mem::take(&mut self.view_grid_toggle) {
            self.spectator_grid = !self.spectator_grid;
        }
        let click = self.attack && !self.prev_attack;
        self.prev_attack = self.attack;
        if click && !seats.is_empty() {
            if self.spectator_grid {
                self.spectator_grid = false;
            } else {
                let next = seats
                    .iter()
                    .copied()
                    .find(|&s| s > self.local_seat)
                    .unwrap_or(seats[0]);
                self.local_seat = next;
            }
        }
    }

    /// Seats with a spawned player body.
    fn seats_in_game(&self) -> Vec<u8> {
        let Some(world) = self.game.world() else {
            return Vec::new();
        };
        world
            .players
            .iter()
            .take(VIEW_SEATS)
            .enumerate()
            .filter(|(_, p)| p.as_ref().is_some_and(|p| p.mo.is_some()))
            .map(|(i, _)| i as u8)
            .collect()
    }

    /// Roster name for `seat` (falls back to "player N").
    fn seat_name(&self, seat: u8) -> String {
        match self.net.as_ref() {
            Some(net) => net
                .peer_name(seat)
                .map_or_else(|| format!("player {}", seat + 1), str::to_owned),
            None => "you".into(),
        }
    }

    /// Spectating a networked match (no input is sent).
    #[must_use]
    pub fn is_spectator(&self) -> bool {
        self.net.as_ref().is_some_and(NetPlay::is_spectator)
    }

    /// Spectator grid panes as `(seat, eye, forward, [x, y, w, h])` in framebuffer
    /// pixels, or `None` when a single full-frame view should be drawn.
    #[must_use]
    pub fn spectator_views(&self, fb_w: f32, fb_h: f32) -> Option<Vec<SpectatorPane>> {
        if !self.is_spectator() || !self.spectator_grid || self.catching_up {
            return None;
        }
        let seats = self.seats_in_game();
        if seats.len() < 2 {
            return None;
        }
        let rects = pane_rects(seats.len(), fb_w, fb_h);
        let views = seats
            .iter()
            .zip(rects)
            .filter_map(|(&seat, rect)| {
                let (eye, forward) = self.view_pose_for(seat)?;
                Some(SpectatorPane {
                    seat,
                    eye,
                    forward,
                    rect,
                })
            })
            .collect::<Vec<_>>();
        (!views.is_empty()).then_some(views)
    }

    fn sample_input(&mut self) -> InputSample {
        let mut keys = Vec::new();
        if self.held.contains("w") {
            keys.push(Key::Forward);
        }
        if self.held.contains("s") {
            keys.push(Key::Back);
        }
        if self.held.contains("a") {
            keys.push(Key::MoveLeft);
        }
        if self.held.contains("d") {
            keys.push(Key::MoveRight);
        }
        if self.held.contains("left") {
            keys.push(Key::Left);
        }
        if self.held.contains("right") {
            keys.push(Key::Right);
        }
        if self.held.contains("shift") {
            keys.push(Key::Speed);
        }
        if self.attack {
            keys.push(Key::Attack);
        }
        if self.held.contains("use") {
            keys.push(Key::Use);
        }
        if self.held.contains("up") {
            keys.push(Key::MoveUp);
        }
        if self.held.contains("down") {
            keys.push(Key::MoveDown);
        }
        if self.held.contains("kick") {
            keys.push(Key::QuickKick);
        }
        if self.held.contains("invuse") {
            keys.push(Key::InvUse);
        }
        match self.weapon {
            1 => keys.push(Key::Weapon1),
            2 => keys.push(Key::Weapon2),
            3 => keys.push(Key::Weapon3),
            4 => keys.push(Key::Weapon4),
            5 => keys.push(Key::Weapon5),
            6 => keys.push(Key::Weapon6),
            7 => keys.push(Key::Weapon7),
            8 => keys.push(Key::Weapon8),
            9 => keys.push(Key::Weapon9),
            _ => {}
        }
        self.weapon = 0;
        if self.weapon_cycle > 0 {
            keys.push(Key::NextWeapon);
            self.weapon_cycle -= 1;
        } else if self.weapon_cycle < 0 {
            keys.push(Key::PrevWeapon);
            self.weapon_cycle += 1;
        }
        let mouse_yaw = self.mouse_yaw;
        let mouse_pitch = self.mouse_pitch;
        self.mouse_yaw = 0;
        self.mouse_pitch = 0;
        InputSample {
            keys,
            mouse_yaw,
            mouse_pitch,
        }
    }

    fn pump_audio(&mut self) {
        // Always drain: `World::tick` clears the queue next tic, and translate
        // also loads DMX lumps into `SoundSystem.bank` for later merge.
        let events = self.game.drain_audio(&self.vfs);
        let music_req = self.game.take_music_request();
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        if audio.needs_resume() {
            audio.resume();
        }
        if let Some(song) = music_req {
            if let Ok(mut music) = audio.music.try_borrow_mut() {
                match play_music_lump(&mut music, &self.vfs, &song) {
                    Ok(lump) => {
                        web_sys::console::log_1(
                            &format!("room-web: ACS music {song} → {lump}").into(),
                        );
                    }
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("room-web: ACS music {song}: {e}").into(),
                        );
                    }
                }
            }
        }
        if let Ok(mut mix) = audio.mixer.try_borrow_mut() {
            if let Some(sounds) = self.game.sounds.as_ref() {
                mix.bank.merge_missing(&sounds.bank);
            }
            if let Some(ears) = self.listener_actor() {
                mix.set_listener(ears.pos, ears.angle.radians());
            }
            mix.apply(&events);
        }
        // Top up the AudioWorklet ring (ScriptProcessor renders in its callback).
        audio.feed_output();
    }

    /// Ears of the seat on screen: the player you control, or the one a
    /// spectator follows. Uses the view actor so ACS / chase cameras match.
    fn listener_actor(&self) -> Option<&room_sim::Actor> {
        let world = self.game.world()?;
        let p = world.players.get(usize::from(self.local_seat))?.as_ref()?;
        world.actors.get(p.camera.or(p.mo)?)
    }

    /// Move the mixer listener now (spectator switched player between tics).
    fn sync_listener(&self) {
        let (Some(audio), Some(ears)) = (self.audio.as_ref(), self.listener_actor()) else {
            return;
        };
        if let Ok(mut mix) = audio.mixer.try_borrow_mut() {
            mix.set_listener(ears.pos, ears.angle.radians());
        }
    }

    /// Rasterize weapon + classic `STBAR` HUD into a CPU canvas for `CanvasGpu`.
    pub fn build_hud_canvas(&mut self, fb_w: f32, fb_h: f32) -> Canvas {
        if self.edit_cam.is_some() {
            let mut canvas = Canvas::new();
            canvas.clear([0.0, 0.0, 0.0, 0.0]);
            return canvas;
        }
        if matches!(self.game.state, GameState::Title) {
            return self.build_title_canvas(fb_w, fb_h);
        }
        let panes = self.spectator_views(fb_w, fb_h);
        let show_bar = panes.is_none() && !self.catching_up;
        let mut ui = UiDrawList::new();
        if show_bar {
            self.draw_player_hud(&mut ui);
        }
        let overlay = self.build_overlay(fb_w, fb_h, panes.as_deref(), show_bar);

        // Prefetch every patch the draw lists need (STBAR, STTNUM*, mugshot, keys, STCFN).
        for cmd in ui.cmds.iter().chain(&overlay.cmds) {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = self.patches.ensure(&self.vfs, name);
            }
        }

        let mut canvas = Canvas::new();
        canvas.clear([0.0, 0.0, 0.0, 0.0]);
        // Preserve 320×200 aspect; bottom-align so STBAR sits on the viewport edge.
        let scale = (fb_w / 320.0).min(fb_h / 200.0);
        let ox = (fb_w - 320.0 * scale) * 0.5;
        let oy = fb_h - 200.0 * scale;
        struct Sink<'a>(&'a mut Canvas);
        impl CanvasSink for Sink<'_> {
            fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                self.0.fill_rect(x, y, w, h, color);
            }
            fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                self.0.text(x, y, text, color);
            }
        }
        struct OffsetSink<'a> {
            inner: Sink<'a>,
            ox: f32,
            oy: f32,
        }
        impl CanvasSink for OffsetSink<'_> {
            fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                self.inner.fill_rect(x + self.ox, y + self.oy, w, h, color);
            }
            fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                self.inner.text(x + self.ox, y + self.oy, text, color);
            }
        }
        blit_draw_list_full(
            &ui,
            320.0 * scale,
            200.0 * scale,
            None,
            Some(&self.patches),
            &mut OffsetSink {
                inner: Sink(&mut canvas),
                ox,
                oy,
            },
        );
        if panes.is_none() {
            for [r, g, b, a] in self.screen_tints() {
                canvas.fill_rect(0.0, 0.0, fb_w, fb_h, [r, g, b, a]);
            }
        }
        blit_draw_list_full(
            &overlay,
            fb_w,
            fb_h,
            None,
            Some(&self.patches),
            &mut Sink(&mut canvas),
        );
        canvas
    }

    /// Pain / pickup / radsuit palette flash for the seat on screen, then the
    /// ACS `FadeTo` / `A_SetBlend` screen fade.
    fn screen_tints(&self) -> Vec<[f32; 4]> {
        let Some(world) = self.game.world() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(c) = world
            .player_hud(usize::from(self.local_seat))
            .and_then(|h| room_ui::screen_blend(h.palette))
        {
            out.push([c.r, c.g, c.b, c.a]);
        }
        let fade = world.screen_fade;
        if fade.a > 0.001 {
            out.push([fade.r, fade.g, fade.b, fade.a.min(1.0)]);
        }
        out
    }

    /// Full-width overlay (virtual height 200): event feed, spectator pane
    /// labels / banner, and the Tab scoreboard.
    fn build_overlay(
        &self,
        fb_w: f32,
        fb_h: f32,
        panes: Option<&[SpectatorPane]>,
        status_bar: bool,
    ) -> UiDrawList {
        let mut ov = UiDrawList::new();
        let px = 200.0 / fb_h.max(1.0);
        ov.virtual_h = 200.0;
        ov.virtual_w = (fb_w * px).max(1.0);
        let line_h = 9.0 * SMALL_TEXT + 1.5;
        let shade = Rgba::new(0.0, 0.0, 0.0, 0.55);

        if let Some(panes) = panes {
            for pane in panes {
                let [x, y, w, _] = pane.rect;
                let label = self.seat_label(pane.seat);
                let lw = measure_stcfn_string(&label, SMALL_TEXT) + 4.0;
                let (lx, ly) = (x * px + 2.0, y * px + 2.0);
                ov.fill(Rect::new(lx, ly, lw.min(w * px - 4.0), line_h + 1.0), shade);
                draw_stcfn_string(&mut ov, lx + 2.0, ly + 1.0, &label, SMALL_TEXT);
            }
        }

        if self.is_spectator() {
            let banner = if self.catching_up {
                "CATCHING UP...".to_owned()
            } else if panes.is_some() {
                "SPECTATING  1-4 FOLLOW  G FOLLOW  TAB SCORES".to_owned()
            } else {
                format!(
                    "FOLLOWING {}  CLICK NEXT  G GRID  TAB SCORES",
                    stcfn_safe(&self.seat_name(self.local_seat))
                )
            };
            let bw = measure_stcfn_string(&banner, SMALL_TEXT);
            let bx = (ov.virtual_w - bw) * 0.5;
            let by = if panes.is_some() {
                100.0 - line_h * 0.5
            } else {
                3.0
            };
            ov.fill(Rect::new(bx - 3.0, by - 1.0, bw + 6.0, line_h + 1.0), shade);
            draw_stcfn_string(&mut ov, bx, by, &banner, SMALL_TEXT);
        }

        // Bottom-left event feed, newest at the bottom.
        let now = now_ms();
        let lines: Vec<(String, f64)> = self
            .feed
            .iter(now)
            .map(|(t, left)| (stcfn_safe(t), left))
            .collect();
        let bottom = if status_bar {
            200.0 - 32.0 - 3.0
        } else {
            197.0
        };
        let mut y = bottom - line_h * lines.len() as f32;
        for (text, left) in &lines {
            // Patch draws can't fade, so the backdrop fades instead.
            let a = (left / 1000.0).clamp(0.0, 1.0) as f32 * 0.5;
            let w = measure_stcfn_string(text, SMALL_TEXT);
            ov.fill(
                Rect::new(2.0, y - 0.5, w + 4.0, line_h),
                Rgba::new(0.0, 0.0, 0.0, a),
            );
            draw_stcfn_string(&mut ov, 4.0, y + 0.5, text, SMALL_TEXT);
            y += line_h;
        }

        if self.held.contains("scores") {
            self.draw_scoreboard(&mut ov, line_h);
        }
        ov
    }

    /// `NAME  HP 100  AR 50` (or `DEAD`) for a spectator pane.
    fn seat_label(&self, seat: u8) -> String {
        let name = stcfn_safe(&self.seat_name(seat));
        let hud = self
            .game
            .world()
            .and_then(|w| w.player_hud(usize::from(seat)));
        match hud {
            Some(h) if h.health > 0 => format!("{name}  HP {}  AR {}", h.health, h.armor),
            Some(_) => format!("{name}  DEAD"),
            None => name,
        }
    }

    fn draw_scoreboard(&self, ov: &mut UiDrawList, line_h: f32) {
        const COLS: [(&str, f32); 7] = [
            ("PLAYER", 0.0),
            ("HEALTH", 92.0),
            ("ARMOR", 128.0),
            ("KILLS", 160.0),
            ("DEATHS", 190.0),
            ("FRAGS", 226.0),
            ("STATUS", 256.0),
        ];
        let width = 300.0;
        let seats: Vec<u8> = match self.game.world() {
            Some(w) => w
                .players
                .iter()
                .take(VIEW_SEATS)
                .enumerate()
                .filter(|(_, p)| p.is_some())
                .map(|(i, _)| i as u8)
                .collect(),
            None => Vec::new(),
        };
        let rows = seats.len() as f32 + 3.0;
        let height = rows * (line_h + 2.0) + 6.0;
        let x0 = ((ov.virtual_w - width) * 0.5).max(0.0);
        let y0 = ((200.0 - height) * 0.5 - 16.0).max(2.0);
        ov.fill(
            Rect::new(x0, y0, width, height),
            Rgba::new(0.0, 0.0, 0.0, 0.72),
        );
        let row_h = line_h + 2.0;
        let mut y = y0 + 4.0;

        let title = match self.game.world() {
            Some(w) => {
                let secs = w.level.time / 35;
                format!(
                    "{}  KILLS {}/{}  TIME {}:{:02}",
                    stcfn_safe(self.game.map_name()),
                    w.level.found_kills,
                    w.level.total_kills,
                    secs / 60,
                    secs % 60
                )
            }
            None => stcfn_safe(self.game.map_name()),
        };
        let tw = measure_stcfn_string(&title, SMALL_TEXT);
        draw_stcfn_string(ov, x0 + (width - tw) * 0.5, y, &title, SMALL_TEXT);
        y += row_h * 1.5;
        for (label, cx) in COLS {
            draw_stcfn_string(ov, x0 + 6.0 + cx, y, label, SMALL_TEXT);
        }
        y += row_h;
        ov.fill(
            Rect::new(x0 + 4.0, y - 2.0, width - 8.0, 0.6),
            Rgba::new(0.75, 0.1, 0.1, 0.9),
        );

        for seat in seats {
            if self.is_seat_followed(seat) {
                ov.fill(
                    Rect::new(x0 + 2.0, y - 1.0, width - 4.0, row_h),
                    Rgba::new(0.6, 0.1, 0.1, 0.35),
                );
            }
            let hud = self
                .game
                .world()
                .and_then(|w| w.player_hud(usize::from(seat)));
            let (hp, ar) = hud.as_ref().map_or((0, 0), |h| (h.health, h.armor));
            let gone = self.net.as_ref().is_some_and(|n| n.is_gone(seat));
            let status = if gone {
                "LEFT"
            } else if hp <= 0 {
                "DEAD"
            } else {
                "ALIVE"
            };
            let sc = self.scores.seats[usize::from(seat).min(self.scores.seats.len() - 1)];
            let mut name = stcfn_safe(&self.seat_name(seat));
            name.truncate(15);
            let cells = [
                name,
                hp.max(0).to_string(),
                ar.max(0).to_string(),
                sc.kills.to_string(),
                sc.deaths.to_string(),
                sc.frags.to_string(),
                status.to_owned(),
            ];
            for ((_, cx), text) in COLS.iter().zip(cells.iter()) {
                draw_stcfn_string(ov, x0 + 6.0 + cx, y, text, SMALL_TEXT);
            }
            y += row_h;
        }
    }

    /// Row highlighted on the scoreboard (you / followed player).
    fn is_seat_followed(&self, seat: u8) -> bool {
        seat == self.local_seat && !(self.is_spectator() && self.spectator_grid)
    }

    /// Weapon sprite + classic `STBAR` for `local_seat` into a 320×200 list.
    fn draw_player_hud(&mut self, ui: &mut UiDrawList) {
        let mut hud = HudState::doom_default();
        if let Some(h) = self
            .game
            .world()
            .and_then(|w| w.player_hud(usize::from(self.local_seat)))
        {
            hud.health = h.health;
            hud.armor = h.armor;
            hud.ammo = h.ammo;
            hud.ammo_max = h.ammo_max;
            hud.keys = h.keys;
            hud.face = h.face;
            hud.weapon_patch = h.weapon_patch;
            hud.flash_patch = h.flash_patch;
            hud.bob_x = h.bob_x;
            hud.bob_y = h.bob_y;
            hud.arms = h.arms;
            hud.ammo_table = h.ammo_table;
            hud.palette = h.palette;
            hud = hud
                .with_colormap(h.colormap)
                .with_netgame(h.deathmatch, h.frags, h.face_back);
        }
        let weapon_cmds = ui.cmds.len();
        // Classic STBAR strip (raises the weapon by `STATUS_BAR_HEIGHT`).
        hud.fullscreen = false;

        if !hud.weapon_patch.is_empty() {
            let _ = self.patches.ensure(&self.vfs, &hud.weapon_patch);
        }
        if !hud.flash_patch.is_empty() {
            let _ = self.patches.ensure(&self.vfs, &hud.flash_patch);
        }

        let soft = self.patches.get(&hud.weapon_patch);
        draw_weapon(
            ui,
            &hud.weapon_patch,
            hud.fullscreen,
            soft,
            hud.bob_x,
            hud.bob_y,
        );
        // A flash frame without a lump draws nothing (GZDoom).
        if let Some(soft_flash) = self.patches.get(&hud.flash_patch) {
            draw_weapon(
                ui,
                &hud.flash_patch,
                hud.fullscreen,
                Some(soft_flash),
                hud.bob_x,
                hud.bob_y,
            );
        }
        if hud.inverse_colormap() {
            ui.invert_since(weapon_cmds);
        }
        if self.vfs.find_any_ns(DukeStatusBar::MARKER).is_some() {
            DukeStatusBar.draw(&hud, ui);
        } else {
            DoomStatusBar.draw(&hud, ui);
        }
    }

    /// Doom `fixedcolormap` of `seat`'s player (32 = invulnerability inverse).
    #[must_use]
    pub fn fixed_colormap(&self, seat: u8) -> u8 {
        self.game
            .world()
            .and_then(|w| w.players.get(usize::from(seat))?.as_ref())
            .map_or(0, room_sim::Player::fixed_colormap)
    }

    fn sync_map_from_world(&mut self) {
        let Some(world) = self.game.world() else {
            return;
        };
        let (extralight, colormap) = world
            .players
            .get(usize::from(self.local_seat))
            .and_then(|p| p.as_ref())
            .map(|p| (p.extralight, p.fixed_colormap()))
            .unwrap_or((0, 0));
        let light_boost = (extralight.clamp(0, 2) * 16 + i32::from(colormap == 1) * 64) as i16;
        // Restore switchable textures from base, then apply live overrides.
        for (i, side) in self.map_base.sides.iter().enumerate() {
            if let Some(dst) = self.map.sides.get_mut(i) {
                dst.middle = side.middle.clone();
                dst.lower = side.lower.clone();
                dst.upper = side.upper.clone();
            }
        }
        for (i, sec) in world.sectors.iter().enumerate() {
            if let Some(dst) = self.map.sectors.get_mut(i) {
                dst.floor_height = sec.floor_z;
                dst.ceiling_height = sec.ceiling_z;
                dst.light = sec.light.saturating_add(light_boost).min(255);
            }
        }
        for (&i, name) in &world.switch_mids {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.middle = name.clone();
            }
        }
        for (&i, name) in &world.switch_lowers {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.lower = name.clone();
            }
        }
        for (&i, name) in &world.switch_uppers {
            if let Some(side) = self.map.sides.get_mut(i as usize) {
                side.upper = name.clone();
            }
        }
        for (&i, name) in &world.sector_floor_pics {
            if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                sec.floor_pic = name.clone();
            }
        }
        for (&i, name) in &world.sector_ceiling_pics {
            if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                sec.ceiling_pic = name.clone();
            }
        }
        for (&i, &(floor_a, ceil_a)) in &world.plane_angles {
            if let Some(sec) = self.map.sectors.get_mut(i as usize) {
                sec.floor_angle = floor_a;
                sec.ceiling_angle = ceil_a;
            }
        }
    }

    fn sync_camera_from_player(&mut self) {
        for seat in 0..VIEW_SEATS {
            let eye = self.sim_eye(seat as u8);
            self.prev_eyes[seat] = match (self.cur_eyes[seat], eye) {
                (Some(prev), Some(cur)) if prev.distance(cur) <= TELEPORT_DIST => Some(prev),
                _ => eye,
            };
            self.cur_eyes[seat] = eye;
        }
        if let Some((yaw, pitch)) = self.sim_look(self.local_seat) {
            self.yaw = yaw;
            self.pitch = pitch;
        }
    }

    /// Yaw / pitch (radians) of `seat`'s view actor as of the last tic.
    fn sim_look(&self, seat: u8) -> Option<(f64, f64)> {
        let world = self.game.world()?;
        let p = world.players.get(usize::from(seat))?.as_ref()?;
        let a = world.actors.get(p.camera.or(p.mo)?)?;
        Some((a.angle.radians(), a.pitch.radians()))
    }

    /// Eye position of `seat` as of the last simulated tic.
    fn sim_eye(&self, seat: u8) -> Option<Vec3> {
        let world = self.game.world()?;
        let p = world
            .players
            .get(usize::from(seat))
            .and_then(|p| p.as_ref())?;
        let view_id = p.camera.or(p.mo)?;
        let a = world.actors.get(view_id)?;
        let z = if p.camera.is_some() {
            let eye = if a.camera_height > 0.0 {
                a.camera_height
            } else if a.view_height > 0.0 {
                a.view_height
            } else {
                41.0
            };
            a.pos.z() + eye
        } else {
            p.view_z
        };
        Some(Vec3::new(a.pos.x() as f32, a.pos.y() as f32, z as f32))
    }

    /// Eye + forward for this frame: eye interpolated between the last two
    /// tics, look includes mouse input not yet applied by the sim.
    #[must_use]
    pub fn view_pose(&self) -> Option<(Vec3, Vec3)> {
        if let Some(cam) = self.edit_cam {
            return Some((cam.pos, look_dir(cam.yaw, cam.pitch)));
        }
        if self.is_spectator() {
            return self.view_pose_for(self.local_seat);
        }
        let scale = std::f64::consts::TAU / 65536.0;
        let (sent_yaw, sent_pitch) = self.net.as_ref().map_or((0, 0), NetPlay::unapplied_look);
        let mouse = f64::from(room_input::MOUSE_YAW_SCALE);
        let yaw = self.yaw + (f64::from(self.mouse_yaw) * mouse + f64::from(sent_yaw)) * scale;
        let pitch = (self.pitch
            + (f64::from(self.mouse_pitch) * mouse + f64::from(sent_pitch)) * scale)
            .clamp(-1.4, 1.4);
        let eye = self.interp_eye(self.local_seat)?;
        Some((eye, look_dir(yaw, pitch)))
    }

    /// Interpolated eye + sim look for any seat (no local mouse preview).
    #[must_use]
    pub fn view_pose_for(&self, seat: u8) -> Option<(Vec3, Vec3)> {
        let (yaw, pitch) = self.sim_look(seat)?;
        Some((self.interp_eye(seat)?, look_dir(yaw, pitch)))
    }

    fn interp_eye(&self, seat: u8) -> Option<Vec3> {
        let i = usize::from(seat);
        match (
            self.prev_eyes.get(i).copied().flatten(),
            self.cur_eyes.get(i).copied().flatten(),
        ) {
            (Some(prev), Some(cur)) => {
                let alpha = (self.tic_accum * 35.0).clamp(0.0, 1.0);
                Some(prev.lerp(cur, alpha))
            }
            _ => self.sim_eye(seat),
        }
    }

    /// Apply eye + forward to `camera` (call every frame).
    pub fn apply_camera(&self, camera: &mut Camera) {
        if let Some((eye, forward)) = self.view_pose() {
            camera.eye = eye;
            camera.forward = forward;
        }
    }

    /// Live actor billboards for `build_scene` (SoftPatch size + topoffset, same as desktop).
    pub fn collect_live_sprites(&mut self, camera: &Camera) -> Vec<SpriteInstance> {
        self.collect_live_sprites_for(camera, self.local_seat)
    }

    /// Like [`Self::collect_live_sprites`], hiding `seat`'s own body.
    pub fn collect_live_sprites_for(&mut self, camera: &Camera, seat: u8) -> Vec<SpriteInstance> {
        let Some(world) = self.game.world() else {
            return Vec::new();
        };
        let seat = usize::from(seat);
        let local_mo = world
            .players
            .get(seat)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let view_x = f64::from(camera.eye.x);
        let view_y = f64::from(camera.eye.y);
        let actors: Vec<_> = world
            .thinkers
            .default
            .iter()
            .filter_map(|&id| {
                let a = world.actors.get(id)?;
                let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                let flags = a.flags | class_flags;
                if a.destroyed || Some(id) == local_mo || (flags & FLAG_DONTDRAW) != 0 {
                    return None;
                }
                let is_monster = (flags & FLAG_COUNTKILL) != 0;
                let is_missile = a.sprite.starts_with("BAL")
                    || a.sprite.starts_with("PLAS")
                    || a.sprite.starts_with("MISL");
                let mut light = 0.7_f32;
                if is_monster || is_missile {
                    light = 1.0;
                }
                Some((
                    a.pos.x(),
                    a.pos.y(),
                    a.pos.z(),
                    a.angle.degrees(),
                    a.radius,
                    a.height,
                    a.scale_x,
                    a.scale_y,
                    light,
                    a.alpha.clamp(0.0, 1.0) as f32,
                    a.sprite.clone(),
                    a.frame,
                    flags,
                ))
            })
            .collect();
        let mut out = Vec::with_capacity(actors.len());
        for (
            x,
            y,
            z,
            angle_deg,
            radius,
            height,
            scale_x,
            scale_y,
            light,
            alpha,
            sprite,
            frame,
            flags,
        ) in actors
        {
            let mut w = (radius * 2.0 * scale_x.abs()).clamp(0.25, 4096.0);
            let mut h = (height * scale_y.abs()).clamp(0.25, 4096.0);
            let mut texture = String::new();
            let mut flip_u = false;
            let mut anchor_u = 0.5_f32;
            let mut z_bottom = z;
            let grounded = (flags & (FLAG_NOGRAVITY | FLAG_SPAWNCEILING | FLAG_FLOAT)) == 0;
            if !sprite.is_empty() {
                let rot = sprite_rotation_digit(x, y, angle_deg, view_x, view_y);
                let candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
                for (name, flip) in &candidates {
                    if let Some(patch) = self.patches.ensure(&self.vfs, name) {
                        let pw = (f64::from(patch.width) * scale_x.abs()).clamp(0.25, 4096.0);
                        let ph = (f64::from(patch.height) * scale_y.abs()).clamp(0.25, 4096.0);
                        let top = f64::from(patch.top_offset) * scale_y.abs();
                        let left = f64::from(patch.left_offset) * scale_x.abs();
                        w = pw;
                        h = ph;
                        const FLOOR_LIFT: f64 = 1.0;
                        let raw_bottom = z + top - ph;
                        z_bottom = if grounded {
                            raw_bottom.max(z + FLOOR_LIFT)
                        } else {
                            raw_bottom + 0.05
                        };
                        anchor_u = if pw > 1e-6 { (left / pw) as f32 } else { 0.5 };
                        texture = name.clone();
                        flip_u = *flip;
                        break;
                    }
                }
            }
            // `TNT1` and frames without a lump draw nothing (GZDoom).
            if texture.is_empty() {
                continue;
            }
            let yaw = angle_deg.to_radians() as f32;
            let orient = if flags & room_sim::FLAG_FLATSPRITE != 0 {
                z_bottom = z;
                SpriteOrient::Flat { yaw }
            } else if flags & room_sim::FLAG_WALLSPRITE != 0 {
                SpriteOrient::Wall { yaw }
            } else {
                SpriteOrient::Billboard
            };
            out.push(SpriteInstance {
                pos: Vec3::new(x as f32, y as f32, z_bottom as f32),
                width: w as f32,
                height: h as f32,
                light,
                alpha,
                fuzz: false,
                texture,
                flip_u,
                anchor_u,
                orient,
            });
        }
        out
    }

    fn advance_intermission(&mut self) {
        let next = self
            .game
            .intermission
            .as_ref()
            .and_then(|t| t.next_map.clone())
            .or_else(|| Headless::default_next_map(self.game.map_name(), false));
        let Some(name) = next else {
            self.game.enter_finale();
            return;
        };
        let mut next_map = match load_map(&self.vfs, &name) {
            Ok(m) => m,
            Err(_) => {
                self.game.enter_finale();
                return;
            }
        };
        ensure_nodes(&mut next_map);
        self.map_base = next_map.clone();
        self.map = next_map.clone();
        let _ = self.game.continue_to_map(next_map);
        self.start_level_music();
    }

    /// `D_DM2INT` / `D_INTER` on the tally screen (desktop parity).
    fn title_elapsed_ms(&self) -> u64 {
        self.title_frames
            .saturating_mul(16)
            .saturating_add(self.title_skip_ms)
    }

    fn title_anim_defs(&self) -> (Option<(u32, u16)>, Option<(u32, u16)>) {
        let read_def = |name: &str| {
            self.vfs
                .find_any_ns(name)
                .and_then(|id| self.vfs.read(id).ok())
                .and_then(|b| parse_titledef(&String::from_utf8_lossy(&b)))
        };
        (read_def("RADLDEF"), read_def("TITLEDEF"))
    }

    fn title_in_splash(&self) -> bool {
        let (splash, logo) = self.title_anim_defs();
        title_screen_pic(splash, logo, self.title_elapsed_ms()).1
    }

    fn skip_title_splash(&mut self) {
        let dur = splash_ms(self.title_anim_defs().0);
        if dur > 0 {
            self.title_skip_ms = dur;
        }
    }

    fn tick_title_menu(&mut self) {
        let up = self.held.contains("w");
        let down = self.held.contains("s");
        let ok = self.attack || self.held.contains("enter") || self.held.contains("use");
        let any = up || down || ok;
        if self.title_in_splash() {
            if any && !self.prev_title_ok {
                self.skip_title_splash();
            }
            self.prev_title_up = up;
            self.prev_title_down = down;
            self.prev_title_ok = ok;
            return;
        }
        if let Some(menu) = self.menu.as_mut() {
            if up && !self.prev_title_up {
                let _ = menu.move_cursor(-1);
            }
            if down && !self.prev_title_down {
                let _ = menu.move_cursor(1);
            }
        }
        if ok && !self.prev_title_ok {
            self.activate_title_menu();
        }
        self.prev_title_up = up;
        self.prev_title_down = down;
        self.prev_title_ok = ok;
    }

    fn activate_title_menu(&mut self) {
        let Some((cmd, _)) = self.menu.as_mut().map(MenuStack::activate) else {
            return;
        };
        let Some(cmd) = cmd else {
            return;
        };
        match cmd {
            MenuCommand::StartSkill(level) => {
                self.skill = Skill::new(level);
                self.game.options.skill = self.skill;
                if let Some(start) = self
                    .menu
                    .as_ref()
                    .and_then(MenuStack::start_map)
                    .map(str::to_owned)
                {
                    if let Ok(mut next) = load_map(&self.vfs, &start) {
                        ensure_nodes(&mut next);
                        self.map_base = next.clone();
                        self.map = next.clone();
                        self.game.set_pending_map(next);
                    }
                }
                match self.game.begin_pending_map() {
                    Ok(()) => {
                        self.menu = None;
                        self.sync_camera_from_player();
                        self.start_level_music();
                    }
                    Err(e) => {
                        web_sys::console::warn_1(&format!("room-web: start: {e}").into());
                    }
                }
            }
            MenuCommand::LoadGame => {
                self.feed.push("Load Game — desktop only", now_ms());
            }
            _ => {}
        }
    }

    fn build_title_canvas(&mut self, fb_w: f32, fb_h: f32) -> Canvas {
        let elapsed_ms = self.title_elapsed_ms();
        let (splash_def, logo_def) = self.title_anim_defs();
        let (pic, splash) = if splash_def.is_some() || logo_def.is_some() {
            let (name, splash) = title_screen_pic(splash_def, logo_def, elapsed_ms);
            (Some(name), splash)
        } else if self.vfs.find_any_ns("TITLEPIC").is_some() {
            (Some("TITLEPIC".into()), false)
        } else {
            (None, false)
        };
        let mut ui = UiDrawList::new();
        draw_title(&mut ui, 320.0, 200.0, pic.as_deref());
        if !splash {
            if let Some(menu) = &self.menu {
                menu.draw(&mut ui, self.title_frames as u32, None);
            }
        }
        for cmd in &ui.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = self.patches.ensure(&self.vfs, name);
            }
        }
        let mut canvas = Canvas::new();
        canvas.clear([0.0, 0.0, 0.0, 1.0]);
        let scale = (fb_w / 320.0).min(fb_h / 200.0);
        let ox = (fb_w - 320.0 * scale) * 0.5;
        let oy = (fb_h - 200.0 * scale) * 0.5;
        struct Sink<'a>(&'a mut Canvas);
        impl CanvasSink for Sink<'_> {
            fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                self.0.fill_rect(x, y, w, h, color);
            }
            fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                self.0.text(x, y, text, color);
            }
        }
        struct OffsetSink<'a> {
            inner: Sink<'a>,
            ox: f32,
            oy: f32,
        }
        impl CanvasSink for OffsetSink<'_> {
            fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
                self.inner.fill_rect(x + self.ox, y + self.oy, w, h, color);
            }
            fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]) {
                self.inner.text(x + self.ox, y + self.oy, text, color);
            }
        }
        blit_draw_list_full(
            &ui,
            320.0 * scale,
            200.0 * scale,
            None,
            Some(&self.patches),
            &mut OffsetSink {
                inner: Sink(&mut canvas),
                ox,
                oy,
            },
        );
        canvas
    }

    fn start_intermission_music(&mut self) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut music) = audio.music.try_borrow_mut() else {
            return;
        };
        if let Err(e) =
            play_first_music_lump(&mut music, &self.vfs, intermission_music_candidates())
        {
            web_sys::console::warn_1(&format!("room-web: intermission music: {e}").into());
        }
    }

    /// Load / restart map music into the Web Audio music player.
    pub fn start_level_music(&mut self) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut music) = audio.music.try_borrow_mut() else {
            return;
        };
        let map = self.game.map_name().to_owned();
        match play_map_music(&mut music, &self.vfs, &map, self.game.mapinfo()) {
            Ok(lump) => {
                web_sys::console::log_1(&format!("room-web: music {lump}").into());
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("room-web: music: {e}").into());
            }
        }
    }

    /// Status line for the HTML overlay.
    #[must_use]
    pub fn status_line(&self) -> String {
        match self.game.state {
            GameState::Level if self.is_spectator() && self.catching_up => format!(
                "Spectating {} — catching up with the match…",
                self.game.map_name()
            ),
            GameState::Level if self.is_spectator() => format!(
                "Spectating {} — G grid/follow · 1-4 or click pick player · Tab scores",
                self.game.map_name()
            ),
            GameState::Level if self.net.as_ref().is_some_and(|n| !n.peers_ready()) => format!(
                "Coop {} seat {} — waiting for peers to finish loading…",
                self.game.map_name(),
                self.local_seat
            ),
            GameState::Level if self.net.is_some() => format!(
                "Coop {} seat {} — click for audio/mouse · WASD · E use · LMB fire",
                self.game.map_name(),
                self.local_seat
            ),
            GameState::Level => format!(
                "Playing {} — click for audio/mouse · WASD · E use · LMB fire",
                self.game.map_name()
            ),
            GameState::Intermission => "Intermission…".into(),
            GameState::Finale => "Episode complete".into(),
            GameState::Title => "Title".into(),
        }
    }
}

impl PlaySession {
    /// Freeze the sim and fly freely (`Some`) or hand control back (`None`).
    pub fn set_edit_cam(&mut self, cam: Option<EditCam>) {
        self.edit_cam = cam;
        self.tic_accum = 0.0;
        self.mouse_yaw = 0;
        self.mouse_pitch = 0;
    }

    /// Pose the view currently shows (fly camera or local player).
    #[must_use]
    pub fn current_eye(&self) -> Option<EditCam> {
        if let Some(cam) = self.edit_cam {
            return Some(cam);
        }
        Some(EditCam {
            pos: self.interp_eye(self.local_seat)?,
            yaw: self.yaw,
            pitch: self.pitch,
        })
    }

    /// Fly-camera mouse look (CSS pixels while the look button is held).
    pub fn edit_look(&mut self, dx: f64, dy: f64) {
        if let Some(cam) = self.edit_cam.as_mut() {
            cam.yaw -= dx * 0.004;
            cam.pitch = (cam.pitch - dy * 0.004).clamp(-1.55, 1.55);
        }
    }

    fn fly_edit_cam(&mut self, dt: f32) {
        self.mouse_yaw = 0;
        self.mouse_pitch = 0;
        self.weapon = 0;
        let held = &self.held;
        let Some(cam) = self.edit_cam.as_mut() else {
            return;
        };
        if held.contains("left") {
            cam.yaw += f64::from(dt) * 2.0;
        }
        if held.contains("right") {
            cam.yaw -= f64::from(dt) * 2.0;
        }
        let fwd = look_dir(cam.yaw, cam.pitch);
        let right = Vec3::new(cam.yaw.sin() as f32, -cam.yaw.cos() as f32, 0.0);
        let mut v = Vec3::ZERO;
        for (key, dir) in [
            ("w", fwd),
            ("s", -fwd),
            ("d", right),
            ("a", -right),
            ("up", Vec3::Z),
            ("use", Vec3::Z),
            ("down", -Vec3::Z),
        ] {
            if held.contains(key) {
                v += dir;
            }
        }
        let boost = if held.contains("shift") { 3.0 } else { 1.0 };
        cam.pos += v.normalize_or_zero() * EDIT_FLY_SPEED * boost * dt;
    }

    /// Swap in edited geometry (nodes built).
    ///
    /// `live`: topology and things are unchanged, so the running world is
    /// patched in place (heights, light, textures, flags, specials). Otherwise
    /// the level restarts; a playing user respawns where they stood.
    pub fn editor_apply(&mut self, geom: MapGeometry, live: bool) -> Result<(), String> {
        if live {
            if let Some(world) = self.game.world_mut() {
                for (i, (new, old)) in geom.sectors.iter().zip(&self.map_base.sectors).enumerate() {
                    let Some(d) = world.sectors.get_mut(i) else {
                        continue;
                    };
                    if new.floor_height != old.floor_height {
                        d.floor_z = new.floor_height;
                        d.prev_floor_z = new.floor_height;
                    }
                    if new.ceiling_height != old.ceiling_height {
                        d.ceiling_z = new.ceiling_height;
                        d.prev_ceiling_z = new.ceiling_height;
                    }
                    if new.light != old.light {
                        d.light = new.light;
                    }
                }
                world.map = Arc::new(geom.clone());
            }
            self.map_base = geom.clone();
            self.map = geom;
            if self.edit_cam.is_none() {
                self.sync_map_from_world();
            }
            return Ok(());
        }
        let at = if self.edit_cam.is_none() {
            self.local_pose_i16()
        } else {
            None
        };
        self.restart_level(geom, at)
    }

    /// Restart on `geom`, optionally moving player 1's start to `(x, y, angle°)`.
    pub fn restart_level(
        &mut self,
        geom: MapGeometry,
        at: Option<(i32, i32, i16)>,
    ) -> Result<(), String> {
        let mut start = geom.clone();
        if let Some((x, y, angle)) = at {
            if let Some(t) = start.things.iter_mut().find(|t| t.type_id == 1) {
                t.x = clamp_i16(f64::from(x));
                t.y = clamp_i16(f64::from(y));
                t.angle = angle;
            }
        }
        self.game
            .start_custom_map(start, self.skill)
            .map_err(|e| e.to_string())?;
        self.map_base = geom.clone();
        self.map = geom;
        self.prev_eyes = [None; VIEW_SEATS];
        self.cur_eyes = [None; VIEW_SEATS];
        self.tic_accum = 0.0;
        self.intermission_wait = 0;
        if self.edit_cam.is_none() {
            self.sync_camera_from_player();
        }
        Ok(())
    }

    /// Swap class definitions (editor actor values). The state table must be
    /// the one the world already uses: live actors hold state ids into it.
    pub fn replace_defs(&mut self, data: EngineData) {
        self.game.set_defs(data.classes, data.states);
        let classes = Arc::clone(self.game.classes());
        if let Some(world) = self.game.world_mut() {
            world.classes = classes;
        }
    }

    /// Mod log lines (and load errors) since the last call.
    pub fn drain_script_log(&mut self) -> Vec<String> {
        self.game.drain_script_log()
    }

    /// Loaded WASM mod module names.
    #[must_use]
    pub fn script_mod_names(&self) -> Vec<String> {
        self.game.script_mod_names()
    }

    /// Play a music lump by name (`""` stops).
    pub fn play_music_named(&mut self, name: &str) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut music) = audio.music.try_borrow_mut() else {
            return;
        };
        if name.is_empty() {
            music.stop();
        } else if let Err(e) = play_music_lump(&mut music, &self.vfs, name) {
            web_sys::console::warn_1(&format!("room-web: music {name}: {e}").into());
        }
    }

    /// Pick cylinders for the map's things (class radius / height).
    #[must_use]
    pub fn thing_boxes(&self) -> Vec<crate::editor_pick::ThingBox> {
        (0..self.map.things.len())
            .filter_map(|i| self.thing_box(i))
            .collect()
    }

    /// Pick cylinder for map thing `index`.
    #[must_use]
    pub fn thing_box(&self, index: usize) -> Option<crate::editor_pick::ThingBox> {
        let t = self.map.things.get(index)?;
        let (x, y) = (f64::from(t.x), f64::from(t.y));
        let class = self.game.world().and_then(|w| {
            w.classes
                .find_doomednum(i32::from(t.type_id))
                .and_then(|id| w.classes.get(id))
        });
        let floor = room_nodebuild::point_in_sector(&self.map, x, y)
            .and_then(|s| self.map.sectors.get(s.raw() as usize))
            .map_or(0.0, |s| s.floor_height);
        Some(crate::editor_pick::ThingBox {
            index,
            pos: glam::DVec3::new(x, y, floor + f64::from(t.z)),
            radius: class.map_or(16.0, |c| c.radius.max(8.0)),
            height: class.map_or(32.0, |c| c.height.max(16.0)),
        })
    }
}

fn look_dir(yaw: f64, pitch: f64) -> Vec3 {
    let cp = pitch.cos();
    let forward = Vec3::new(
        (yaw.cos() * cp) as f32,
        (yaw.sin() * cp) as f32,
        pitch.sin() as f32,
    )
    .normalize_or_zero();
    if forward.length_squared() < 1e-6 {
        Vec3::X
    } else {
        forward
    }
}

/// Integer pixel panes: 2 side by side, 3-4 as a 2×2 grid.
fn pane_rects(n: usize, fb_w: f32, fb_h: f32) -> Vec<[f32; 4]> {
    let (cols, rows) = if n <= 2 { (n.max(1), 1) } else { (2, 2) };
    let cell_w = ((fb_w - PANE_GAP * (cols as f32 - 1.0)) / cols as f32).floor();
    let cell_h = ((fb_h - PANE_GAP * (rows as f32 - 1.0)) / rows as f32).floor();
    (0..n.min(cols * rows))
        .map(|i| {
            let (c, r) = ((i % cols) as f32, (i / cols) as f32);
            [
                c * (cell_w + PANE_GAP),
                r * (cell_h + PANE_GAP),
                cell_w.max(1.0),
                cell_h.max(1.0),
            ]
        })
        .collect()
}

fn clamp_i16(v: f64) -> i16 {
    v.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
}

fn load_genmidi(vfs: &Vfs) -> Option<GenMidiBank> {
    let id = vfs
        .find("GENMIDI", Namespace::Global)
        .or_else(|| vfs.find_any_ns("GENMIDI"))?;
    let bytes = vfs.read(id).ok()?;
    GenMidiBank::parse(&bytes).ok()
}

fn resolve_start_map(vfs: &Vfs, requested: &str) -> String {
    let has =
        |name: &str| vfs.find(name, Namespace::Global).is_some() || vfs.find_any_ns(name).is_some();
    if !requested.is_empty() && has(requested) {
        return requested.to_owned();
    }
    for cand in ["E1M1", "MAP01"] {
        if has(cand) {
            return (*cand).to_owned();
        }
    }
    if requested.is_empty() {
        "E1M1".into()
    } else {
        requested.to_owned()
    }
}

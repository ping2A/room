//! Browser Doom / Freedoom play session (Headless + materials).

use crate::netplay::NetPlay;
use crate::web_audio::WebAudio;
use bytes::Bytes;
use room_audio::ENGINE_SAMPLE_RATE;
use room_defs::{DefinitionsBuilder, GlDefs};
use room_game::{
    GameOptions, GameState, Headless, Skill, SoundSystem, load_embedded_engine_data,
    mount_embedded_engine, play_map_music,
};
use room_input::{InputSample, Key, build_ticcmd};
use room_map::{MapGeometry, load_map};
use room_music::{GenMidiBank, MidiDevice};
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, Canvas, DynLightInstance, MaterialCatalog, SpriteInstance,
    sprite_lump_candidates_rot, sprite_rotation_digit,
};
use room_sim::{
    Buttons, FLAG_COUNTKILL, FLAG_DONTDRAW, FLAG_FLOAT, FLAG_NOGRAVITY, FLAG_SPAWNCEILING, TicCmd,
};
use room_ui::{
    CanvasSink, DoomStatusBar, DrawCmd, HudState, PatchCache, StatusBar, UiDrawList,
    blit_draw_list_full, draw_weapon,
};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use glam::Vec3;
use std::collections::BTreeSet;
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
    /// Held movement / action keys.
    held: BTreeSet<&'static str>,
    /// One-shot weapon slot `1..=7`.
    weapon: u8,
    /// Mouse yaw counts this tic (public for mid-tic look preview).
    pub mouse_yaw: i32,
    /// Mouse pitch counts this tic (public for mid-tic look preview).
    pub mouse_pitch: i32,
    /// Attack held (LMB / Ctrl).
    attack: bool,
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
        }
    }
}

impl PlaySession {
    /// Mount embedded `room.pk3` + IWAD bytes and start `map` (or E1M1/MAP01).
    pub fn start(iwad: &[u8], map_name: Option<&str>) -> Result<Self, String> {
        Self::start_with(
            iwad,
            PlayStartOpts {
                map_name: map_name.map(str::to_owned),
                ..PlayStartOpts::default()
            },
        )
    }

    /// Start with coop / lockstep options.
    pub fn start_with(iwad: &[u8], opts: PlayStartOpts) -> Result<Self, String> {
        let builder = mount_embedded_engine(VfsBuilder::new()).map_err(|e| e.to_string())?;
        let vfs = builder
            .mount_memory("doom1.wad", Bytes::copy_from_slice(iwad))
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;

        let requested = opts.map_name.as_deref().unwrap_or("").trim();
        let map_name = resolve_start_map(&vfs, requested);
        let mut map = load_map(&vfs, &map_name).map_err(|e| e.to_string())?;
        ensure_nodes(&mut map);

        let skill = Skill::new(opts.skill);
        let coop = opts.coop_players.clamp(1, 4);
        let mut game = Headless::new(GameOptions {
            skill,
            rng_seed: opts.rng_seed,
            record_demo: false,
            coop_players: coop,
            ..GameOptions::default()
        });

        let mut gldefs = GlDefs::default();
        if let Ok(defs) = DefinitionsBuilder::new(&vfs).build() {
            game.set_complevel(defs.complevel);
            game.set_mapinfo(Arc::new(defs.mapinfo));
            game.set_terrain(&defs.terrain);
            game.set_sndseq(&defs.sndseq);
            game.set_lockdefs(&defs.lockdefs, Some("Doom"));
            game.set_decaldef(&defs.decaldef);
            game.set_keyconf(&defs.keyconf);
            game.set_menudef(defs.menudef.clone());
            gldefs = defs.gldefs;
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

        let data = load_embedded_engine_data().map_err(|e| e.to_string())?;
        game.set_defs(data.classes, data.states);
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

        game.start_custom_map(map.clone(), skill)
            .map_err(|e| e.to_string())?;

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
        Ok(Self {
            vfs,
            game,
            map_base: map.clone(),
            map,
            materials,
            patches,
            gldefs,
            held: BTreeSet::new(),
            weapon: 0,
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
        })
    }

    /// Whether this session is lockstep coop.
    #[must_use]
    pub fn is_net(&self) -> bool {
        self.net.is_some()
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
                // OPL + IWAD GENMIDI (no SoftFont fetch required).
                if let Some(bank) = load_genmidi(&self.vfs) {
                    if let Ok(mut music) = audio.music.try_borrow_mut() {
                        music.set_sample_rate(rate);
                        music.set_genmidi(bank);
                        music.set_mididevice(MidiDevice::Opl);
                        let map = self.game.map_name().to_owned();
                        match play_map_music(&mut music, &self.vfs, &map, None) {
                            Ok(lump) => {
                                web_sys::console::log_1(
                                    &format!("room-web: music {lump} (OPL)").into(),
                                );
                            }
                            Err(e) => {
                                web_sys::console::warn_1(
                                    &format!("room-web: music: {e}").into(),
                                );
                            }
                        }
                    }
                } else {
                    web_sys::console::warn_1(&"room-web: no GENMIDI — music silent".into());
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
        names.insert("SKY1".into());
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

    /// Key down/up from `KeyboardEvent.code` (e.g. `KeyW`).
    pub fn set_key(&mut self, code: &str, down: bool) {
        let token: Option<&'static str> = match code {
            "KeyW" | "ArrowUp" => Some("w"),
            "KeyS" | "ArrowDown" => Some("s"),
            "KeyA" => Some("a"),
            "KeyD" => Some("d"),
            "ArrowLeft" => Some("left"),
            "ArrowRight" => Some("right"),
            "ShiftLeft" | "ShiftRight" => Some("shift"),
            "KeyE" | "Space" => Some("use"),
            "ControlLeft" | "ControlRight" => Some("attack"),
            "Digit1" | "Numpad1" => {
                if down {
                    self.weapon = 1;
                }
                None
            }
            "Digit2" | "Numpad2" => {
                if down {
                    self.weapon = 2;
                }
                None
            }
            "Digit3" | "Numpad3" => {
                if down {
                    self.weapon = 3;
                }
                None
            }
            "Digit4" | "Numpad4" => {
                if down {
                    self.weapon = 4;
                }
                None
            }
            "Digit5" | "Numpad5" => {
                if down {
                    self.weapon = 5;
                }
                None
            }
            "Digit6" | "Numpad6" => {
                if down {
                    self.weapon = 6;
                }
                None
            }
            "Digit7" | "Numpad7" => {
                if down {
                    self.weapon = 7;
                }
                None
            }
            _ => None,
        };
        if let Some(t) = token {
            if down {
                self.held.insert(t);
            } else {
                self.held.remove(t);
            }
            if t == "attack" {
                self.attack = down || self.held.contains("attack");
            }
        }
    }

    /// Mouse button (0 = primary).
    pub fn set_mouse_button(&mut self, button: i16, down: bool) {
        if button == 0 {
            self.attack = down;
        }
    }

    /// Accumulate pointer-lock mouse motion (CSS pixels).
    pub fn add_mouse_delta(&mut self, dx: f64, dy: f64) {
        // Match desktop (`room-app`): mouse right → look right ⇒ −dx yaw;
        // mouse up → look up ⇒ −dy pitch (unless invertmouse).
        self.mouse_yaw += (-dx).round() as i32;
        self.mouse_pitch += (-dy).round() as i32;
    }

    /// Advance sim at 35 Hz; returns `true` if at least one tic ran.
    pub fn tick_play(&mut self, dt: f32) -> bool {
        const TIC: f32 = 1.0 / 35.0;
        self.tic_accum = (self.tic_accum + dt).min(TIC * 3.0);
        let mut stepped = false;
        while self.tic_accum >= TIC {
            match self.game.state {
                GameState::Level => {
                    let cmds = if let Some(net) = self.net.as_mut() {
                        let yaw = self.mouse_yaw;
                        let pitch = self.mouse_pitch;
                        let Some(cmds) = net.try_advance(
                            &self.held,
                            self.attack,
                            self.weapon,
                            yaw,
                            pitch,
                        ) else {
                            break;
                        };
                        self.mouse_yaw = 0;
                        self.mouse_pitch = 0;
                        self.weapon = 0;
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
                    self.pump_audio();
                    self.sync_map_from_world();
                    self.sync_camera_from_player();
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
                _ => {
                    self.tic_accum -= TIC;
                }
            }
        }
        stepped
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
        match self.weapon {
            1 => keys.push(Key::Weapon1),
            2 => keys.push(Key::Weapon2),
            3 => keys.push(Key::Weapon3),
            4 => keys.push(Key::Weapon4),
            5 => keys.push(Key::Weapon5),
            6 => keys.push(Key::Weapon6),
            7 => keys.push(Key::Weapon7),
            _ => {}
        }
        self.weapon = 0;
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
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let seat = usize::from(self.local_seat);
        let listener = self
            .game
            .world()
            .and_then(|w| w.players.get(seat).and_then(|p| p.as_ref()))
            .and_then(|p| p.mo)
            .and_then(|id| self.game.world().and_then(|w| w.actors.get(id)))
            .map(|a| a.pos);
        if let Ok(mut mix) = audio.mixer.try_borrow_mut() {
            if let Some(sounds) = self.game.sounds.as_ref() {
                mix.bank.merge_missing(&sounds.bank);
            }
            if let Some(pos) = listener {
                mix.listener = pos;
            }
            mix.apply(&events);
        }
    }

    /// Rasterize weapon + classic `STBAR` HUD into a CPU canvas for `CanvasGpu`.
    pub fn build_hud_canvas(&mut self, fb_w: f32, fb_h: f32) -> Canvas {
        let mut ui = UiDrawList::new();
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
        }
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
            &mut ui,
            &hud.weapon_patch,
            hud.fullscreen,
            soft,
            hud.bob_x,
            hud.bob_y,
        );
        if !hud.flash_patch.is_empty() {
            let soft_flash = self.patches.get(&hud.flash_patch);
            draw_weapon(
                &mut ui,
                &hud.flash_patch,
                hud.fullscreen,
                soft_flash,
                hud.bob_x,
                hud.bob_y,
            );
        }
        DoomStatusBar.draw(&hud, &mut ui);

        // Prefetch every patch the draw list needs (STBAR, STTNUM*, mugshot, keys).
        for cmd in &ui.cmds {
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
        canvas
    }

    fn sync_map_from_world(&mut self) {
        let Some(world) = self.game.world() else {
            return;
        };
        let (extralight, infrared) = world
            .players
            .first()
            .and_then(|p| p.as_ref())
            .map(|p| (p.extralight, p.infrared))
            .unwrap_or((0, false));
        let light_boost = (extralight.clamp(0, 2) * 16 + i32::from(infrared) * 64) as i16;
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
    }

    fn sync_camera_from_player(&mut self) {
        let Some(world) = self.game.world() else {
            return;
        };
        let seat = usize::from(self.local_seat);
        let Some(p) = world.players.get(seat).and_then(|p| p.as_ref()) else {
            return;
        };
        let view_id = p.camera.or(p.mo);
        let Some(vid) = view_id else {
            return;
        };
        let Some(a) = world.actors.get(vid) else {
            return;
        };
        self.yaw = a.angle.radians();
        self.pitch = a.pitch.radians();
    }

    /// Eye + forward for this frame (includes pending mouse look preview).
    #[must_use]
    pub fn view_pose(&self) -> Option<(Vec3, Vec3)> {
        let scale =
            f64::from(room_input::MOUSE_YAW_SCALE) * std::f64::consts::TAU / 65536.0;
        let yaw = self.yaw + f64::from(self.mouse_yaw) * scale;
        let pitch = (self.pitch + f64::from(self.mouse_pitch) * scale).clamp(-1.4, 1.4);
        let world = self.game.world()?;
        let seat = usize::from(self.local_seat);
        let p = world.players.get(seat).and_then(|p| p.as_ref())?;
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
        let eye = Vec3::new(a.pos.x() as f32, a.pos.y() as f32, z as f32);
        let cp = pitch.cos();
        let mut forward = Vec3::new(
            (yaw.cos() * cp) as f32,
            (yaw.sin() * cp) as f32,
            pitch.sin() as f32,
        )
        .normalize_or_zero();
        if forward.length_squared() < 1e-6 {
            forward = Vec3::X;
        }
        Some((eye, forward))
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
        let Some(world) = self.game.world() else {
            return Vec::new();
        };
        let seat = usize::from(self.local_seat);
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
        for (x, y, z, angle_deg, radius, height, scale_x, scale_y, light, alpha, sprite, frame, flags)
            in actors
        {
            let mut w = (radius * 2.0 * scale_x.abs()).clamp(1.0, 256.0);
            let mut h = (height * scale_y.abs()).clamp(1.0, 256.0);
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
                        let pw = (f64::from(patch.width) * scale_x.abs()).clamp(1.0, 256.0);
                        let ph = (f64::from(patch.height) * scale_y.abs()).clamp(1.0, 256.0);
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
                        anchor_u = if pw > 1e-6 {
                            (left / pw) as f32
                        } else {
                            0.5
                        };
                        texture = name.clone();
                        flip_u = *flip;
                        break;
                    }
                }
                if texture.is_empty() {
                    if let Some((name, flip)) = candidates.first() {
                        texture = name.clone();
                        flip_u = *flip;
                    }
                    if grounded {
                        z_bottom = z + 1.0;
                    }
                }
            } else if grounded {
                z_bottom = z + 1.0;
            }
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
            .or_else(|| {
                Headless::default_next_map(self.game.map_name(), false)
            });
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

    /// Load / restart map music into the Web Audio music player.
    pub fn start_level_music(&mut self) {
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let Ok(mut music) = audio.music.try_borrow_mut() else {
            return;
        };
        let map = self.game.map_name().to_owned();
        match play_map_music(&mut music, &self.vfs, &map, None) {
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

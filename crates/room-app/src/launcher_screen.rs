//! The launcher screen (docs/41 M4): pick a game, tick its mods, change the
//! touch settings, see why a file was skipped or a start failed. Works by
//! touch (tap a row, drag to scroll), keyboard and controller (up / down /
//! enter / back). Drawn with the built-in font, so it needs no game data.

use crate::config::AppConfig;
use crate::library::{Engine, Entry, Library};
use crate::ui_text::{self, Face};
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Navigation input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nav {
    Up,
    Down,
    Enter,
    Back,
}

/// What the app should do after an input.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Nothing outside the screen changes.
    None,
    /// Start this game.
    Start(Box<AppConfig>),
    /// Set a CVar (console syntax value) and save the settings.
    SetCvar(String, String),
    /// Look for co-op games on the local network (docs/41 M7).
    FindLan,
    /// Stop waiting for players / for the host.
    CancelLobby,
}

/// Frames a launcher speed test measures (about 10 s at 60 fps).
const SPEED_TEST_FRAMES: u64 = 600;

/// Co-op players a phone can host (UDP lobby: 2 to 4).
const HOST_SIZES: [u8; 3] = [2, 3, 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Games,
    Mods,
    Settings,
    /// Host or join a co-op game.
    Together,
    /// Type a host's address.
    Address,
    /// A lobby is running (hosting or joining).
    Waiting,
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    PickGame(usize),
    OpenSettings,
    ToggleMod(PathBuf),
    Play,
    Back,
    Cycle(&'static str, &'static [&'static str]),
    /// Information only (problems, hints).
    Info,
    OpenTogether,
    NextGame,
    SpeedTest,
    Host(u8),
    FindLan,
    JoinAt(SocketAddr),
    OpenAddress,
    Type(char),
    Erase,
    JoinTyped,
    CancelLobby,
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    label: String,
    detail: String,
    action: Action,
}

/// Touch settings on the settings page: CVar, label, values to cycle through.
const SETTINGS: [(&str, &str, &[&str]); 12] = [
    ("touch_size", "Button size", &["0.8", "1", "1.2", "1.4"]),
    (
        "touch_opacity",
        "Button opacity",
        &["0.4", "0.6", "0.8", "1"],
    ),
    (
        "touch_look",
        "Look speed",
        &["0.5", "0.75", "1", "1.5", "2"],
    ),
    ("touch_invert", "Invert up/down look", &["false", "true"]),
    ("touch_lefty", "Left-handed layout", &["false", "true"]),
    ("touch_tapfire", "Double-tap to fire", &["false", "true"]),
    (
        "touch_jumpduck",
        "Jump and crouch buttons",
        &["false", "true"],
    ),
    ("touch_haptics", "Vibration", &["false", "true"]),
    (
        "r_scale",
        "3D resolution",
        &["0.5", "0.6", "0.75", "0.85", "1"],
    ),
    (
        "r_dynres",
        "Lower resolution to keep speed",
        &["false", "true"],
    ),
    // 0: no limit. 60 saves battery; 120 only helps on 120 Hz screens.
    ("vid_maxfps", "Frame rate limit", &["30", "60", "120", "0"]),
    ("vid_fps", "Show frame rate", &["false", "true"]),
];

/// The CVars the settings page reads.
pub(crate) const SETTING_CVARS: [&str; 12] = [
    "touch_size",
    "touch_opacity",
    "touch_look",
    "touch_invert",
    "touch_lefty",
    "touch_tapfire",
    "touch_jumpduck",
    "touch_haptics",
    "r_scale",
    "r_dynres",
    "vid_maxfps",
    "vid_fps",
];

/// Row height (dp).
const ROW_DP: f32 = 46.0;
/// Widest the list gets (dp).
const MAX_WIDTH_DP: f32 = 460.0;

/// A draw command in framebuffer pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Draw {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    Image {
        x: f32,
        y: f32,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
    /// The game's backdrop over the whole screen (cover), drawn on the GPU
    /// under everything else, at `alpha` (cross-fades).
    Backdrop {
        key: String,
        w: u32,
        h: u32,
        rgba: std::sync::Arc<Vec<u8>>,
        alpha: f32,
    },
}

/// Cache key for a rendered line: text, face, size (1/8 px), colour.
type TextKey = (String, crate::ui_text::Face, u32, [u8; 4]);
/// A rendered line: width, height, RGBA.
type TextImage = (u32, u32, Vec<u8>);

/// Text colours.
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BONE: [u8; 4] = [236, 232, 224, 255];
const MUTED: [u8; 4] = [150, 145, 138, 255];
/// Accent before a backdrop is known.
const DEFAULT_ACCENT: [u8; 3] = [240, 90, 40];

/// Where things go on a `w × h` screen at `scale` px per dp.
struct Layout {
    x: f32,
    col_w: f32,
    mark_y: f32,
    mark_px: f32,
    hero_y: f32,
    hero_px: f32,
    meta_y: f32,
    meta_px: f32,
    list_top: f32,
    row_h: f32,
    row_px: f32,
    detail_px: f32,
    info_px: f32,
    visible: usize,
}

impl Layout {
    fn new(w: f32, h: f32, scale: f32) -> Self {
        let u = scale.max(0.5);
        let x = (28.0 * u).min(w * 0.06).max(8.0 * u).round();
        let col_w = (w * 0.46)
            .max(300.0 * u)
            .min(MAX_WIDTH_DP * u)
            .min(w - 2.0 * x)
            .max(1.0);
        let mark_px = 17.0 * u;
        let mark_y = 14.0 * u;
        // The title shrinks on short screens (phones in landscape).
        let hero_px = (h * 0.12).clamp(34.0 * u, 60.0 * u);
        let hero_y = mark_y + ui_text::line_height(Face::Display, mark_px) + 2.0 * u;
        let meta_px = 15.0 * u;
        let meta_y = hero_y + ui_text::line_height(Face::Display, hero_px) - 2.0 * u;
        let list_top = (meta_y + ui_text::line_height(Face::Body, meta_px) + 14.0 * u).round();
        let row_h = (ROW_DP * u).round();
        let visible = (((h - list_top - 12.0 * u) / row_h).floor() as usize).max(1);
        Self {
            x,
            col_w,
            mark_y,
            mark_px,
            hero_y,
            hero_px,
            meta_y,
            meta_px,
            list_top,
            row_h,
            row_px: 18.0 * u,
            detail_px: 14.5 * u,
            info_px: 15.0 * u,
            visible,
        }
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Move `value` toward `target` by `follow` (0–1) of the way; snap when
/// close or unset. Returns the new value.
fn spring(value: &mut Option<f32>, target: f32, follow: f32) -> f32 {
    let v = match *value {
        Some(v) if (v - target).abs() > 0.5 => v + (target - v) * follow,
        _ => target,
    };
    *value = Some(v);
    v
}

/// An on/off switch: a rounded track (accent when on) and a round knob.
fn switch_draw(out: &mut Vec<Draw>, x: f32, y: f32, w: f32, h: f32, on: bool, accent: [f32; 3]) {
    let (iw, ih) = (w.max(4.0) as u32, h.max(4.0) as u32);
    let mut rgba = vec![0u8; (iw * ih * 4) as usize];
    let r = ih as f32 / 2.0;
    let track = if on {
        [accent[0], accent[1], accent[2], 1.0]
    } else {
        [1.0, 1.0, 1.0, 0.22]
    };
    let knob_r = r - (ih as f32 * 0.14).max(1.5);
    let knob_x = if on { iw as f32 - r } else { r };
    for yy in 0..ih {
        for xx in 0..iw {
            let (px, py) = (xx as f32 + 0.5, yy as f32 + 0.5);
            // Distance outside the capsule (negative inside).
            let cx = px.clamp(r, iw as f32 - r);
            let d_track = ((px - cx).powi(2) + (py - r).powi(2)).sqrt() - r;
            let d_knob = ((px - knob_x).powi(2) + (py - r).powi(2)).sqrt() - knob_r;
            let a_track = (0.5 - d_track).clamp(0.0, 1.0) * track[3];
            let a_knob = (0.5 - d_knob).clamp(0.0, 1.0);
            // Knob over track (straight alpha "over").
            let a = a_knob + a_track * (1.0 - a_knob);
            if a <= 0.0 {
                continue;
            }
            let mix = |t: f32| (a_knob + t * a_track * (1.0 - a_knob)) / a;
            let i = ((yy * iw + xx) * 4) as usize;
            for c in 0..3 {
                rgba[i + c] = (mix(track[c]) * 255.0).round() as u8;
            }
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out.push(Draw::Image {
        x,
        y,
        w: iw,
        h: ih,
        rgba,
    });
}

/// A mod's check box: filled with a tick when on, an outline when off.
fn check_box(
    out: &mut Vec<Draw>,
    x: f32,
    y: f32,
    size: f32,
    scale: f32,
    on: bool,
    accent: [f32; 3],
) {
    let line = (1.5 * scale).round().max(1.0);
    if on {
        out.push(Draw::Rect {
            x,
            y,
            w: size,
            h: size,
            color: [accent[0], accent[1], accent[2], 1.0],
        });
        // Tick: two strokes of small squares.
        let n = (size as u32).max(8);
        let mut rgba = vec![0u8; (n * n * 4) as usize];
        let stroke = (size * 0.12).max(1.5);
        let mut dot = |cx: f32, cy: f32| {
            for yy in 0..n {
                for xx in 0..n {
                    let d =
                        ((xx as f32 + 0.5 - cx).powi(2) + (yy as f32 + 0.5 - cy).powi(2)).sqrt();
                    let a = (stroke - d + 0.5).clamp(0.0, 1.0);
                    let i = ((yy * n + xx) * 4) as usize;
                    if a > 0.0 && f32::from(rgba[i + 3]) < a * 255.0 {
                        rgba[i..i + 3].copy_from_slice(&[20, 18, 16]);
                        rgba[i + 3] = (a * 255.0) as u8;
                    }
                }
            }
        };
        let s = n as f32;
        let (a, b, c) = (
            (0.24 * s, 0.52 * s),
            (0.42 * s, 0.70 * s),
            (0.78 * s, 0.30 * s),
        );
        for k in 0..=24 {
            let t = k as f32 / 24.0;
            dot(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            dot(b.0 + (c.0 - b.0) * t, b.1 + (c.1 - b.1) * t);
        }
        out.push(Draw::Image {
            x,
            y,
            w: n,
            h: n,
            rgba,
        });
    } else {
        let c = [1.0, 1.0, 1.0, 0.55];
        out.push(Draw::Rect {
            x,
            y,
            w: size,
            h: line,
            color: c,
        });
        out.push(Draw::Rect {
            x,
            y: y + size - line,
            w: size,
            h: line,
            color: c,
        });
        out.push(Draw::Rect {
            x,
            y,
            w: line,
            h: size,
            color: c,
        });
        out.push(Draw::Rect {
            x: x + size - line,
            y,
            w: line,
            h: size,
            color: c,
        });
    }
}

/// Launcher state.
pub(crate) struct LauncherScreen {
    library: Library,
    /// Where the player can copy game files (shown on the games page).
    hint: Option<String>,
    page: Page,
    cursor: usize,
    /// First visible row.
    scroll: usize,
    game: usize,
    mods: BTreeSet<PathBuf>,
    /// Last start-up error, shown in red until the next action.
    pub(crate) error: Option<String>,
    /// "Loading …" while a picked game starts (drawn before the app blocks
    /// on loading it).
    pub(crate) notice: Option<String>,
    /// Games found on the local network (`None`: not searched yet).
    pub(crate) lan_games: Option<Vec<crate::lan::LanGame>>,
    /// A search is running.
    pub(crate) lan_searching: bool,
    /// What the lobby is doing ("Waiting for players …"), on the waiting page.
    pub(crate) waiting: Option<String>,
    /// Host address typed on the address page.
    address: String,
    text_cache: HashMap<TextKey, TextImage>,
    drag: Option<(u64, f32, f32, f32, bool)>,
    /// Row under a finger (drawn pressed).
    pressed: Option<usize>,
    /// Backdrops by game path.
    backdrops: HashMap<PathBuf, crate::launcher_art::Backdrop>,
    /// Motion state of the last draw.
    motion: Motion,
}

/// What the screen is animating between draws.
#[derive(Default)]
struct Motion {
    last: Option<std::time::Instant>,
    /// Highlight top (px, list space) and the list's scroll (px).
    highlight: Option<f32>,
    scroll: Option<f32>,
    /// Page shown and when it opened (slide in).
    page: Option<(Page, std::time::Instant)>,
    /// Backdrop shown, the one before it, and when the change started.
    backdrop: Option<(PathBuf, Option<PathBuf>, std::time::Instant)>,
    /// Game whose title the games page shows (the last game row hovered).
    hero_game: usize,
}

impl LauncherScreen {
    pub(crate) fn new(library: Library, hint: Option<String>) -> Self {
        let mut screen = Self {
            library,
            hint,
            page: Page::Games,
            cursor: 0,
            scroll: 0,
            game: 0,
            mods: BTreeSet::new(),
            error: None,
            notice: None,
            lan_games: None,
            lan_searching: false,
            waiting: None,
            address: String::new(),
            text_cache: HashMap::new(),
            drag: None,
            pressed: None,
            backdrops: HashMap::new(),
            motion: Motion::default(),
        };
        screen.tick_default_mods();
        screen
    }

    /// Tick the selected game's default mods (the ones shipped with the app).
    fn tick_default_mods(&mut self) {
        self.mods = self
            .selected_game()
            .map(|g| self.library.default_mods(g).into_iter().collect())
            .unwrap_or_default();
    }

    /// "Loading Freedoom: Phase 2 + Brutal Doom …" for the current pick.
    fn loading_text(&self) -> String {
        if self.selected_game().is_none() {
            return "Loading …".into();
        }
        format!("Loading {} …", self.selection_text())
    }

    /// Join the co-op game at `addr` with the current pick.
    fn join(&mut self, addr: SocketAddr) -> Outcome {
        match self.start_config() {
            Some(mut config) => {
                config.join = Some(addr.to_string());
                Outcome::Start(Box::new(config))
            }
            None => Outcome::None,
        }
    }

    /// Wrap a start request: remember what is loading, to show it.
    fn start(&mut self) -> Outcome {
        match self.start_config() {
            Some(config) => {
                self.notice = Some(self.loading_text());
                Outcome::Start(Box::new(config))
            }
            None => Outcome::None,
        }
    }

    fn selected_game(&self) -> Option<&Entry> {
        self.library.games.get(self.game)
    }

    fn rows(&self, cvar: &dyn Fn(&str) -> String) -> Vec<Row> {
        let mut rows = Vec::new();
        match self.page {
            Page::Games => {
                for (i, g) in self.library.games.iter().enumerate() {
                    let mods = self.library.mods_for(g).len();
                    let mut detail = if g.bundled {
                        "included".to_owned()
                    } else if g.title == g.file {
                        String::new()
                    } else {
                        g.file.clone()
                    };
                    if mods > 0 {
                        let sep = if detail.is_empty() { "" } else { " + " };
                        let s = if mods == 1 { "" } else { "s" };
                        detail.push_str(&format!("{sep}{mods} mod{s}"));
                    }
                    rows.push(Row {
                        label: g.title.clone(),
                        detail,
                        action: Action::PickGame(i),
                    });
                }
                if self.library.games.is_empty() {
                    rows.push(Row {
                        label: "No games found".into(),
                        detail: String::new(),
                        action: Action::Info,
                    });
                }
                if !self.library.games.is_empty() {
                    rows.push(Row {
                        label: "Play together".into(),
                        detail: "co-op on this Wi-Fi".into(),
                        action: Action::OpenTogether,
                    });
                }
                rows.push(Row {
                    label: "Settings".into(),
                    detail: "controls, resolution, frame rate".into(),
                    action: Action::OpenSettings,
                });
                for (file, why) in &self.library.problems {
                    rows.push(Row {
                        label: format!("Skipped {file}"),
                        detail: why.clone(),
                        action: Action::Info,
                    });
                }
                if let Some(hint) = &self.hint {
                    rows.push(Row {
                        label: "Add games and mods:".into(),
                        detail: String::new(),
                        action: Action::Info,
                    });
                    rows.push(Row {
                        label: hint.clone(),
                        detail: String::new(),
                        action: Action::Info,
                    });
                }
            }
            Page::Mods => {
                let title = self
                    .selected_game()
                    .map_or_else(String::new, |g| g.title.clone());
                rows.push(Row {
                    label: format!("Play {title}"),
                    detail: match self.mods.len() {
                        0 => "no mods".into(),
                        1 => "1 mod".into(),
                        n => format!("{n} mods"),
                    },
                    action: Action::Play,
                });
                if let Some(g) = self.selected_game() {
                    for m in self.library.mods_for(g) {
                        let on = self.mods.contains(&m.path);
                        rows.push(Row {
                            label: format!("[{}] {}", if on { "X" } else { " " }, m.title),
                            detail: format!("{} KB", m.size.div_ceil(1024)),
                            action: Action::ToggleMod(m.path.clone()),
                        });
                    }
                }
                rows.push(Row {
                    label: "Play together".into(),
                    detail: "co-op".into(),
                    action: Action::OpenTogether,
                });
                rows.push(Row {
                    label: "Back".into(),
                    detail: String::new(),
                    action: Action::Back,
                });
            }
            Page::Settings => {
                for (name, label, values) in SETTINGS {
                    rows.push(Row {
                        label: label.into(),
                        detail: shown(name, &cvar(name)),
                        action: Action::Cycle(name, values),
                    });
                }
                if self.selected_game().is_some() {
                    rows.push(Row {
                        label: "Speed test".into(),
                        detail: format!(
                            "{} frames of {}",
                            SPEED_TEST_FRAMES,
                            self.selection_text()
                        ),
                        action: Action::SpeedTest,
                    });
                }
                rows.push(Row {
                    label: "Back".into(),
                    detail: String::new(),
                    action: Action::Back,
                });
            }
            Page::Together => self.together_rows(&mut rows),
            Page::Address => {
                let shown = if self.address.is_empty() {
                    "host address, e.g. 192.168.1.20".to_owned()
                } else {
                    format!("{}_", self.address)
                };
                rows.push(Row {
                    label: shown,
                    detail: String::new(),
                    action: Action::Info,
                });
                if self.address_target().is_some() {
                    rows.push(Row {
                        label: format!("Join {}", self.address),
                        detail: String::new(),
                        action: Action::JoinTyped,
                    });
                }
                for c in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '.', ':'] {
                    rows.push(Row {
                        label: c.to_string(),
                        detail: String::new(),
                        action: Action::Type(c),
                    });
                }
                rows.push(Row {
                    label: "Delete".into(),
                    detail: String::new(),
                    action: Action::Erase,
                });
                rows.push(Row {
                    label: "Back".into(),
                    detail: String::new(),
                    action: Action::Back,
                });
            }
            Page::Waiting => {
                rows.push(Row {
                    label: self
                        .waiting
                        .clone()
                        .unwrap_or_else(|| "Connecting …".into()),
                    detail: String::new(),
                    action: Action::Info,
                });
                rows.push(Row {
                    label: "Cancel".into(),
                    detail: String::new(),
                    action: Action::CancelLobby,
                });
            }
        }
        rows
    }

    /// The co-op page: what will be played, host sizes, games found on the
    /// network, join by address.
    fn together_rows(&self, rows: &mut Vec<Row>) {
        rows.push(Row {
            label: self.selection_text(),
            detail: String::new(),
            action: Action::Info,
        });
        if self.library.games.len() > 1 {
            rows.push(Row {
                label: "Change game".into(),
                detail: String::new(),
                action: Action::NextGame,
            });
        }
        for n in HOST_SIZES {
            rows.push(Row {
                label: format!("Host: {n} players"),
                detail: format!("others join you, port {}", crate::lan::DEFAULT_PORT),
                action: Action::Host(n),
            });
        }
        rows.push(Row {
            label: "Find games on this Wi-Fi".into(),
            detail: if self.lan_searching {
                "searching …".into()
            } else {
                match &self.lan_games {
                    None => String::new(),
                    Some(g) if g.is_empty() => "none found".into(),
                    Some(g) => format!("{} found", g.len()),
                }
            },
            action: Action::FindLan,
        });
        let mine = self
            .start_config()
            .map(|c| crate::netplay::config_fingerprint(&c));
        for game in self.lan_games.iter().flatten() {
            rows.push(Row {
                label: format!(
                    "Join {} {} ({}/{})",
                    game.addr.ip(),
                    game.map,
                    game.players,
                    game.max_players
                ),
                detail: if Some(game.fingerprint) == mine {
                    "same game".into()
                } else {
                    "other game or mods".into()
                },
                action: Action::JoinAt(game.addr),
            });
        }
        rows.push(Row {
            label: "Join by address".into(),
            detail: String::new(),
            action: Action::OpenAddress,
        });
        rows.push(Row {
            label: "Back".into(),
            detail: String::new(),
            action: Action::Back,
        });
    }

    /// "Freedoom: Phase 2 + Brutal Doom" for the current pick.
    fn selection_text(&self) -> String {
        let Some(game) = self.selected_game() else {
            return "No game".into();
        };
        let mut text = game.title.clone();
        for m in self.library.mods_for(game) {
            if self.mods.contains(&m.path) {
                text.push_str(" + ");
                text.push_str(&m.title);
            }
        }
        text
    }

    /// The typed address as a join target (port 5029 when none is typed).
    fn address_target(&self) -> Option<SocketAddr> {
        let text = self.address.trim();
        if text.is_empty() {
            return None;
        }
        let with_port = if text.contains(':') {
            text.to_owned()
        } else {
            format!("{text}:{}", crate::lan::DEFAULT_PORT)
        };
        with_port.parse().ok()
    }

    /// Show the waiting page with `text` (the app updates it while the lobby runs).
    pub(crate) fn show_waiting(&mut self, text: String) {
        self.waiting = Some(text);
        self.notice = None;
        self.page = Page::Waiting;
        self.scroll = 0;
        self.cursor = 1;
    }

    /// The lobby ended without a game: back to the co-op page, with `error`
    /// if it failed.
    pub(crate) fn lobby_ended(&mut self, error: Option<String>) {
        self.waiting = None;
        self.error = error;
        self.page = Page::Together;
        self.scroll = 0;
        self.cursor = 0;
    }

    fn selectable(row: &Row) -> bool {
        row.action != Action::Info
    }

    fn open(&mut self, page: Page, cvar: &dyn Fn(&str) -> String) {
        self.page = page;
        self.scroll = 0;
        self.cursor = 0;
        if let Some(first) = self.rows(cvar).iter().position(Self::selectable) {
            self.cursor = first;
        }
    }

    fn start_config(&self) -> Option<AppConfig> {
        let g = self.selected_game()?;
        let mods: Vec<PathBuf> = self
            .library
            .mods_for(g)
            .iter()
            .filter(|m| self.mods.contains(&m.path))
            .flat_map(|m| std::iter::once(m.path.clone()).chain(m.companions.iter().cloned()))
            .collect();
        Some(match g.engine {
            Engine::Doom => AppConfig {
                iwad: Some(g.path.clone()),
                file: mods,
                play: true,
                from_launcher: true,
                ..AppConfig::default()
            },
            Engine::Duke => AppConfig {
                native_duke: true,
                iwad: Some(g.path.clone()),
                grp: mods,
                from_launcher: true,
                ..AppConfig::default()
            },
        })
    }

    fn activate(&mut self, index: usize, cvar: &dyn Fn(&str) -> String) -> Outcome {
        let rows = self.rows(cvar);
        let Some(row) = rows.get(index) else {
            return Outcome::None;
        };
        if !Self::selectable(row) {
            return Outcome::None;
        }
        self.cursor = index;
        self.error = None;
        self.notice = None;
        match row.action.clone() {
            Action::PickGame(i) => {
                if self.game != i {
                    self.game = i;
                    self.tick_default_mods();
                }
                let has_mods = self
                    .selected_game()
                    .is_some_and(|g| !self.library.mods_for(g).is_empty());
                if has_mods {
                    self.open(Page::Mods, cvar);
                    Outcome::None
                } else {
                    self.start()
                }
            }
            Action::Play => self.start(),
            Action::ToggleMod(path) => {
                if !self.mods.remove(&path) {
                    self.mods.insert(path);
                }
                Outcome::None
            }
            Action::OpenSettings => {
                self.open(Page::Settings, cvar);
                Outcome::None
            }
            Action::OpenTogether => {
                self.open(Page::Together, cvar);
                Outcome::None
            }
            Action::SpeedTest => match self.start_config() {
                Some(mut config) => {
                    config.bench_frames = Some(SPEED_TEST_FRAMES);
                    self.notice = Some(format!("Speed test: {} …", self.loading_text()));
                    Outcome::Start(Box::new(config))
                }
                None => Outcome::None,
            },
            Action::NextGame => {
                if !self.library.games.is_empty() {
                    self.game = (self.game + 1) % self.library.games.len();
                    self.tick_default_mods();
                    // A different game: earlier search results may not match.
                }
                Outcome::None
            }
            Action::Host(players) => match self.start_config() {
                Some(mut config) => {
                    config.host = Some(crate::lan::DEFAULT_PORT.to_string());
                    config.players = players;
                    Outcome::Start(Box::new(config))
                }
                None => Outcome::None,
            },
            Action::FindLan => {
                self.lan_searching = true;
                Outcome::FindLan
            }
            Action::JoinAt(addr) => self.join(addr),
            Action::OpenAddress => {
                self.open(Page::Address, cvar);
                Outcome::None
            }
            Action::Type(c) => {
                if self.address.len() < 40 {
                    self.address.push(c);
                }
                Outcome::None
            }
            Action::Erase => {
                self.address.pop();
                Outcome::None
            }
            Action::JoinTyped => match self.address_target() {
                Some(addr) => self.join(addr),
                None => Outcome::None,
            },
            Action::CancelLobby => Outcome::CancelLobby,
            Action::Back => self.back(cvar),
            Action::Cycle(name, values) => {
                let now = cvar(name);
                let i = values.iter().position(|v| same_value(v, &now));
                let next = values[i.map_or(0, |i| (i + 1) % values.len())];
                Outcome::SetCvar(name.to_owned(), next.to_owned())
            }
            Action::Info => Outcome::None,
        }
    }

    fn back(&mut self, cvar: &dyn Fn(&str) -> String) -> Outcome {
        match self.page {
            Page::Address => {
                self.open(Page::Together, cvar);
                return Outcome::None;
            }
            Page::Waiting => return Outcome::CancelLobby,
            _ => {}
        }
        if self.page != Page::Games {
            let from = self.page;
            self.open(Page::Games, cvar);
            // Land on the row we came from.
            let rows = self.rows(cvar);
            self.cursor = rows
                .iter()
                .position(|r| match (&r.action, from) {
                    (Action::PickGame(i), Page::Mods) => *i == self.game,
                    (Action::OpenSettings, Page::Settings) => true,
                    (Action::OpenTogether, Page::Together) => true,
                    _ => false,
                })
                .unwrap_or(self.cursor);
        }
        Outcome::None
    }

    /// Keyboard / controller navigation.
    pub(crate) fn nav(&mut self, nav: Nav, cvar: &dyn Fn(&str) -> String) -> Outcome {
        let rows = self.rows(cvar);
        match nav {
            Nav::Up | Nav::Down => {
                let step: isize = if nav == Nav::Up { -1 } else { 1 };
                let mut i = self.cursor as isize;
                loop {
                    i += step;
                    if i < 0 || i as usize >= rows.len() {
                        break;
                    }
                    if Self::selectable(&rows[i as usize]) {
                        self.cursor = i as usize;
                        break;
                    }
                }
                Outcome::None
            }
            Nav::Enter => self.activate(self.cursor, cvar),
            Nav::Back => self.back(cvar),
        }
    }

    /// The list: left edge, top, width, row height, visible rows. The menu
    /// is a column on the left; the game's picture shows on the right.
    fn geometry(&self, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32, usize) {
        let l = Layout::new(w, h, scale);
        (l.x, l.list_top, l.col_w, l.row_h, l.visible)
    }

    fn keep_cursor_visible(&mut self, visible: usize) {
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + visible {
            self.scroll = self.cursor + 1 - visible;
        }
    }

    /// A finger event (`started`, `moved`, `ended`): tap a row, drag to scroll.
    pub(crate) fn touch(
        &mut self,
        id: u64,
        phase: room_input::touch::TouchPhase,
        x: f32,
        y: f32,
        screen: (f32, f32, f32),
        cvar: &dyn Fn(&str) -> String,
    ) -> Outcome {
        use room_input::touch::TouchPhase as P;
        let (w, h, scale) = screen;
        let (lx, top, lw, row_h, visible) = self.geometry(w, h, scale);
        match phase {
            P::Started => {
                self.drag = Some((id, x, y, y, false));
                self.pressed = (x >= lx && x <= lx + lw && y >= top)
                    .then(|| self.scroll + ((y - top) / row_h) as usize);
                Outcome::None
            }
            P::Moved => {
                let rows = self.rows(cvar).len();
                if let Some((did, sx, sy, last, moved)) = self.drag.as_mut().filter(|d| d.0 == id) {
                    let _ = did;
                    if (x - *sx).hypot(y - *sy) > 14.0 * scale {
                        *moved = true;
                        self.pressed = None;
                    }
                    // Drag up to scroll down, a row at a time.
                    while *last - y >= row_h && self.scroll + visible < rows {
                        *last -= row_h;
                        self.scroll += 1;
                    }
                    while y - *last >= row_h && self.scroll > 0 {
                        *last += row_h;
                        self.scroll -= 1;
                    }
                }
                // Keep the highlight in view, or the next draw would scroll
                // back to it and undo the drag.
                let last_visible = (self.scroll + visible).min(rows).saturating_sub(1);
                self.cursor = self
                    .cursor
                    .clamp(self.scroll, last_visible.max(self.scroll));
                Outcome::None
            }
            P::Ended => {
                self.pressed = None;
                let Some((_, _, _, _, moved)) = self.drag.take().filter(|d| d.0 == id) else {
                    return Outcome::None;
                };
                if moved || x < lx || x > lx + lw || y < top {
                    return Outcome::None;
                }
                let index = self.scroll + ((y - top) / row_h) as usize;
                self.activate(index, cvar)
            }
            P::Cancelled => {
                self.drag = None;
                self.pressed = None;
                Outcome::None
            }
        }
    }

    fn text(&mut self, text: &str, face: Face, px: f32, color: [u8; 4]) -> TextImage {
        if self.text_cache.len() > 512 {
            self.text_cache.clear();
        }
        self.text_cache
            .entry((text.to_owned(), face, (px * 8.0).round() as u32, color))
            .or_insert_with(|| ui_text::render(text, face, px, color))
            .clone()
    }

    /// The backdrop of game `i` (made on first use).
    fn backdrop(&mut self, i: usize) -> Option<(PathBuf, crate::launcher_art::Backdrop)> {
        let game = self.library.games.get(i)?;
        let (path, engine) = (game.path.clone(), game.engine);
        let b = self
            .backdrops
            .entry(path.clone())
            .or_insert_with(|| crate::launcher_art::backdrop(&path, engine))
            .clone();
        Some((path, b))
    }

    /// Draw commands for a `w × h` framebuffer at `scale` px per dp.
    pub(crate) fn draw(
        &mut self,
        w: f32,
        h: f32,
        scale: f32,
        cvar: &dyn Fn(&str) -> String,
    ) -> Vec<Draw> {
        let now = std::time::Instant::now();
        let dt = self
            .motion
            .last
            .map_or(1.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
        self.motion.last = Some(now);
        let l = Layout::new(w, h, scale);
        self.keep_cursor_visible(l.visible);
        let rows = self.rows(cvar);
        let mut out = Vec::new();

        // The game whose picture and title show.
        if self.page == Page::Games {
            if let Some(Action::PickGame(i)) = rows.get(self.cursor).map(|r| &r.action) {
                self.motion.hero_game = *i;
            }
        } else {
            self.motion.hero_game = self.game;
        }
        let hero = self.motion.hero_game;
        let accent = self.backdrop_draws(hero, now, &mut out);
        let accent_f = accent.map(|c| f32::from(c) / 255.0);

        // Page slide-in.
        let opened = match self.motion.page {
            Some((p, t)) if p == self.page => t,
            _ => {
                let first = self.motion.page.is_none();
                self.motion.page = Some((self.page, now));
                self.motion.highlight = None;
                self.motion.scroll = None;
                if first {
                    now - std::time::Duration::from_secs(1)
                } else {
                    now
                }
            }
        };
        let t = (now.duration_since(opened).as_secs_f32() / 0.22).min(1.0);
        let slide = (1.0 - ease_out(t)) * 28.0 * scale;
        let x = l.x + slide;

        // Wordmark, title, line under it.
        let (title, mut meta) = self.heading(hero, &rows);
        if let Some(err) = &self.error {
            meta = (err.clone(), [255, 120, 100, 255]);
        } else if let Some(note) = &self.notice {
            meta = (
                note.clone(),
                [
                    accent[0].max(200),
                    accent[1].max(170),
                    accent[2].max(120),
                    255,
                ],
            );
        }
        let image = |out: &mut Vec<Draw>, me: &mut Self, s: &str, face, px, c, x: f32, y: f32| {
            let (iw, ih, rgba) = me.text(s, face, px, c);
            if iw > 1 {
                out.push(Draw::Image {
                    x: x.round().max(0.0),
                    y: y.round().max(0.0),
                    w: iw,
                    h: ih,
                    rgba,
                });
            }
            iw
        };
        image(
            &mut out,
            self,
            "Room",
            Face::Display,
            l.mark_px,
            MUTED,
            x,
            l.mark_y,
        );
        let title = ui_text::fit(&title, Face::Display, l.hero_px, l.col_w);
        image(
            &mut out,
            self,
            &title,
            Face::Display,
            l.hero_px,
            BONE,
            x,
            l.hero_y,
        );
        let meta_text = ui_text::fit(&meta.0, Face::Body, l.meta_px, l.col_w);
        image(
            &mut out,
            self,
            &meta_text,
            Face::Body,
            l.meta_px,
            meta.1,
            x,
            l.meta_y,
        );

        // Rows: the list scrolls and the highlight glides (springs).
        let follow = 1.0 - (-dt * 18.0).exp();
        let scroll_to = self.scroll as f32 * l.row_h;
        let scroll = spring(&mut self.motion.scroll, scroll_to, follow);
        let cursor_to = self.cursor as f32 * l.row_h;
        let highlight = spring(&mut self.motion.highlight, cursor_to, follow);
        let list_bottom = l.list_top + l.visible as f32 * l.row_h;
        let row_inset = 2.0 * scale;
        let in_view = |y: f32| y >= l.list_top - 1.0 && y + l.row_h <= list_bottom + 1.0;
        let hy = l.list_top + highlight - scroll;
        let cursor_selectable = rows.get(self.cursor).is_some_and(Self::selectable);
        if cursor_selectable && in_view(hy) {
            out.push(Draw::Rect {
                x,
                y: hy + row_inset,
                w: l.col_w,
                h: l.row_h - 2.0 * row_inset,
                color: [accent_f[0], accent_f[1], accent_f[2], 0.22],
            });
            out.push(Draw::Rect {
                x,
                y: hy + row_inset,
                w: (3.0 * scale).round().max(2.0),
                h: l.row_h - 2.0 * row_inset,
                color: [accent_f[0], accent_f[1], accent_f[2], 1.0],
            });
        }
        let pad = 14.0 * scale;
        for (i, row) in rows.iter().enumerate() {
            let y = l.list_top + i as f32 * l.row_h - scroll;
            if !in_view(y) {
                continue;
            }
            let selectable = Self::selectable(row);
            let current = i == self.cursor && selectable;
            if self.pressed == Some(i) && selectable {
                out.push(Draw::Rect {
                    x,
                    y: y + row_inset,
                    w: l.col_w,
                    h: l.row_h - 2.0 * row_inset,
                    color: [accent_f[0], accent_f[1], accent_f[2], 0.18],
                });
            }
            // Mods: a check box instead of "[X] ".
            let (label, check) = match row.label.strip_prefix("[X] ") {
                Some(rest) => (rest, Some(true)),
                None => match row.label.strip_prefix("[ ] ") {
                    Some(rest) => (rest, Some(false)),
                    None => (row.label.as_str(), None),
                },
            };
            let mut lx = x + pad;
            if let Some(on) = check {
                let size = (16.0 * scale).round();
                let by = (y + (l.row_h - size) / 2.0).round();
                check_box(&mut out, lx, by, size, scale, on, accent_f);
                lx += size + 10.0 * scale;
            }
            let (face, px, color) = if !selectable {
                (Face::Body, l.info_px, MUTED)
            } else if current {
                (Face::Strong, l.row_px, WHITE)
            } else {
                (Face::Body, l.row_px, BONE)
            };
            // On/off settings: a switch instead of "true" / "false".
            let switch = match &row.action {
                Action::Cycle(_, values) if *values == ["false", "true"] => {
                    Some(same_value("true", &row.detail))
                }
                _ => None,
            };
            let detail_w = if let Some(on) = switch {
                let sw = (34.0 * scale).round();
                let sh = (20.0 * scale).round();
                let sx = x + l.col_w - pad - sw;
                let sy = (y + (l.row_h - sh) / 2.0).round();
                switch_draw(&mut out, sx, sy, sw, sh, on, accent_f);
                sw
            } else if row.detail.is_empty() || !selectable {
                0.0
            } else {
                ui_text::width(&row.detail, Face::Body, l.detail_px).min(l.col_w * 0.45)
            };
            let label_room = (x + l.col_w
                - pad
                - lx
                - if detail_w > 0.0 {
                    detail_w + 12.0 * scale
                } else {
                    0.0
                })
            .max(0.0);
            let label = if !selectable && !row.detail.is_empty() {
                format!("{label}: {}", row.detail)
            } else {
                label.to_owned()
            };
            let label = ui_text::fit(&label, face, px, label_room);
            let ty = y + (l.row_h - ui_text::line_height(face, px)) / 2.0;
            image(&mut out, self, &label, face, px, color, lx, ty);
            if detail_w > 0.0 && switch.is_none() {
                // Scales read better as percentages ("1" → "100%").
                let shown_detail = match &row.action {
                    Action::Cycle(name, _)
                        if matches!(
                            *name,
                            "touch_size" | "touch_opacity" | "touch_look" | "r_scale"
                        ) =>
                    {
                        row.detail
                            .parse::<f32>()
                            .map_or_else(|_| row.detail.clone(), |v| format!("{:.0}%", v * 100.0))
                    }
                    _ => row.detail.clone(),
                };
                let detail = ui_text::fit(&shown_detail, Face::Body, l.detail_px, detail_w);
                let dw = ui_text::width(&detail, Face::Body, l.detail_px);
                let dy = y + (l.row_h - ui_text::line_height(Face::Body, l.detail_px)) / 2.0;
                let dc = if current { BONE } else { MUTED };
                image(
                    &mut out,
                    self,
                    &detail,
                    Face::Body,
                    l.detail_px,
                    dc,
                    x + l.col_w - pad - dw,
                    dy,
                );
            }
        }
        // Scroll bar when the list is longer than the screen.
        if rows.len() > l.visible {
            let track = l.visible as f32 * l.row_h;
            let bar_h = (track * l.visible as f32 / rows.len() as f32).max(24.0 * scale);
            let max_scroll = (rows.len() - l.visible) as f32 * l.row_h;
            let at = if max_scroll > 0.0 {
                (scroll / max_scroll).clamp(0.0, 1.0)
            } else {
                0.0
            };
            out.push(Draw::Rect {
                x: x + l.col_w + 6.0 * scale,
                y: l.list_top + at * (track - bar_h),
                w: (2.0 * scale).round().max(1.0),
                h: bar_h,
                color: [1.0, 1.0, 1.0, 0.28],
            });
        }
        out
    }

    /// Backdrop draws for game `hero` (cross-fading from the previous one);
    /// returns the accent colour (blended while fading).
    fn backdrop_draws(
        &mut self,
        hero: usize,
        now: std::time::Instant,
        out: &mut Vec<Draw>,
    ) -> [u8; 3] {
        const FADE: f32 = 0.35;
        let Some((path, current)) = self.backdrop(hero) else {
            return DEFAULT_ACCENT;
        };
        match &self.motion.backdrop {
            Some((shown, _, _)) if *shown == path => {}
            Some((shown, _, _)) => {
                let previous = Some(shown.clone());
                self.motion.backdrop = Some((path.clone(), previous, now));
            }
            None => {
                self.motion.backdrop =
                    Some((path.clone(), None, now - std::time::Duration::from_secs(1)));
            }
        }
        let (since, previous) = match &self.motion.backdrop {
            Some((_, prev, t)) => (*t, prev.clone()),
            None => (now, None),
        };
        let t = (now.duration_since(since).as_secs_f32() / FADE).min(1.0);
        let mut accent = current.accent;
        if t < 1.0
            && let Some(prev) = previous.and_then(|p| self.backdrops.get(&p).cloned())
        {
            out.push(Draw::Backdrop {
                key: prev.key.clone(),
                w: crate::launcher_art::WIDTH,
                h: crate::launcher_art::HEIGHT,
                rgba: prev.rgba.clone(),
                alpha: 1.0,
            });
            let e = ease_out(t);
            for c in 0..3 {
                accent[c] = (f32::from(prev.accent[c]) * (1.0 - e)
                    + f32::from(current.accent[c]) * e) as u8;
            }
            out.push(Draw::Backdrop {
                key: current.key.clone(),
                w: crate::launcher_art::WIDTH,
                h: crate::launcher_art::HEIGHT,
                rgba: current.rgba.clone(),
                alpha: e,
            });
        } else {
            out.push(Draw::Backdrop {
                key: current.key.clone(),
                w: crate::launcher_art::WIDTH,
                h: crate::launcher_art::HEIGHT,
                rgba: current.rgba.clone(),
                alpha: 1.0,
            });
        }
        accent
    }

    /// Title and the line under it for the page.
    fn heading(&self, hero: usize, rows: &[Row]) -> (String, (String, [u8; 4])) {
        let game = self.library.games.get(hero);
        let game_title = game.map_or_else(|| "Room".to_owned(), |g| g.title.clone());
        match self.page {
            Page::Games => {
                let detail = rows
                    .iter()
                    .find(|r| r.action == Action::PickGame(hero))
                    .map(|r| r.detail.clone())
                    .unwrap_or_default();
                let line = if self.library.games.is_empty() {
                    "Add a game to start".to_owned()
                } else if detail.is_empty() {
                    "Tap to play".to_owned()
                } else {
                    detail
                };
                (game_title, (line, MUTED))
            }
            Page::Mods => (game_title, ("Choose mods, then play".into(), MUTED)),
            Page::Settings => (
                "Settings".into(),
                ("Touch controls, picture and speed".into(), MUTED),
            ),
            Page::Together => ("Play together".into(), (self.selection_text(), MUTED)),
            Page::Address => (
                "Join by address".into(),
                ("Type the host's address".into(), MUTED),
            ),
            Page::Waiting => ("Lobby".into(), (self.selection_text(), MUTED)),
        }
    }

    /// Index of the cursor row and the page, for tests.
    #[cfg(test)]
    fn at(&self) -> (Page, usize) {
        (self.page, self.cursor)
    }
}

/// A setting's value as the settings page shows it.
fn shown(name: &str, value: &str) -> String {
    match (name, value.trim()) {
        ("vid_maxfps", "0") => "no limit".into(),
        ("vid_maxfps", fps) => format!("{fps} fps"),
        _ => value.to_owned(),
    }
}

/// Whether a CVar's current text matches a choice (`1` == `1.0`, `true` == `1`).
fn same_value(choice: &str, now: &str) -> bool {
    let norm = |s: &str| match s.trim() {
        "true" => "1".to_owned(),
        "false" => "0".to_owned(),
        other => other
            .parse::<f64>()
            .map_or_else(|_| other.to_owned(), |v| format!("{v}")),
    };
    norm(choice) == norm(now)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use room_input::touch::TouchPhase;

    fn entry(file: &str, title: &str, engine: Engine, bundled: bool) -> Entry {
        Entry {
            path: PathBuf::from(format!("/lib/{file}")),
            file: file.into(),
            title: title.into(),
            size: 4096,
            bundled,
            engine,
            companions: Vec::new(),
        }
    }

    fn library() -> Library {
        Library {
            games: vec![
                entry("freedoom2.wad", "Freedoom: Phase 2", Engine::Doom, true),
                entry("DUKE3D.GRP", "Duke Nukem 3D", Engine::Duke, false),
                entry("freedoom1.wad", "Freedoom: Phase 1", Engine::Doom, true),
            ],
            mods: vec![
                entry("brutal.pk3", "brutal.pk3", Engine::Doom, false),
                entry("map.wad", "map.wad", Engine::Doom, false),
            ],
            problems: vec![("broken.wad".into(), "truncated".into())],
        }
    }

    fn cvars() -> impl Fn(&str) -> String {
        |name: &str| match name {
            "touch_size" => "1".into(),
            "touch_lefty" => "false".into(),
            _ => "1".into(),
        }
    }

    #[test]
    fn a_game_without_mods_starts_at_once() {
        let mut l = LauncherScreen::new(library(), None);
        let c = cvars();
        l.nav(Nav::Down, &c);
        assert_eq!(l.at(), (Page::Games, 1));
        let Outcome::Start(cfg) = l.nav(Nav::Enter, &c) else {
            panic!("duke starts");
        };
        assert!(cfg.native_duke && cfg.grp.is_empty());
        assert_eq!(cfg.iwad, Some(PathBuf::from("/lib/DUKE3D.GRP")));
    }

    #[test]
    fn mods_are_ticked_then_played_in_library_order() {
        let mut l = LauncherScreen::new(library(), None);
        let c = cvars();
        assert_eq!(
            l.nav(Nav::Enter, &c),
            Outcome::None,
            "Freedoom 2 has mods: mods page"
        );
        assert_eq!(l.at(), (Page::Mods, 0));
        // Tick map.wad then brutal.pk3; the order played is the library's.
        l.nav(Nav::Down, &c);
        l.nav(Nav::Down, &c);
        l.nav(Nav::Enter, &c);
        l.nav(Nav::Up, &c);
        l.nav(Nav::Enter, &c);
        l.nav(Nav::Up, &c);
        let Outcome::Start(cfg) = l.nav(Nav::Enter, &c) else {
            panic!("play");
        };
        assert_eq!(cfg.iwad, Some(PathBuf::from("/lib/freedoom2.wad")));
        assert_eq!(
            cfg.file,
            [
                PathBuf::from("/lib/brutal.pk3"),
                PathBuf::from("/lib/map.wad")
            ]
        );
        assert!(cfg.play && !cfg.native_duke);
        // Untick brutal: only map.wad.
        l.nav(Nav::Down, &c);
        l.nav(Nav::Enter, &c);
        l.nav(Nav::Up, &c);
        let Outcome::Start(cfg) = l.nav(Nav::Enter, &c) else {
            panic!("play");
        };
        assert_eq!(cfg.file, [PathBuf::from("/lib/map.wad")]);
        // Back returns to the game we came from.
        l.nav(Nav::Back, &c);
        assert_eq!(l.at(), (Page::Games, 0));
    }

    #[test]
    fn shipped_mods_start_ticked_with_their_companions() {
        let mut lib = library();
        for m in &mut lib.mods {
            if m.file == "brutal.pk3" {
                m.bundled = true;
                m.title = "Brutal Doom".into();
                m.companions = vec![PathBuf::from("/lib/brutal-zscript.wad")];
            }
        }
        let mut l = LauncherScreen::new(lib, None);
        let c = cvars();
        // Freedoom 2 → mods page, Brutal Doom already ticked; Play starts it.
        assert_eq!(l.nav(Nav::Enter, &c), Outcome::None);
        assert_eq!(l.at(), (Page::Mods, 0));
        let Outcome::Start(cfg) = l.nav(Nav::Enter, &c) else {
            panic!("play");
        };
        assert_eq!(
            cfg.file,
            [
                PathBuf::from("/lib/brutal.pk3"),
                PathBuf::from("/lib/brutal-zscript.wad")
            ]
        );
        assert_eq!(
            l.notice.as_deref(),
            Some("Loading Freedoom: Phase 2 + Brutal Doom …")
        );
        // Back on the games page, Duke (no mods) starts at once and says so.
        l.nav(Nav::Back, &c);
        assert_eq!(l.at(), (Page::Games, 0));
        l.nav(Nav::Down, &c);
        let Outcome::Start(cfg) = l.nav(Nav::Enter, &c) else {
            panic!("duke");
        };
        assert!(cfg.native_duke);
        assert_eq!(l.notice.as_deref(), Some("Loading Duke Nukem 3D …"));
    }

    /// Activate the first row whose label starts with `label`.
    fn tap(l: &mut LauncherScreen, label: &str) -> Outcome {
        let c = cvars();
        let rows = l.rows(&c);
        let i = rows
            .iter()
            .position(|r| r.label.starts_with(label))
            .unwrap_or_else(|| {
                panic!(
                    "no row {label:?} in {:?}",
                    rows.iter().map(|r| &r.label).collect::<Vec<_>>()
                )
            });
        l.activate(i, &c)
    }

    #[test]
    fn play_together_hosts_or_joins_the_current_pick() {
        let mut l = LauncherScreen::new(library(), None);
        assert_eq!(tap(&mut l, "Play together"), Outcome::None);
        assert_eq!(l.at().0, Page::Together);
        let rows: Vec<String> = l.rows(&cvars()).into_iter().map(|r| r.label).collect();
        assert_eq!(rows[0], "Freedoom: Phase 2", "what will be played");
        // Host: the game as Play would start it, plus the lobby.
        let Outcome::Start(cfg) = tap(&mut l, "Host: 3 players") else {
            panic!("host");
        };
        assert_eq!(cfg.host.as_deref(), Some("5029"));
        assert_eq!(cfg.players, 3);
        assert_eq!(cfg.iwad, Some(PathBuf::from("/lib/freedoom2.wad")));
        assert!(cfg.play && cfg.join.is_none());
        // Change game cycles through the library.
        tap(&mut l, "Change game");
        assert_eq!(l.rows(&cvars())[0].label, "Duke Nukem 3D");
        tap(&mut l, "Change game");
        tap(&mut l, "Change game");
        // Join by address: the keypad builds it; port 5029 unless typed.
        tap(&mut l, "Join by address");
        assert_eq!(l.at().0, Page::Address);
        for key in ["1", "0", ".", "0", ".", "0", ".", "7", "7"] {
            tap(&mut l, key);
        }
        tap(&mut l, "Delete");
        let Outcome::Start(cfg) = tap(&mut l, "Join 10.0.0.7") else {
            panic!("join");
        };
        assert_eq!(cfg.join.as_deref(), Some("10.0.0.7:5029"));
        assert!(cfg.host.is_none());
        // Back from the keypad goes to the co-op page, then to the games.
        assert_eq!(l.nav(Nav::Back, &cvars()), Outcome::None);
        assert_eq!(l.at().0, Page::Together);
        l.nav(Nav::Back, &cvars());
        assert_eq!(l.at().0, Page::Games);
    }

    #[test]
    fn games_found_on_the_network_say_whether_they_match() {
        let mut l = LauncherScreen::new(library(), None);
        tap(&mut l, "Play together");
        assert_eq!(tap(&mut l, "Find games"), Outcome::FindLan);
        assert!(l.lan_searching);
        let mine = crate::netplay::config_fingerprint(&l.start_config().unwrap());
        let game = |ip: &str, fingerprint| crate::lan::LanGame {
            addr: format!("{ip}:5029").parse().unwrap(),
            players: 1,
            max_players: 2,
            fingerprint,
            map: "MAP01".into(),
        };
        l.lan_searching = false;
        l.lan_games = Some(vec![
            game("192.168.1.5", mine),
            game("192.168.1.9", mine ^ 1),
        ]);
        let rows = l.rows(&cvars());
        let found: Vec<(&str, &str)> = rows
            .iter()
            .filter(|r| r.label.starts_with("Join 192"))
            .map(|r| (r.label.as_str(), r.detail.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("Join 192.168.1.5 MAP01 (1/2)", "same game"),
                ("Join 192.168.1.9 MAP01 (1/2)", "other game or mods"),
            ]
        );
        let Outcome::Start(cfg) = tap(&mut l, "Join 192.168.1.5") else {
            panic!("join");
        };
        assert_eq!(cfg.join.as_deref(), Some("192.168.1.5:5029"));
    }

    #[test]
    fn the_waiting_page_cancels_and_failures_show() {
        let mut l = LauncherScreen::new(library(), None);
        l.show_waiting("Waiting for players at 10.0.0.2:5029 (1/2)".into());
        assert_eq!(l.at(), (Page::Waiting, 1), "cursor on Cancel");
        assert_eq!(l.nav(Nav::Enter, &cvars()), Outcome::CancelLobby);
        assert_eq!(l.nav(Nav::Back, &cvars()), Outcome::CancelLobby);
        l.lobby_ended(Some("join failed: host rejected: fingerprint".into()));
        assert_eq!(l.at().0, Page::Together);
        assert!(l.error.as_deref().is_some_and(|e| e.contains("rejected")));
        assert!(l.waiting.is_none());
    }

    #[test]
    fn games_started_here_come_back_and_the_speed_test_measures() {
        let mut l = LauncherScreen::new(library(), None);
        assert_eq!(tap(&mut l, "Freedoom: Phase 2"), Outcome::None, "mods page");
        let Outcome::Start(cfg) = tap(&mut l, "Play Freedoom") else {
            panic!("start");
        };
        assert!(cfg.from_launcher, "quitting goes back to the launcher");
        assert!(cfg.bench_frames.is_none());
        l.nav(Nav::Back, &cvars());
        tap(&mut l, "Settings");
        let Outcome::Start(cfg) = tap(&mut l, "Speed test") else {
            panic!("speed test");
        };
        assert_eq!(cfg.bench_frames, Some(SPEED_TEST_FRAMES));
        assert!(cfg.from_launcher && cfg.play);
        assert!(
            l.notice
                .as_deref()
                .is_some_and(|n| n.starts_with("Speed test: Loading"))
        );
    }

    #[test]
    fn cursor_skips_information_rows() {
        let mut l = LauncherScreen::new(library(), Some("copy files to /sdcard/x".into()));
        let c = cvars();
        for _ in 0..10 {
            l.nav(Nav::Down, &c);
        }
        let rows = l.rows(&c);
        assert_eq!(rows[l.cursor].label, "Settings", "the last selectable row");
        assert!(
            rows.iter()
                .any(|r| r.label == "Skipped broken.wad" && r.detail == "truncated")
        );
        assert!(rows.iter().any(|r| r.label.contains("/sdcard/x")));
    }

    #[test]
    fn settings_cycle_their_cvars() {
        let mut l = LauncherScreen::new(library(), None);
        let c = cvars();
        // Three games, "Play together", then Settings.
        for _ in 0..4 {
            l.nav(Nav::Down, &c);
        }
        l.nav(Nav::Enter, &c);
        assert_eq!(l.at(), (Page::Settings, 0));
        assert_eq!(
            l.nav(Nav::Enter, &c),
            Outcome::SetCvar("touch_size".into(), "1.2".into()),
            "1 → next value"
        );
        for _ in 0..4 {
            l.nav(Nav::Down, &c);
        }
        assert_eq!(
            l.nav(Nav::Enter, &c),
            Outcome::SetCvar("touch_lefty".into(), "true".into())
        );
        l.nav(Nav::Back, &c);
        assert_eq!(l.at(), (Page::Games, 4), "back on the Settings row");
    }

    #[test]
    fn tapping_a_row_activates_it_and_drags_scroll() {
        let mut l = LauncherScreen::new(library(), None);
        let c = cvars();
        let screen = (2400.0, 1080.0, 3.0);
        let (lx, top, _, row_h, _) = l.geometry(2400.0, 1080.0, 3.0);
        let y = top + row_h * 1.5; // second row: Duke
        // The list is a column on the left.
        let x = lx + 100.0;
        l.touch(1, TouchPhase::Started, x, y, screen, &c);
        let out = l.touch(1, TouchPhase::Ended, x + 2.0, y, screen, &c);
        assert!(matches!(out, Outcome::Start(cfg) if cfg.native_duke));
        // A drag is not a tap.
        l.touch(2, TouchPhase::Started, x, y, screen, &c);
        l.touch(2, TouchPhase::Moved, x, y - 200.0, screen, &c);
        assert_eq!(
            l.touch(2, TouchPhase::Ended, x, y - 200.0, screen, &c),
            Outcome::None
        );
        // Taps on the picture beside the list do nothing.
        l.touch(4, TouchPhase::Started, 2300.0, y, screen, &c);
        assert_eq!(
            l.touch(4, TouchPhase::Ended, 2300.0, y, screen, &c),
            Outcome::None
        );
        // Taps outside the list do nothing.
        l.touch(3, TouchPhase::Started, 5.0, y, screen, &c);
        assert_eq!(
            l.touch(3, TouchPhase::Ended, 5.0, y, screen, &c),
            Outcome::None
        );
    }

    #[test]
    fn a_drag_scroll_survives_the_next_draw() {
        let mut lib = library();
        for i in 0..40 {
            lib.games.push(entry(
                &format!("g{i}.wad"),
                &format!("Game {i}"),
                Engine::Doom,
                false,
            ));
        }
        let mut l = LauncherScreen::new(lib, None);
        let c = cvars();
        let screen = (1280.0, 720.0, 1.0);
        let (lx, top, _, row_h, visible) = l.geometry(1280.0, 720.0, 1.0);
        let x = lx + 50.0;
        let start = top + row_h * (visible as f32 - 0.5);
        l.touch(1, TouchPhase::Started, x, start, screen, &c);
        for step in 1..=10 {
            l.touch(
                1,
                TouchPhase::Moved,
                x,
                start - row_h * step as f32 * 0.6,
                screen,
                &c,
            );
        }
        l.touch(1, TouchPhase::Ended, x, start - row_h * 6.0, screen, &c);
        let scrolled = l.scroll;
        assert!(scrolled >= 5, "six rows dragged: {scrolled}");
        l.draw(1280.0, 720.0, 1.0, &c);
        assert_eq!(l.scroll, scrolled, "the draw keeps the drag");
        // A tap now picks the game under the finger (the first one in view).
        let under_finger = l.rows(&c)[scrolled].label.clone();
        let y = top + row_h * 0.5;
        l.touch(2, TouchPhase::Started, x, y, screen, &c);
        l.touch(2, TouchPhase::Ended, x, y, screen, &c);
        assert_eq!(
            l.selected_game().map(|g| g.title.clone()),
            Some(under_finger)
        );
    }

    #[test]
    fn long_lists_scroll_to_keep_the_cursor_visible() {
        let mut lib = library();
        for i in 0..40 {
            lib.games.push(entry(
                &format!("g{i}.wad"),
                &format!("Game {i}"),
                Engine::Doom,
                false,
            ));
        }
        let mut l = LauncherScreen::new(lib, None);
        let c = cvars();
        for _ in 0..30 {
            l.nav(Nav::Down, &c);
        }
        let draws = l.draw(1280.0, 720.0, 1.0, &c);
        let (_, _, _, _, visible) = l.geometry(1280.0, 720.0, 1.0);
        assert!(l.scroll > 0 && l.cursor < l.scroll + visible && l.cursor >= l.scroll);
        assert!(draws.len() > 2);
    }

    #[test]
    fn drawing_stays_on_screen_and_caches_text() {
        let mut l = LauncherScreen::new(library(), Some("copy files here".into()));
        l.error = Some("I/O error: No such file or directory (os error 2)".into());
        let c = cvars();
        for (w, h, s) in [
            (2400.0, 1080.0, 3.0),
            (1280.0, 720.0, 1.0),
            (2796.0, 1290.0, 3.0),
        ] {
            for d in l.draw(w, h, s, &c) {
                let (x, y, dw, dh) = match d {
                    Draw::Rect { x, y, w, h, .. } => (x, y, w, h),
                    Draw::Image { x, y, w, h, .. } => (x, y, w as f32, h as f32),
                    Draw::Backdrop { .. } => continue,
                };
                assert!(x >= 0.0 && y >= 0.0 && x + dw <= w + 0.5 && y + dh <= h + 0.5);
            }
        }
        let n = l.text_cache.len();
        l.draw(2400.0, 1080.0, 3.0, &c);
        assert_eq!(l.text_cache.len(), n);
    }

    #[test]
    fn the_game_picture_shows_and_the_highlight_glides_to_the_cursor() {
        let mut l = LauncherScreen::new(library(), None);
        let c = cvars();
        let draws = l.draw(2400.0, 1080.0, 3.0, &c);
        assert!(
            draws
                .iter()
                .any(|d| matches!(d, Draw::Backdrop { alpha, .. } if *alpha >= 1.0))
        );
        let (_, top, _, row_h, _) = l.geometry(2400.0, 1080.0, 3.0);
        l.nav(Nav::Down, &c);
        // Right after the move the highlight is between the rows…
        std::thread::sleep(std::time::Duration::from_millis(16));
        l.draw(2400.0, 1080.0, 3.0, &c);
        let h = l.motion.highlight.expect("highlight");
        assert!(h > 0.0 && h < row_h, "{h}");
        // …and settles on the cursor.
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            l.draw(2400.0, 1080.0, 3.0, &c);
        }
        assert_eq!(l.motion.highlight, Some(row_h));
        let _ = top;
    }

    #[test]
    fn setting_cvars_match_the_settings_page() {
        let names: Vec<&str> = SETTINGS.iter().map(|(n, _, _)| *n).collect();
        assert_eq!(names, SETTING_CVARS);
    }

    #[test]
    fn values_compare_loosely() {
        assert!(same_value("1", "1.0"));
        assert!(same_value("true", "1"));
        assert!(same_value("false", "false"));
        assert_eq!(shown("vid_maxfps", "0"), "no limit");
        assert_eq!(shown("vid_maxfps", "60"), "60 fps");
        assert_eq!(shown("r_scale", "0.75"), "0.75");
        assert!(!same_value("0.8", "1"));
    }
}

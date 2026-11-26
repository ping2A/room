//! On-screen touch controls for phones (docs/41 M3).
//!
//! A port of the web client's `touch.js` to plain Rust so the native app
//! (Android / iOS) behaves the same:
//!
//! - **Move side** (left 45 %, right for left-handed play): a floating analog
//!   stick that appears under the thumb, follows it past its rim, and moves at
//!   any angle and speed (a full push runs).
//! - **Look side**: drag to look, faster flicks turn further; double-tap and
//!   hold fires while aiming.
//! - **Buttons** (bottom corner cluster): FIRE (drag on it to aim), USE, JUMP,
//!   DUCK, previous / next weapon (hold for a weapon picker); Duke adds KICK
//!   and ITEM. Top right: automap and game menu.
//!
//! Everything here is pure: the shell feeds [`TouchEvent`]s and a clock,
//! and reads a [`TouchFrame`] each frame (held [`Key`]s, an analog move
//! vector, look counts and one-shot actions). Geometry is in physical
//! pixels; sizes are given in density-independent points (dp) and scaled by
//! [`Screen::scale`], as CSS pixels are on the web.

use crate::Key;
use std::collections::{BTreeMap, BTreeSet};

/// Stick radius (dp).
pub const STICK_RADIUS_DP: f32 = 56.0;
/// Below this push the analog stick reads zero.
pub const ANALOG_DEAD: f32 = 0.12;
/// Push needed before a direction key is pressed (netplay / Duke keys).
pub const KEY_DEAD: f32 = 0.35;
/// Push that counts as running.
pub const RUN_AT: f32 = 0.85;
/// Mouse counts per dp of drag at look speed 1 (a count is ≈1.4° of turn).
pub const LOOK_COUNTS_PER_DP: f32 = 0.3;
/// Hold a weapon arrow this long to open the weapon picker.
pub const LONG_PRESS_MS: f64 = 420.0;
/// Two taps closer than this (and 60 dp) make a double tap.
pub const DOUBLE_TAP_MS: f64 = 300.0;
/// The weapon picker closes by itself after this long.
pub const SLOTS_OPEN_MS: f64 = 4000.0;
/// Vertical drag that moves the menu cursor one item (dp).
pub const MENU_STEP_DP: f32 = 44.0;
/// Screens at most this tall (dp) use the compact cluster.
pub const SHORT_SCREEN_DP: f32 = 380.0;

/// Player-tunable options (`touch_*` CVars).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchSettings {
    /// Drag-look speed.
    pub look: f32,
    /// Up/down look speed relative to left/right.
    pub look_y: f32,
    /// Fast swipes turn further.
    pub accel: bool,
    /// Double-tap and hold the look side to fire.
    pub tap_fire: bool,
    /// Invert up/down look.
    pub invert_y: bool,
    /// Button scale.
    pub size: f32,
    /// Control opacity, `0.1..=1`.
    pub opacity: f32,
    /// Stick on the right, buttons on the left.
    pub lefty: bool,
    /// Vibrate on presses.
    pub haptics: bool,
    /// Show JUMP / DUCK in Doom-format games (Duke always shows them).
    pub jump_duck: bool,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            look: 1.0,
            // Landscape screens are short, so a full-height swipe should still
            // tilt the view most of its range.
            look_y: 1.6,
            accel: true,
            tap_fire: true,
            invert_y: false,
            size: 1.0,
            opacity: 1.0,
            lefty: false,
            haptics: true,
            jump_duck: true,
        }
    }
}

/// The drawable area in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// Width (px).
    pub width: f32,
    /// Height (px).
    pub height: f32,
    /// Physical pixels per dp (the OS display scale).
    pub scale: f32,
    /// Safe-area insets (px): notch, rounded corners, gesture bar.
    pub inset_top: f32,
    /// Right inset (px).
    pub inset_right: f32,
    /// Bottom inset (px).
    pub inset_bottom: f32,
    /// Left inset (px).
    pub inset_left: f32,
}

impl Screen {
    /// A screen without insets.
    #[must_use]
    pub fn new(width: f32, height: f32, scale: f32) -> Self {
        Self {
            width,
            height,
            scale: scale.max(0.1),
            inset_top: 0.0,
            inset_right: 0.0,
            inset_bottom: 0.0,
            inset_left: 0.0,
        }
    }

    fn height_dp(&self) -> f32 {
        self.height / self.scale
    }
}

/// Axis-aligned rectangle (px), y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether the two overlap (touching edges do not count).
    #[must_use]
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    /// Whether `self` lies fully inside `o`.
    #[must_use]
    pub fn inside(&self, o: &Rect) -> bool {
        self.x >= o.x && self.y >= o.y && self.right() <= o.right() && self.bottom() <= o.bottom()
    }
}

/// A tappable control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TouchButton {
    /// Attack (drag on it to aim).
    Fire,
    /// Use / open.
    Use,
    /// Jump.
    Jump,
    /// Crouch.
    Duck,
    /// Duke quick kick.
    Kick,
    /// Duke: use the selected inventory item.
    Item,
    /// Duke: select the previous inventory item (the jetpack is one).
    ItemPrev,
    /// Duke: select the next inventory item.
    ItemNext,
    /// Next weapon (hold for the picker).
    WeaponNext,
    /// Previous weapon (hold for the picker).
    WeaponPrev,
    /// Toggle the automap.
    Map,
    /// The round minimap radar (top-left); a tap opens the full automap.
    /// The app draws it; the overlay draws nothing for it.
    Radar,
    /// Zoom the automap in (shown while it is open).
    MapZoomIn,
    /// Zoom the automap out (shown while it is open).
    MapZoomOut,
    /// Open / close the game menu.
    Menu,
    /// Weapon picker slot `0..=9`.
    Slot(u8),
}

impl TouchButton {
    /// Text drawn on the button.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Fire => "FIRE".into(),
            Self::Use => "USE".into(),
            Self::Jump => "JUMP".into(),
            Self::Duck => "DUCK".into(),
            Self::Kick => "KICK".into(),
            Self::Item => "ITEM".into(),
            Self::ItemPrev => "<IT".into(),
            Self::ItemNext => "IT>".into(),
            Self::WeaponNext => ">".into(),
            Self::WeaponPrev => "<".into(),
            Self::Map => "MAP".into(),
            Self::Radar => String::new(),
            Self::MapZoomIn => "+".into(),
            Self::MapZoomOut => "-".into(),
            Self::Menu => "=".into(),
            Self::Slot(n) => n.to_string(),
        }
    }

    /// Game action held while the button is down.
    #[must_use]
    pub fn held_key(self) -> Option<Key> {
        match self {
            Self::Fire => Some(Key::Attack),
            Self::Use => Some(Key::Use),
            Self::Jump => Some(Key::Jump),
            Self::Duck => Some(Key::MoveDown),
            Self::Kick => Some(Key::QuickKick),
            Self::Item => Some(Key::InvUse),
            Self::ItemPrev => Some(Key::InvPrev),
            Self::ItemNext => Some(Key::InvNext),
            _ => None,
        }
    }
}

/// Button shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Round action button.
    Circle,
    /// Rounded square (top bar, weapon picker).
    Square,
}

/// One placed control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Which control.
    pub button: TouchButton,
    /// Centre X (px).
    pub cx: f32,
    /// Centre Y (px).
    pub cy: f32,
    /// Half the side / the radius (px).
    pub r: f32,
    /// Outline.
    pub shape: Shape,
}

impl Placed {
    /// Bounding box.
    #[must_use]
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.cx - self.r,
            y: self.cy - self.r,
            w: self.r * 2.0,
            h: self.r * 2.0,
        }
    }

    /// Whether `(x, y)` is on the control.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match self.shape {
            Shape::Circle => (x - self.cx).hypot(y - self.cy) <= self.r,
            Shape::Square => (x - self.cx).abs() <= self.r && (y - self.cy).abs() <= self.r,
        }
    }
}

/// The radar's diameter (dp): `clamp(76px, 24vmin, 116px)`, as on the web.
#[must_use]
pub fn radar_diameter_dp(screen: &Screen) -> f32 {
    let vmin = screen.width.min(screen.height) / screen.scale.max(0.1);
    (vmin * 0.24).clamp(76.0, 116.0)
}

/// The bottom cluster as in `touch.js`: `(button, size, distance from the
/// side edge, distance from the bottom)`, all dp, measured to the box edge.
const CLUSTER: [(TouchButton, f32, f32, f32); 8] = [
    (TouchButton::Fire, 92.0, 24.0, 28.0),
    (TouchButton::Use, 64.0, 128.0, 22.0),
    (TouchButton::WeaponNext, 48.0, 36.0, 136.0),
    (TouchButton::WeaponPrev, 48.0, 100.0, 112.0),
    (TouchButton::Jump, 58.0, 204.0, 86.0),
    (TouchButton::Duck, 52.0, 206.0, 18.0),
    (TouchButton::Kick, 48.0, 166.0, 158.0),
    (TouchButton::Item, 48.0, 230.0, 156.0),
];

/// Where every control sits for one screen and set of options.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchLayout {
    /// Visible controls, drawing order.
    pub buttons: Vec<Placed>,
    /// Resting place of the stick (centre, px).
    pub idle_stick: (f32, f32),
    /// Stick radius (px).
    pub stick_radius: f32,
    /// Stick knob radius (px).
    pub knob_radius: f32,
    /// X where the move side ends (or starts, left-handed).
    pub split_x: f32,
    /// Left-handed layout.
    pub lefty: bool,
    /// Physical pixels per dp.
    pub scale: f32,
    /// Opacity, `0.1..=1`.
    pub opacity: f32,
}

/// What the layout should keep clear of and show.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutOptions {
    /// Duke Nukem 3D (KICK and ITEM, ten weapon slots).
    pub duke: bool,
    /// Weapon picker open.
    pub slots_open: bool,
    /// Automap open: zoom buttons beside MAP.
    pub map_open: bool,
    /// Show the minimap radar in the top corner opposite the top bar.
    pub radar: bool,
    /// Screen area controls must not cover (the status bar). The bottom
    /// cluster and the resting stick are lifted above it where they would.
    pub avoid: Option<Rect>,
}

impl TouchLayout {
    /// Place the controls.
    #[must_use]
    pub fn new(screen: &Screen, s: &TouchSettings, opts: &LayoutOptions) -> Self {
        let dp = screen.scale;
        let k = dp * s.size.clamp(0.5, 2.0);
        let short = screen.height_dp() <= SHORT_SCREEN_DP;
        let show_jump_duck = opts.duke || s.jump_duck;
        let mut cluster: Vec<Placed> = Vec::new();
        for (button, size, side, bottom) in CLUSTER {
            match button {
                TouchButton::Jump | TouchButton::Duck if !show_jump_duck => continue,
                TouchButton::Kick | TouchButton::Item if !opts.duke => continue,
                _ => {}
            }
            // Short landscape phones: tighten the cluster so it clears the stick.
            let (size, bottom) = match button {
                TouchButton::Fire if short => (78.0, bottom),
                TouchButton::Jump if short => (size, 76.0),
                TouchButton::Kick | TouchButton::Item if short => (size, 138.0),
                _ => (size, bottom),
            };
            let half = size * k / 2.0;
            let edge = side * k + half;
            let cx = if s.lefty {
                screen.inset_left + edge
            } else {
                screen.width - screen.inset_right - edge
            };
            let cy = screen.height - screen.inset_bottom - bottom * k - half;
            cluster.push(Placed {
                button,
                cx,
                cy,
                r: half,
                shape: Shape::Circle,
            });
        }
        let stick_radius = STICK_RADIUS_DP * dp;
        let idle_offset = (STICK_RADIUS_DP + 36.0) * dp;
        let mut idle_stick = (
            if s.lefty {
                screen.width - screen.inset_right - idle_offset
            } else {
                screen.inset_left + idle_offset
            },
            screen.height - screen.inset_bottom - idle_offset,
        );
        if let Some(avoid) = opts.avoid {
            // Lift the whole cluster (keeping its shape) until no button
            // overlaps the area, plus a small gap.
            let gap = 6.0 * dp;
            let lift = cluster
                .iter()
                .filter(|b| b.rect().intersects(&avoid))
                .map(|b| b.rect().bottom() - avoid.y + gap)
                .fold(0.0_f32, f32::max);
            for b in &mut cluster {
                b.cy -= lift;
            }
            let stick = Rect {
                x: idle_stick.0 - stick_radius,
                y: idle_stick.1 - stick_radius,
                w: stick_radius * 2.0,
                h: stick_radius * 2.0,
            };
            if stick.intersects(&avoid) {
                idle_stick.1 -= stick.bottom() - avoid.y + gap;
            }
        }
        let mut buttons = cluster;
        // Top bar: game menu in the corner, automap beside it (40 dp squares,
        // not scaled by the button size).
        let top_half = 20.0 * dp;
        let top_y = screen.inset_top + 8.0 * dp + top_half;
        let mut top_bar = vec![(TouchButton::Menu, 8.0), (TouchButton::Map, 56.0)];
        if opts.duke {
            // Duke's inventory bar: pick an item (jetpack, medkit, …), then ITEM.
            top_bar.extend([
                (TouchButton::ItemNext, 104.0),
                (TouchButton::ItemPrev, 152.0),
            ]);
        }
        if opts.map_open {
            let at = top_bar.last().map_or(56.0, |&(_, edge)| edge);
            top_bar.extend([
                (TouchButton::MapZoomIn, at + 48.0),
                (TouchButton::MapZoomOut, at + 96.0),
            ]);
        }
        for (button, from_edge) in top_bar {
            let edge = from_edge * dp + top_half;
            let cx = if s.lefty {
                screen.inset_left + edge
            } else {
                screen.width - screen.inset_right - edge
            };
            buttons.push(Placed {
                button,
                cx,
                cy: top_y,
                r: top_half,
                shape: Shape::Square,
            });
        }
        if opts.radar {
            let d = radar_diameter_dp(screen) * dp;
            let edge = 8.0 * dp + d / 2.0;
            let cx = if s.lefty {
                screen.width - screen.inset_right - edge
            } else {
                screen.inset_left + edge
            };
            buttons.push(Placed {
                button: TouchButton::Radar,
                cx,
                cy: screen.inset_top + edge,
                r: d / 2.0,
                shape: Shape::Circle,
            });
        }
        if opts.slots_open {
            let slots: Vec<u8> = if opts.duke {
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0]
            } else {
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
            };
            let side = 46.0 * dp;
            let gap = 8.0 * dp;
            let total = slots.len() as f32 * side + (slots.len() - 1) as f32 * gap;
            let x0 = screen.width / 2.0 - total / 2.0;
            let cy = screen.inset_top + 60.0 * dp + 10.0 * dp + side / 2.0;
            for (i, n) in slots.into_iter().enumerate() {
                buttons.push(Placed {
                    button: TouchButton::Slot(n),
                    cx: x0 + i as f32 * (side + gap) + side / 2.0,
                    cy,
                    r: side / 2.0,
                    shape: Shape::Square,
                });
            }
        }
        let split_x = if s.lefty {
            screen.width * 0.55
        } else {
            screen.width * 0.45
        };
        Self {
            buttons,
            idle_stick,
            stick_radius,
            knob_radius: 23.0 * dp,
            split_x,
            lefty: s.lefty,
            scale: dp,
            opacity: s.opacity.clamp(0.1, 1.0),
        }
    }

    /// The control under `(x, y)`; the one drawn last wins.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<TouchButton> {
        self.buttons
            .iter()
            .rev()
            .find(|b| b.contains(x, y))
            .map(|b| b.button)
    }

    /// Whether `x` is on the move (stick) side.
    #[must_use]
    pub fn in_move_side(&self, x: f32) -> bool {
        if self.lefty {
            x >= self.split_x
        } else {
            x < self.split_x
        }
    }

    /// A placed control.
    #[must_use]
    pub fn get(&self, button: TouchButton) -> Option<&Placed> {
        self.buttons.iter().find(|b| b.button == button)
    }
}

/// Finger state change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// Finger down.
    Started,
    /// Finger moved.
    Moved,
    /// Finger lifted.
    Ended,
    /// The OS took the touch away (system gesture, call).
    Cancelled,
}

/// One finger event (physical px, milliseconds on any steady clock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    /// Finger id, stable while the finger is down.
    pub id: u64,
    /// Phase.
    pub phase: TouchPhase,
    /// X (px).
    pub x: f32,
    /// Y (px).
    pub y: f32,
    /// Time (ms).
    pub time_ms: f64,
}

/// What the screen is showing, which changes what touches mean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchMode {
    /// A level is running: stick, look, buttons.
    #[default]
    Play,
    /// A menu is open: drag up/down to move, tap to choose.
    Menu,
    /// Intermission, text screen, title: tap to continue.
    TapToContinue,
}

/// Menu navigation from touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Choose the item under the cursor / continue.
    Activate,
    /// A finger came down here (screen px): select the item under it.
    Point {
        /// X.
        x: i32,
        /// Y.
        y: i32,
    },
    /// A tap here (screen px): choose the item under it, or the selected
    /// one when the tap is beside the items.
    ActivateAt {
        /// X.
        x: i32,
        /// Y.
        y: i32,
    },
}

/// Vibration request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buzz {
    /// Short tick (buttons).
    Light,
    /// Stronger pulse (fire, picker opening).
    Heavy,
}

/// One frame's worth of touch input for the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchFrame {
    /// Actions held right now (stick directions, run, buttons).
    pub keys: Vec<Key>,
    /// Analog stick `(x right, y forward)`, magnitude ≤ 1, when pushed.
    pub analog: Option<(f32, f32)>,
    /// Look in mouse counts since the last frame (`+x` right, `+y` down;
    /// already inverted when [`TouchSettings::invert_y`] is set).
    pub look: (i32, i32),
    /// Weapon cycle steps (`+1` next, `-1` previous).
    pub weapon_cycle: i32,
    /// Weapon slot picked in the picker (`1..=9`, `0` for Duke slot 10).
    pub weapon_slot: Option<u8>,
    /// Menu button pressed.
    pub toggle_menu: bool,
    /// Automap button pressed.
    pub toggle_map: bool,
    /// Automap zoom steps (`+1` per tap on `+`, `-1` per tap on `-`).
    pub map_zoom: i32,
    /// Menu navigation ([`TouchMode::Menu`] / [`TouchMode::TapToContinue`]).
    pub menu: Vec<MenuAction>,
    /// Vibrations to play (empty when haptics are off).
    pub buzz: Vec<Buzz>,
}

#[derive(Clone, Copy, Debug)]
struct StickTouch {
    id: u64,
    /// Current stick centre (it follows the finger past the rim).
    cx: f32,
    cy: f32,
    /// Knob offset inside the unit circle, y down.
    dx: f32,
    dy: f32,
}

#[derive(Clone, Copy, Debug)]
struct LookTouch {
    x: f32,
    y: f32,
    t: f64,
}

#[derive(Clone, Copy, Debug)]
struct WeaponHold {
    id: u64,
    steps: i32,
    since: f64,
    long: bool,
}

#[derive(Clone, Copy, Debug)]
struct MenuTouch {
    start_x: f32,
    start_y: f32,
    start_t: f64,
    acc_y: f32,
    moved: bool,
}

/// Running state of the on-screen controls.
#[derive(Clone, Debug, Default)]
pub struct TouchState {
    mode: TouchMode,
    stick: Option<StickTouch>,
    looks: BTreeMap<u64, LookTouch>,
    /// Buttons held, by finger.
    held: BTreeMap<u64, TouchButton>,
    /// Fingers firing from the look side (double-tap and hold).
    look_fire: BTreeSet<u64>,
    /// Look-side touch-downs, for tap detection.
    down_at: BTreeMap<u64, (f64, f32, f32)>,
    last_tap: Option<(f64, f32, f32)>,
    weapon_hold: Option<WeaponHold>,
    slots_open_since: Option<f64>,
    menu_touches: BTreeMap<u64, MenuTouch>,
    acc: (f32, f32),
    out: TouchFrame,
    /// A touch was seen (the shell shows the controls from then on).
    pub seen: bool,
}

impl TouchState {
    /// Fresh state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current mode.
    #[must_use]
    pub fn mode(&self) -> TouchMode {
        self.mode
    }

    /// Switch mode; held controls are released so nothing sticks.
    pub fn set_mode(&mut self, mode: TouchMode) {
        if mode != self.mode {
            self.release_all();
            self.mode = mode;
        }
    }

    /// Drop every finger (app backgrounded, focus lost, mode change).
    pub fn release_all(&mut self) {
        self.stick = None;
        self.looks.clear();
        self.held.clear();
        self.look_fire.clear();
        self.down_at.clear();
        self.weapon_hold = None;
        self.slots_open_since = None;
        self.menu_touches.clear();
        self.acc = (0.0, 0.0);
    }

    /// Whether the weapon picker is showing.
    #[must_use]
    pub fn slots_open(&self) -> bool {
        self.slots_open_since.is_some()
    }

    /// Whether `button` is held (drawn highlighted).
    #[must_use]
    pub fn is_held(&self, button: TouchButton) -> bool {
        self.held.values().any(|b| *b == button)
            || (button == TouchButton::Fire && !self.look_fire.is_empty())
            || self.weapon_hold.is_some_and(|w| {
                (w.steps > 0 && button == TouchButton::WeaponNext)
                    || (w.steps < 0 && button == TouchButton::WeaponPrev)
            })
    }

    /// Stick centre and knob offset (unit circle, y down), when a thumb is on it.
    #[must_use]
    pub fn stick(&self) -> Option<((f32, f32), (f32, f32))> {
        self.stick.map(|s| ((s.cx, s.cy), (s.dx, s.dy)))
    }

    /// Whether the stick is pushed far enough to run.
    #[must_use]
    pub fn running(&self) -> bool {
        self.stick.is_some_and(|s| s.dx.hypot(s.dy) >= RUN_AT)
    }

    fn buzz(&mut self, s: &TouchSettings, b: Buzz) {
        if s.haptics {
            self.out.buzz.push(b);
        }
    }

    /// Advance timers: weapon-picker long press and auto-close.
    pub fn update(&mut self, now_ms: f64, s: &TouchSettings) {
        if let Some(hold) = self.weapon_hold.as_mut() {
            if !hold.long && now_ms - hold.since >= LONG_PRESS_MS {
                hold.long = true;
                self.slots_open_since = Some(now_ms);
                self.buzz(s, Buzz::Heavy);
            }
        }
        if self
            .slots_open_since
            .is_some_and(|t| now_ms - t >= SLOTS_OPEN_MS)
        {
            self.slots_open_since = None;
        }
    }

    /// Feed one finger event. `layout` must match what is on screen.
    pub fn event(&mut self, ev: TouchEvent, layout: &TouchLayout, s: &TouchSettings) {
        self.seen = true;
        match self.mode {
            TouchMode::Play => self.play_event(ev, layout, s),
            TouchMode::Menu | TouchMode::TapToContinue => self.menu_event(ev, layout, s),
        }
    }

    fn menu_event(&mut self, ev: TouchEvent, layout: &TouchLayout, s: &TouchSettings) {
        let step = MENU_STEP_DP * layout.scale;
        match ev.phase {
            TouchPhase::Started => {
                if self.mode == TouchMode::Menu && layout.hit(ev.x, ev.y) == Some(TouchButton::Menu)
                {
                    self.out.toggle_menu = true;
                    self.buzz(s, Buzz::Light);
                    return;
                }
                if self.mode == TouchMode::Menu {
                    self.out.menu.push(MenuAction::Point {
                        x: ev.x as i32,
                        y: ev.y as i32,
                    });
                }
                self.menu_touches.insert(
                    ev.id,
                    MenuTouch {
                        start_x: ev.x,
                        start_y: ev.y,
                        start_t: ev.time_ms,
                        acc_y: ev.y,
                        moved: false,
                    },
                );
            }
            TouchPhase::Moved => {
                let Some(t) = self.menu_touches.get_mut(&ev.id) else {
                    return;
                };
                if self.mode == TouchMode::Menu {
                    while ev.y - t.acc_y >= step {
                        t.acc_y += step;
                        t.moved = true;
                        self.out.menu.push(MenuAction::Down);
                    }
                    while t.acc_y - ev.y >= step {
                        t.acc_y -= step;
                        t.moved = true;
                        self.out.menu.push(MenuAction::Up);
                    }
                }
                if (ev.x - t.start_x).hypot(ev.y - t.start_y) > 14.0 * layout.scale {
                    t.moved = true;
                }
            }
            TouchPhase::Ended => {
                if let Some(t) = self.menu_touches.remove(&ev.id) {
                    if !t.moved && ev.time_ms - t.start_t < 600.0 {
                        self.out.menu.push(if self.mode == TouchMode::Menu {
                            MenuAction::ActivateAt {
                                x: ev.x as i32,
                                y: ev.y as i32,
                            }
                        } else {
                            MenuAction::Activate
                        });
                        self.buzz(s, Buzz::Light);
                    }
                }
            }
            TouchPhase::Cancelled => {
                self.menu_touches.remove(&ev.id);
            }
        }
    }

    fn play_event(&mut self, ev: TouchEvent, layout: &TouchLayout, s: &TouchSettings) {
        match ev.phase {
            TouchPhase::Started => self.play_start(ev, layout, s),
            TouchPhase::Moved => self.play_move(ev, layout, s),
            TouchPhase::Ended | TouchPhase::Cancelled => self.play_end(ev, s),
        }
    }

    fn play_start(&mut self, ev: TouchEvent, layout: &TouchLayout, s: &TouchSettings) {
        if let Some(button) = layout.hit(ev.x, ev.y) {
            match button {
                TouchButton::Slot(n) => {
                    self.out.weapon_slot = Some(n);
                    self.slots_open_since = None;
                    self.buzz(s, Buzz::Light);
                }
                TouchButton::WeaponNext | TouchButton::WeaponPrev => {
                    self.weapon_hold = Some(WeaponHold {
                        id: ev.id,
                        steps: if button == TouchButton::WeaponNext {
                            1
                        } else {
                            -1
                        },
                        since: ev.time_ms,
                        long: false,
                    });
                }
                TouchButton::Map => {
                    self.out.toggle_map = true;
                    self.buzz(s, Buzz::Light);
                }
                TouchButton::Radar => {
                    self.out.toggle_map = true;
                    self.buzz(s, Buzz::Light);
                }
                TouchButton::MapZoomIn | TouchButton::MapZoomOut => {
                    self.out.map_zoom += if button == TouchButton::MapZoomIn {
                        1
                    } else {
                        -1
                    };
                    self.buzz(s, Buzz::Light);
                }
                TouchButton::Menu => {
                    self.out.toggle_menu = true;
                    self.buzz(s, Buzz::Light);
                }
                held => {
                    self.held.insert(ev.id, held);
                    if held == TouchButton::Fire {
                        // FIRE also aims: drags on it look around.
                        self.looks.insert(
                            ev.id,
                            LookTouch {
                                x: ev.x,
                                y: ev.y,
                                t: ev.time_ms,
                            },
                        );
                        self.buzz(s, Buzz::Heavy);
                    } else {
                        self.buzz(s, Buzz::Light);
                    }
                }
            }
            return;
        }
        if layout.in_move_side(ev.x) {
            if self.stick.is_none() {
                self.stick = Some(StickTouch {
                    id: ev.id,
                    cx: ev.x,
                    cy: ev.y,
                    dx: 0.0,
                    dy: 0.0,
                });
            }
            return;
        }
        // Look side.
        self.looks.insert(
            ev.id,
            LookTouch {
                x: ev.x,
                y: ev.y,
                t: ev.time_ms,
            },
        );
        self.down_at.insert(ev.id, (ev.time_ms, ev.x, ev.y));
        if s.tap_fire {
            if let Some((t, x, y)) = self.last_tap {
                if ev.time_ms - t < DOUBLE_TAP_MS
                    && (ev.x - x).hypot(ev.y - y) < 60.0 * layout.scale
                {
                    self.look_fire.insert(ev.id);
                    self.buzz(s, Buzz::Heavy);
                }
            }
        }
    }

    fn play_move(&mut self, ev: TouchEvent, layout: &TouchLayout, s: &TouchSettings) {
        if let Some(stick) = self.stick.as_mut().filter(|st| st.id == ev.id) {
            let r = layout.stick_radius;
            let mut dx = (ev.x - stick.cx) / r;
            let mut dy = (ev.y - stick.cy) / r;
            let mag = dx.hypot(dy);
            if mag > 1.0 {
                // Past the rim: drag the stick along so turning back responds at once.
                dx /= mag;
                dy /= mag;
                stick.cx = ev.x - dx * r;
                stick.cy = ev.y - dy * r;
            }
            stick.dx = dx;
            stick.dy = dy;
            return;
        }
        let Some(last) = self.looks.get_mut(&ev.id) else {
            return;
        };
        let dx_dp = (ev.x - last.x) / layout.scale;
        let dy_dp = (ev.y - last.y) / layout.scale;
        let dt = (ev.time_ms - last.t).max(1.0);
        // Acceleration: slow drags stay precise, quick flicks turn further.
        let mut gain = 1.0_f32;
        if s.accel {
            let speed = dx_dp.hypot(dy_dp) / dt as f32; // dp per ms
            gain += ((speed - 0.25) / 1.5).clamp(0.0, 1.0);
        }
        let k = LOOK_COUNTS_PER_DP * s.look * gain;
        let sign_y = if s.invert_y { -1.0 } else { 1.0 };
        self.acc.0 += dx_dp * k;
        self.acc.1 += dy_dp * k * s.look_y * sign_y;
        let ix = self.acc.0.trunc();
        let iy = self.acc.1.trunc();
        self.acc.0 -= ix;
        self.acc.1 -= iy;
        self.out.look.0 = self.out.look.0.saturating_add(ix as i32);
        self.out.look.1 = self.out.look.1.saturating_add(iy as i32);
        last.x = ev.x;
        last.y = ev.y;
        last.t = ev.time_ms;
    }

    fn play_end(&mut self, ev: TouchEvent, s: &TouchSettings) {
        let ended = ev.phase == TouchPhase::Ended;
        if self.stick.is_some_and(|st| st.id == ev.id) {
            self.stick = None;
        }
        self.held.remove(&ev.id);
        self.looks.remove(&ev.id);
        if let Some(hold) = self.weapon_hold.filter(|h| h.id == ev.id) {
            self.weapon_hold = None;
            if ended && !hold.long {
                self.out.weapon_cycle += hold.steps;
                self.buzz(s, Buzz::Light);
            }
        }
        let down = self.down_at.remove(&ev.id);
        if self.look_fire.remove(&ev.id) {
            self.last_tap = None;
            return;
        }
        // A short, still touch counts as a tap for the double tap.
        if let (true, Some((t, x, y))) = (ended, down) {
            if ev.time_ms - t < 220.0 && (ev.x - x).hypot(ev.y - y) < 14.0 * s.size.max(0.5) {
                self.last_tap = Some((ev.time_ms, ev.x, ev.y));
            }
        }
    }

    /// Keys held right now.
    #[must_use]
    pub fn held_keys(&self) -> Vec<Key> {
        let mut keys = Vec::new();
        let mut add = |k: Key| {
            if !keys.contains(&k) {
                keys.push(k);
            }
        };
        if let Some(st) = self.stick {
            if st.dy < -KEY_DEAD {
                add(Key::Forward);
            }
            if st.dy > KEY_DEAD {
                add(Key::Back);
            }
            if st.dx < -KEY_DEAD {
                add(Key::MoveLeft);
            }
            if st.dx > KEY_DEAD {
                add(Key::MoveRight);
            }
            if st.dx.hypot(st.dy) >= RUN_AT {
                add(Key::Speed);
            }
        }
        for b in self.held.values() {
            if let Some(k) = b.held_key() {
                add(k);
            }
        }
        if !self.look_fire.is_empty() {
            add(Key::Attack);
        }
        keys
    }

    /// Analog stick: rescaled past the dead zone so a light push creeps.
    #[must_use]
    pub fn analog(&self) -> Option<(f32, f32)> {
        let st = self.stick?;
        let m = st.dx.hypot(st.dy).min(1.0);
        if m <= ANALOG_DEAD {
            return None;
        }
        let out = (m - ANALOG_DEAD) / (1.0 - ANALOG_DEAD);
        Some((st.dx / m * out, -st.dy / m * out))
    }

    /// This frame's input; one-shot parts (look, taps) are consumed.
    pub fn take_frame(&mut self) -> TouchFrame {
        let mut frame = std::mem::take(&mut self.out);
        if self.mode == TouchMode::Play {
            frame.keys = self.held_keys();
            frame.analog = self.analog();
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const PHONES: [(f32, f32, f32); 3] = [
        // Landscape phones (px) at their display scale.
        (2400.0, 1080.0, 3.0),   // 800×360 dp
        (2400.0, 1080.0, 2.625), // 914×411 dp
        (2796.0, 1290.0, 3.0),   // 932×430 dp
    ];

    fn screen(i: usize) -> Screen {
        let (w, h, s) = PHONES[i];
        Screen::new(w, h, s)
    }

    fn ev(id: u64, phase: TouchPhase, x: f32, y: f32, t: f64) -> TouchEvent {
        TouchEvent {
            id,
            phase,
            x,
            y,
            time_ms: t,
        }
    }

    fn setup() -> (TouchState, TouchLayout, TouchSettings) {
        let s = TouchSettings::default();
        let layout = TouchLayout::new(&screen(1), &s, &LayoutOptions::default());
        (TouchState::new(), layout, s)
    }

    fn center(layout: &TouchLayout, b: TouchButton) -> (f32, f32) {
        let p = layout.get(b).expect("button");
        (p.cx, p.cy)
    }

    #[test]
    fn every_layout_fits_the_screen_without_overlaps() {
        for i in 0..PHONES.len() {
            for duke in [false, true] {
                for lefty in [false, true] {
                    for size in [0.8, 1.0, 1.2] {
                        let mut sc = screen(i);
                        sc.inset_left = 90.0; // notch side
                        sc.inset_bottom = 40.0; // gesture bar
                        let s = TouchSettings {
                            lefty,
                            size,
                            ..TouchSettings::default()
                        };
                        let opts = LayoutOptions {
                            duke,
                            slots_open: true,
                            map_open: true,
                            radar: true,
                            avoid: None,
                        };
                        let l = TouchLayout::new(&sc, &s, &opts);
                        let safe = Rect {
                            x: sc.inset_left,
                            y: sc.inset_top,
                            w: sc.width - sc.inset_left - sc.inset_right,
                            h: sc.height - sc.inset_top - sc.inset_bottom,
                        };
                        for b in &l.buttons {
                            assert!(
                                b.rect().inside(&safe),
                                "{:?} outside the safe area (phone {i}, lefty {lefty}, size {size})",
                                b.button
                            );
                        }
                        for (n, a) in l.buttons.iter().enumerate() {
                            for b in &l.buttons[n + 1..] {
                                assert!(
                                    !a.rect().intersects(&b.rect())
                                        || matches!(
                                            (a.shape, b.shape),
                                            (Shape::Circle, Shape::Circle)
                                        ) && (a.cx - b.cx).hypot(a.cy - b.cy) >= a.r + b.r,
                                    "{:?} overlaps {:?} (phone {i}, duke {duke}, lefty {lefty}, size {size})",
                                    a.button,
                                    b.button
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn duke_adds_kick_and_item_and_jump_duck_can_be_hidden() {
        let sc = screen(1);
        let doom = TouchLayout::new(
            &sc,
            &TouchSettings {
                jump_duck: false,
                ..TouchSettings::default()
            },
            &LayoutOptions::default(),
        );
        for b in [
            TouchButton::Jump,
            TouchButton::Duck,
            TouchButton::Kick,
            TouchButton::Item,
            TouchButton::ItemPrev,
            TouchButton::ItemNext,
        ] {
            assert!(doom.get(b).is_none(), "{b:?}");
        }
        let duke = TouchLayout::new(
            &sc,
            &TouchSettings {
                jump_duck: false,
                ..TouchSettings::default()
            },
            &LayoutOptions {
                duke: true,
                ..LayoutOptions::default()
            },
        );
        for b in [
            TouchButton::Jump,
            TouchButton::Duck,
            TouchButton::Kick,
            TouchButton::Item,
            TouchButton::ItemPrev,
            TouchButton::ItemNext,
        ] {
            assert!(duke.get(b).is_some(), "{b:?}");
        }
        // The inventory pair sits in the top bar beside MAP, next first.
        let map = duke.get(TouchButton::Map).unwrap();
        let next = duke.get(TouchButton::ItemNext).unwrap();
        let prev = duke.get(TouchButton::ItemPrev).unwrap();
        assert!((next.cy - map.cy).abs() < 0.01 && (prev.cy - map.cy).abs() < 0.01);
        assert!(prev.cx < next.cx && next.cx < map.cx);
    }

    #[test]
    fn the_cluster_lifts_clear_of_the_status_bar() {
        for i in 0..PHONES.len() {
            let sc = screen(i);
            // Doom's 320×32 status bar scaled to the screen height, centred.
            let k = sc.height / 200.0;
            let bar = Rect {
                x: (sc.width - 320.0 * k) / 2.0,
                y: sc.height - 32.0 * k,
                w: 320.0 * k,
                h: 32.0 * k,
            };
            let s = TouchSettings::default();
            let plain = TouchLayout::new(&sc, &s, &LayoutOptions::default());
            assert!(
                plain.buttons.iter().any(|b| b.rect().intersects(&bar)),
                "phone {i}: the web layout covers the bar (the bug being fixed)"
            );
            let l = TouchLayout::new(
                &sc,
                &s,
                &LayoutOptions {
                    avoid: Some(bar),
                    ..LayoutOptions::default()
                },
            );
            for b in &l.buttons {
                assert!(
                    !b.rect().intersects(&bar),
                    "phone {i}: {:?} on the bar",
                    b.button
                );
                assert!(b.rect().y >= 0.0, "phone {i}: {:?} off the top", b.button);
            }
            let r = l.stick_radius;
            assert!(
                l.idle_stick.1 + r <= bar.y,
                "phone {i}: resting stick on the bar"
            );
            // The cluster keeps its shape: FIRE→USE offset unchanged.
            let d = |l: &TouchLayout| {
                let (fx, fy) = center(l, TouchButton::Fire);
                let (ux, uy) = center(l, TouchButton::Use);
                (ux - fx, uy - fy)
            };
            assert_eq!(d(&plain), d(&l));
        }
    }

    #[test]
    fn short_screens_use_the_compact_cluster() {
        let s = TouchSettings::default();
        let short = TouchLayout::new(&screen(0), &s, &LayoutOptions::default());
        let tall = TouchLayout::new(&screen(1), &s, &LayoutOptions::default());
        let fire = |l: &TouchLayout| l.get(TouchButton::Fire).map(|p| p.r / l.scale);
        assert_eq!(fire(&short), Some(39.0));
        assert_eq!(fire(&tall), Some(46.0));
    }

    #[test]
    fn lefty_mirrors_the_sides() {
        let sc = screen(1);
        let right = TouchLayout::new(&sc, &TouchSettings::default(), &LayoutOptions::default());
        let left = TouchLayout::new(
            &sc,
            &TouchSettings {
                lefty: true,
                ..TouchSettings::default()
            },
            &LayoutOptions::default(),
        );
        let (rx, ry) = center(&right, TouchButton::Fire);
        let (lx, ly) = center(&left, TouchButton::Fire);
        assert!((rx + lx - sc.width).abs() < 0.01 && (ry - ly).abs() < 0.01);
        assert!(right.in_move_side(100.0) && !right.in_move_side(sc.width - 100.0));
        assert!(left.in_move_side(sc.width - 100.0) && !left.in_move_side(100.0));
        assert!((right.idle_stick.0 + left.idle_stick.0 - sc.width).abs() < 0.01);
    }

    #[test]
    fn full_stick_push_runs_forward_light_push_creeps() {
        let (mut t, l, s) = setup();
        let r = l.stick_radius;
        t.event(ev(1, TouchPhase::Started, 300.0, 700.0, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Moved, 300.0, 700.0 - r, 16.0), &l, &s);
        let f = t.take_frame();
        assert_eq!(f.keys, vec![Key::Forward, Key::Speed]);
        let (x, y) = f.analog.expect("pushed");
        assert!(x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5);
        // A fifth of a push: analog creeps, no keys.
        t.event(
            ev(1, TouchPhase::Moved, 300.0, 700.0 - r * 0.2, 32.0),
            &l,
            &s,
        );
        let f = t.take_frame();
        assert!(f.keys.is_empty());
        let (_, y) = f.analog.expect("creeping");
        assert!(y > 0.0 && y < 0.1, "{y}");
        // Inside the dead zone: nothing.
        t.event(
            ev(1, TouchPhase::Moved, 300.0, 700.0 - r * 0.05, 48.0),
            &l,
            &s,
        );
        assert_eq!(t.take_frame().analog, None);
        // Lift: everything released.
        t.event(ev(1, TouchPhase::Ended, 300.0, 700.0, 64.0), &l, &s);
        let f = t.take_frame();
        assert!(f.keys.is_empty() && f.analog.is_none());
    }

    #[test]
    fn diagonal_push_presses_two_keys_and_any_angle() {
        let (mut t, l, s) = setup();
        let r = l.stick_radius;
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        t.event(ev(1, TouchPhase::Started, 300.0, 700.0, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Moved, 300.0 + d, 700.0 + d, 16.0), &l, &s);
        let f = t.take_frame();
        assert_eq!(f.keys, vec![Key::Back, Key::MoveRight, Key::Speed]);
        let (x, y) = f.analog.expect("pushed");
        assert!((x - y.abs()).abs() < 1e-4 && y < 0.0);
    }

    #[test]
    fn the_stick_follows_the_thumb_past_its_rim() {
        let (mut t, l, s) = setup();
        let r = l.stick_radius;
        t.event(ev(1, TouchPhase::Started, 400.0, 700.0, 0.0), &l, &s);
        // Far past the rim upwards: the stick is dragged along.
        t.event(
            ev(1, TouchPhase::Moved, 400.0, 700.0 - 3.0 * r, 16.0),
            &l,
            &s,
        );
        let ((_, cy), _) = t.stick().expect("on");
        assert!((cy - (700.0 - 2.0 * r)).abs() < 0.01);
        // Coming back just one radius reverses at once (no dead travel).
        t.event(
            ev(1, TouchPhase::Moved, 400.0, 700.0 - 1.5 * r, 32.0),
            &l,
            &s,
        );
        assert!(t.take_frame().keys.contains(&Key::Back));
    }

    #[test]
    fn fire_and_move_at_once_and_fire_aims() {
        let (mut t, l, s) = setup();
        let (fx, fy) = center(&l, TouchButton::Fire);
        t.event(ev(1, TouchPhase::Started, 300.0, 700.0, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Moved, 300.0, 600.0, 10.0), &l, &s);
        t.event(ev(2, TouchPhase::Started, fx, fy, 20.0), &l, &s);
        // Dragging on FIRE turns: 100 px right at 2.625 px/dp ≈ 38 dp.
        t.event(ev(2, TouchPhase::Moved, fx + 100.0, fy, 400.0), &l, &s);
        let f = t.take_frame();
        assert!(f.keys.contains(&Key::Forward) && f.keys.contains(&Key::Attack));
        assert_eq!(f.look, (11, 0), "38 dp × 0.3 counts/dp, slow drag");
        assert!(t.is_held(TouchButton::Fire));
        assert_eq!(f.buzz, vec![Buzz::Heavy]);
        t.event(ev(2, TouchPhase::Ended, fx + 100.0, fy, 500.0), &l, &s);
        let f = t.take_frame();
        assert!(!f.keys.contains(&Key::Attack) && f.keys.contains(&Key::Forward));
    }

    #[test]
    fn slow_drags_accumulate_fractions_and_flicks_turn_further() {
        let (mut t, l, s) = setup();
        let x0 = l.split_x + 200.0;
        t.event(ev(1, TouchPhase::Started, x0, 300.0, 0.0), &l, &s);
        // 1 px steps, slow: 0.3/2.625 ≈ 0.114 counts each.
        let mut x = x0;
        for i in 1..=30 {
            x += 1.0;
            t.event(
                ev(1, TouchPhase::Moved, x, 300.0, f64::from(i) * 50.0),
                &l,
                &s,
            );
        }
        let slow = t.take_frame().look.0;
        assert_eq!(slow, 3, "30 px ≈ 11.4 dp ≈ 3.4 counts");
        // The same 30 px as one quick flick turns further.
        t.event(ev(1, TouchPhase::Moved, x + 300.0, 300.0, 1600.0), &l, &s);
        let quick = t.take_frame().look.0;
        assert!(quick > 10 * slow, "flick {quick} vs {slow}");
        // Up/down is scaled by look_y and can be inverted.
        let (mut t, l, mut s) = setup();
        t.event(ev(1, TouchPhase::Started, x0, 300.0, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Moved, x0, 300.0 + 105.0, 1000.0), &l, &s);
        assert_eq!(t.take_frame().look, (0, 19), "40 dp × 0.3 × 1.6");
        s.invert_y = true;
        t.event(ev(1, TouchPhase::Moved, x0, 300.0 + 210.0, 2000.0), &l, &s);
        assert_eq!(t.take_frame().look, (0, -19));
    }

    #[test]
    fn double_tap_and_hold_on_the_look_side_fires() {
        let (mut t, l, s) = setup();
        let (x, y) = (l.split_x + 300.0, 400.0);
        t.event(ev(1, TouchPhase::Started, x, y, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Ended, x, y, 80.0), &l, &s);
        assert!(!t.take_frame().keys.contains(&Key::Attack));
        t.event(ev(2, TouchPhase::Started, x + 10.0, y, 200.0), &l, &s);
        let f = t.take_frame();
        assert!(f.keys.contains(&Key::Attack));
        assert!(t.is_held(TouchButton::Fire));
        t.event(ev(2, TouchPhase::Ended, x + 10.0, y, 900.0), &l, &s);
        assert!(!t.take_frame().keys.contains(&Key::Attack));
        // Too slow a second tap, or tap-fire off: no fire.
        let (mut t, l, mut s) = setup();
        t.event(ev(1, TouchPhase::Started, x, y, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Ended, x, y, 80.0), &l, &s);
        t.event(ev(2, TouchPhase::Started, x, y, 500.0), &l, &s);
        assert!(!t.take_frame().keys.contains(&Key::Attack));
        s.tap_fire = false;
        t.event(ev(2, TouchPhase::Ended, x, y, 560.0), &l, &s);
        t.event(ev(3, TouchPhase::Started, x, y, 600.0), &l, &s);
        assert!(!t.take_frame().keys.contains(&Key::Attack));
    }

    #[test]
    fn weapon_arrow_tap_cycles_and_hold_opens_the_picker() {
        let (mut t, l, s) = setup();
        let (nx, ny) = center(&l, TouchButton::WeaponNext);
        t.event(ev(1, TouchPhase::Started, nx, ny, 0.0), &l, &s);
        t.update(100.0, &s);
        t.event(ev(1, TouchPhase::Ended, nx, ny, 150.0), &l, &s);
        assert_eq!(t.take_frame().weapon_cycle, 1);
        let (px, py) = center(&l, TouchButton::WeaponPrev);
        t.event(ev(2, TouchPhase::Started, px, py, 1000.0), &l, &s);
        t.update(1500.0, &s);
        assert!(t.slots_open());
        t.event(ev(2, TouchPhase::Ended, px, py, 1600.0), &l, &s);
        let f = t.take_frame();
        assert_eq!(f.weapon_cycle, 0, "a long press does not also cycle");
        assert_eq!(f.buzz, vec![Buzz::Heavy]);
        // Pick slot 3 from the open picker.
        let open = TouchLayout::new(
            &screen(1),
            &s,
            &LayoutOptions {
                slots_open: true,
                ..LayoutOptions::default()
            },
        );
        let (sx, sy) = center(&open, TouchButton::Slot(3));
        t.event(ev(3, TouchPhase::Started, sx, sy, 1700.0), &open, &s);
        assert_eq!(t.take_frame().weapon_slot, Some(3));
        assert!(!t.slots_open());
        // Left alone, the picker closes after four seconds.
        t.event(ev(4, TouchPhase::Started, px, py, 2000.0), &l, &s);
        t.update(2500.0, &s);
        t.event(ev(4, TouchPhase::Ended, px, py, 2600.0), &l, &s);
        t.update(6400.0, &s);
        assert!(t.slots_open());
        t.update(6600.0, &s);
        assert!(!t.slots_open());
    }

    #[test]
    fn buttons_hold_their_keys_and_top_buttons_toggle() {
        let (mut t, _, s) = setup();
        let l = TouchLayout::new(
            &screen(1),
            &s,
            &LayoutOptions {
                duke: true,
                ..LayoutOptions::default()
            },
        );
        let press = [
            (TouchButton::Use, Key::Use),
            (TouchButton::Jump, Key::Jump),
            (TouchButton::Duck, Key::MoveDown),
            (TouchButton::Kick, Key::QuickKick),
            (TouchButton::Item, Key::InvUse),
            (TouchButton::ItemPrev, Key::InvPrev),
            (TouchButton::ItemNext, Key::InvNext),
        ];
        for (i, (b, k)) in press.iter().enumerate() {
            let (x, y) = center(&l, *b);
            t.event(ev(10 + i as u64, TouchPhase::Started, x, y, 0.0), &l, &s);
            assert!(t.take_frame().keys.contains(k), "{b:?}");
        }
        assert_eq!(
            t.take_frame().keys.len(),
            press.len(),
            "all five held at once"
        );
        for i in 0..press.len() {
            t.event(
                ev(10 + i as u64, TouchPhase::Cancelled, 0.0, 0.0, 10.0),
                &l,
                &s,
            );
        }
        assert!(t.take_frame().keys.is_empty());
        let (mx, my) = center(&l, TouchButton::Map);
        t.event(ev(1, TouchPhase::Started, mx, my, 20.0), &l, &s);
        let (ux, uy) = center(&l, TouchButton::Menu);
        t.event(ev(2, TouchPhase::Started, ux, uy, 20.0), &l, &s);
        let f = t.take_frame();
        assert!(f.toggle_map && f.toggle_menu);
        assert!(f.keys.is_empty(), "top buttons hold nothing");
        let f = t.take_frame();
        assert!(!f.toggle_map && !f.toggle_menu, "one-shot");
    }

    #[test]
    fn radar_sits_in_the_free_corner_and_opens_the_map() {
        let (mut t, plain, s) = setup();
        assert!(plain.get(TouchButton::Radar).is_none(), "only when asked");
        let opts = LayoutOptions {
            radar: true,
            ..LayoutOptions::default()
        };
        let sc = screen(1);
        let l = TouchLayout::new(&sc, &s, &opts);
        let r = l.get(TouchButton::Radar).expect("radar");
        let d = radar_diameter_dp(&sc);
        assert!((76.0..=116.0).contains(&d));
        assert!((r.r * 2.0 - d * l.scale).abs() < 0.5);
        assert!(r.cx < sc.width / 2.0 && r.cy < sc.height / 2.0, "top-left");
        for b in l.buttons.iter().filter(|b| b.button != TouchButton::Radar) {
            assert!(
                !r.rect().intersects(&b.rect()),
                "radar covers {:?}",
                b.button
            );
        }
        let (x, y) = (r.cx, r.cy);
        t.event(ev(1, TouchPhase::Started, x, y, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Ended, x, y, 60.0), &l, &s);
        let f = t.take_frame();
        assert!(f.toggle_map, "a tap on the radar opens the automap");
        assert_eq!(f.look, (0, 0), "and does not turn the view");
        // Left-handed: the top bar moves left, the radar right.
        let lefty = TouchSettings { lefty: true, ..s };
        let l = TouchLayout::new(&sc, &lefty, &opts);
        assert!(l.get(TouchButton::Radar).expect("radar").cx > sc.width / 2.0);
    }

    #[test]
    fn open_automap_shows_zoom_buttons() {
        let (mut t, closed, s) = setup();
        assert!(closed.get(TouchButton::MapZoomIn).is_none());
        let open = TouchLayout::new(
            &screen(1),
            &s,
            &LayoutOptions {
                map_open: true,
                ..LayoutOptions::default()
            },
        );
        let (ix, iy) = center(&open, TouchButton::MapZoomIn);
        let (ox, oy) = center(&open, TouchButton::MapZoomOut);
        let (mx, _) = center(&open, TouchButton::Map);
        let step = 48.0 * open.scale;
        assert!(
            (mx - ix - step).abs() < 0.5 && (ix - ox - step).abs() < 0.5,
            "MAP, +, - side by side: {mx} {ix} {ox}"
        );
        t.event(ev(1, TouchPhase::Started, ix, iy, 0.0), &open, &s);
        t.event(ev(1, TouchPhase::Ended, ix, iy, 50.0), &open, &s);
        t.event(ev(2, TouchPhase::Started, ix, iy, 100.0), &open, &s);
        t.event(ev(3, TouchPhase::Started, ox, oy, 100.0), &open, &s);
        let f = t.take_frame();
        assert_eq!(f.map_zoom, 1, "two + taps and one -");
        assert!(f.keys.is_empty(), "zoom buttons hold nothing");
        assert_eq!(t.take_frame().map_zoom, 0, "one-shot");
    }

    #[test]
    fn haptics_off_means_no_buzz() {
        let (mut t, l, mut s) = setup();
        s.haptics = false;
        let (fx, fy) = center(&l, TouchButton::Fire);
        t.event(ev(1, TouchPhase::Started, fx, fy, 0.0), &l, &s);
        assert!(t.take_frame().buzz.is_empty());
    }

    #[test]
    fn menu_mode_drags_move_the_cursor_and_taps_choose() {
        let (mut t, l, s) = setup();
        let r = l.stick_radius;
        // A held stick is released on entering the menu.
        t.event(ev(9, TouchPhase::Started, 300.0, 700.0, 0.0), &l, &s);
        t.event(ev(9, TouchPhase::Moved, 300.0, 700.0 - r, 5.0), &l, &s);
        t.set_mode(TouchMode::Menu);
        assert!(t.take_frame().keys.is_empty());
        let step = MENU_STEP_DP * l.scale;
        t.event(ev(1, TouchPhase::Started, 1200.0, 300.0, 10.0), &l, &s);
        t.event(
            ev(1, TouchPhase::Moved, 1200.0, 300.0 + 2.5 * step, 200.0),
            &l,
            &s,
        );
        t.event(
            ev(1, TouchPhase::Moved, 1200.0, 300.0 + 0.9 * step, 300.0),
            &l,
            &s,
        );
        t.event(
            ev(1, TouchPhase::Ended, 1200.0, 300.0 + 0.9 * step, 400.0),
            &l,
            &s,
        );
        let f = t.take_frame();
        // The finger first points at the item under it, then drags.
        assert_eq!(
            f.menu,
            vec![
                MenuAction::Point { x: 1200, y: 300 },
                MenuAction::Down,
                MenuAction::Down,
                MenuAction::Up
            ]
        );
        // A tap chooses what is under it.
        t.event(ev(2, TouchPhase::Started, 1200.0, 300.0, 500.0), &l, &s);
        t.event(ev(2, TouchPhase::Ended, 1201.0, 300.0, 560.0), &l, &s);
        assert_eq!(
            t.take_frame().menu,
            vec![
                MenuAction::Point { x: 1200, y: 300 },
                MenuAction::ActivateAt { x: 1201, y: 300 }
            ]
        );
        // The menu button closes the menu.
        let (ux, uy) = center(&l, TouchButton::Menu);
        t.event(ev(3, TouchPhase::Started, ux, uy, 600.0), &l, &s);
        assert!(t.take_frame().toggle_menu);
        // Tap to continue: taps only.
        t.set_mode(TouchMode::TapToContinue);
        t.event(ev(4, TouchPhase::Started, 900.0, 500.0, 700.0), &l, &s);
        t.event(
            ev(4, TouchPhase::Moved, 900.0, 500.0 + 3.0 * step, 720.0),
            &l,
            &s,
        );
        t.event(
            ev(4, TouchPhase::Ended, 900.0, 500.0 + 3.0 * step, 740.0),
            &l,
            &s,
        );
        assert!(t.take_frame().menu.is_empty(), "a drag is not a tap");
        t.event(ev(5, TouchPhase::Started, 900.0, 500.0, 800.0), &l, &s);
        t.event(ev(5, TouchPhase::Ended, 900.0, 500.0, 850.0), &l, &s);
        assert_eq!(t.take_frame().menu, vec![MenuAction::Activate]);
    }

    #[test]
    fn release_all_drops_every_finger() {
        let (mut t, l, s) = setup();
        let (fx, fy) = center(&l, TouchButton::Fire);
        t.event(ev(1, TouchPhase::Started, 300.0, 700.0, 0.0), &l, &s);
        t.event(ev(1, TouchPhase::Moved, 300.0, 500.0, 5.0), &l, &s);
        t.event(ev(2, TouchPhase::Started, fx, fy, 5.0), &l, &s);
        t.release_all();
        let f = t.take_frame();
        assert!(f.keys.is_empty() && f.analog.is_none());
        // Late events for the dropped fingers do nothing.
        t.event(ev(1, TouchPhase::Moved, 300.0, 400.0, 9.0), &l, &s);
        t.event(ev(2, TouchPhase::Ended, fx, fy, 9.0), &l, &s);
        assert!(t.take_frame().keys.is_empty());
    }
}

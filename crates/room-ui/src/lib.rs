//! 2D draw lists, fonts, menus, status bars, HUD, automap, intermission, finale.
//!
//! No GPU dependency — produces [`UiDrawList`] for the renderer.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod acs_hud;
mod automap;
mod canvas_bridge;
mod doom_text;
pub mod endoom;
mod draw;
mod finale;
mod font;
mod hud;
mod i18n;
mod intermission;
mod menu;
mod menudef_runtime;
mod messages;
mod mod_hud;
mod mod_map;
mod mod_zones;
mod patches;
mod sbarinfo_draw;
mod script_sbar;
mod scoreboard;
mod statusbar;
mod title;
mod weapon;
mod zscript_bar;

pub use acs_hud::{color_by_name, draw_acs_hud_messages, strip_color_codes};
pub use automap::{AutomapThing, AutomapView, draw_automap, draw_automap_things};
pub use canvas_bridge::{
    CanvasSink, blit_draw_list, blit_draw_list_full, blit_draw_list_with_fonts,
};
pub use doom_text::{
    draw_stcfn_string, draw_stt_percent, draw_stt_string, draw_stt_string_right,
    measure_stcfn_string, measure_stt_string,
};
pub use draw::{DrawCmd, Rect, Rgba, UiDrawList, Vec2};
pub use finale::{draw_finale, finale_anm_prefix};
pub use font::{FontId, FontRegistry, SoftFont};
pub use hud::{HudInvItem, HudLayer, HudState, screen_blend};
pub use i18n::{StringTable, localize};
pub use intermission::{IntermissionStats, draw_intermission};
pub use menu::{ListMenu, MenuCommand, MenuItem, MenuSfx, MenuStack, parse_menu_action};
pub use menudef_runtime::{
    MenuBuildOpts, MenuGame, any_menu_from_def, list_menu_from_def, option_menu_from_def,
};
pub use messages::MessageQueue;
pub use mod_hud::{
    MarkerCam, ModHudMapCtx, draw_mod_hud, draw_mod_hud_layer, draw_mod_hud_modals,
    player_map_poses, seat_has_modal, set_marker_cam,
};
pub use mod_zones::{draw_mod_zones_automap, draw_outside_zone_tint};
pub use patches::{PatchCache, SoftPatch};
pub use sbarinfo_draw::{SbarInfoBar, SbarInfoFrame, SbarInfoOutput};
pub use statusbar::{DoomStatusBar, DukeStatusBar, StatusBar};
pub use title::{
    anim_pic, anim_pic_loop, draw_title, parse_titledef, splash_ms, title_anim_pic,
    title_screen_pic,
};
pub use weapon::{
    draw_crosshair, draw_weapon, draw_weapon_lit, draw_weapon_tinted, overlay_bob, set_patch_corners,
    overlay_layer_bob, weapon_ready_patch,
};
pub use script_sbar::{
    HUD_CVARS, HUD_FULLSCREEN, HUD_STATUSBAR, PatchDims, SbarSnapshot, SbarWeapon, ScriptStatusBar,
};
pub use scoreboard::{ScoreLine, draw_scoreboard};
pub use zscript_bar::ZscriptStatusBar;

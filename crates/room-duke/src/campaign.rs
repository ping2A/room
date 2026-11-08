//! Atomic episode/level progression (`player.c::fist_incs`).

use serde::{Deserialize, Serialize};

use crate::player::Player;

/// Carry Chocolate `resetplayerstats` persistent state across a map boundary,
/// while retaining the new map's spawn pose and body handle.
pub fn carry_player_state(previous: &Player, next: &mut Player) {
    next.extra = previous.extra;
    next.last_extra = previous.extra;
    next.shield_amount = previous.shield_amount;
    next.steroids_amount = previous.steroids_amount;
    next.scuba_amount = previous.scuba_amount;
    next.holoduke_amount = previous.holoduke_amount;
    next.jetpack_amount = previous.jetpack_amount;
    next.heat_amount = previous.heat_amount;
    next.firstaid_amount = previous.firstaid_amount;
    next.boot_amount = previous.boot_amount;
    next.god_mode = previous.god_mode;
    next.ammo = previous.ammo;
    next.gotweapon = previous.gotweapon;
    next.curr_weapon = previous.curr_weapon;
    next.last_weapon = previous.last_weapon;
    // Chocolate `premap.c::resetprestat` clears both per-level bonus counters
    // on each campaign transition; they intentionally stay at the new map's
    // zero defaults instead of following the carried player inventory.
    next.coop_mode = previous.coop_mode;
    next.game_mode = previous.game_mode;
    next.friendly_fire = previous.friendly_fire;
    next.auto_aim = previous.auto_aim;

    next.heat_on = false;
    next.holoduke_on = -1;
    next.jetpack_on = 0;
    next.scuba_on = 0;
    next.got_access = 0;
    next.access_incs = 0;
    next.access_wallnum = -1;
    next.access_spritenum = -1;
    next.fist_incs = 0;
    next.button_palette = 0;
    next.secret_level = 0;
    next.time_before_exit = 0;
    next.custom_exit_sound = -1;
    next.end_of_game = false;
    next.quick_kick = 0;
    next.kickback_pic = if next.curr_weapon == 1 { 5 } else { 0 };
    next.weapon_pos = 6;
    next.want_weapon_fire = -1;
    next.hbomb_hold_delay = 0;
    next.hbomb_on = false;
    next.rapid_fire_hold = false;
    next.posxv = 0;
    next.posyv = 0;
    next.poszv = 0;
    next.effector_force_x = 0;
    next.effector_force_y = 0;
}

/// Atomic finale animation sound labels keyed by the displayed ANM frame.
/// The frame tables follow Chocolate `menues.c::{endanimsounds,endanimvol41}`.
#[must_use]
pub fn finale_sound_labels(map_name: &str, frame: u32) -> &'static [&'static str] {
    match map_name.trim().to_ascii_uppercase().as_str() {
        "E1M6" => match frame {
            1 => &["WIND_AMBIENCE"],
            26 => &["ENDSEQVOL2SND1"],
            36 => &["ENDSEQVOL2SND2"],
            54 => &["THUD"],
            62 => &["ENDSEQVOL2SND3"],
            75 => &["ENDSEQVOL2SND4"],
            81 => &["ENDSEQVOL2SND5"],
            115 => &["ENDSEQVOL2SND6"],
            124 => &["ENDSEQVOL2SND7"],
            _ => &[],
        },
        "E2M11" => match frame {
            1 => &["WIND_REPEAT"],
            98 => &["DUKE_GRUNT"],
            102 => &["THUD", "SQUISHED"],
            124 => &["ENDSEQVOL3SND3"],
            134 => &["ENDSEQVOL3SND2"],
            158 => &["PIPEBOMB_EXPLODE"],
            _ => &[],
        },
        "E4M11" => match frame {
            3 => &["DUKE_UNDERWATER"],
            35 => &["VOL4ENDSND1"],
            _ => &[],
        },
        _ => &[],
    }
}

/// Campaign position stored by Atomic as `ud.volume_number`,
/// `ud.level_number`, and `ud.from_bonus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DukeCampaignProgress {
    /// Zero-based Atomic episode.
    pub volume: u8,
    /// Zero-based Atomic map index within the episode.
    pub level: u8,
    /// Return level after completing the secret map (`0` means none).
    pub from_bonus: u8,
}

/// Result of advancing after an Atomic level exit animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DukeCampaignTransition {
    /// Load the next Atomic map at these zero-based coordinates.
    NextLevel {
        /// Zero-based Atomic episode.
        volume: u8,
        /// Zero-based map index within the episode.
        level: u8,
    },
    /// Atomic `ud.eog` completed the campaign and should return to its menu.
    EndGame,
}

/// Per-level values consumed by Chocolate's `dobonus` intermission.
/// Times are in the native 30 Hz Duke tic domain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DukeLevelBonusStats {
    /// Enemies the player has killed on the current level.
    pub kills: u16,
    /// Chocolate `max_actors_killed` total from eligible prelevel spawn paths.
    pub total_kills: u16,
    /// Secret sectors entered by the player.
    pub secrets_found: u16,
    /// Chocolate `max_secret_rooms` total from prelevel sector scan.
    pub total_secrets: u16,
    /// Elapsed level time in 30 Hz game tics.
    pub elapsed_tics: u32,
    /// CON `definelevelname` par time, or zero when undefined.
    pub par_tics: u32,
    /// CON `definelevelname` designer time, or zero when undefined.
    pub designer_tics: u32,
}

impl DukeCampaignProgress {
    /// Construct a campaign cursor for Atomic's four episodes and eleven slots.
    #[must_use]
    pub const fn new(volume: u8, level: u8) -> Option<Self> {
        if volume < 4 && level < 11 {
            Some(Self {
                volume,
                level,
                from_bonus: 0,
            })
        } else {
            None
        }
    }

    /// Apply the route rules used by Chocolate `player.c` after `fist_incs`.
    pub fn advance(&mut self, player: &mut Player) -> DukeCampaignTransition {
        if player.end_of_game && self.from_bonus == 0 {
            player.button_palette = 0;
            player.secret_level = 0;
            player.end_of_game = false;
            return DukeCampaignTransition::EndGame;
        }
        if player.button_palette != 0 && self.from_bonus == 0 {
            self.from_bonus = self.level.saturating_add(1);
            if (1..12).contains(&player.secret_level) {
                self.level = (player.secret_level - 1) as u8;
            }
        } else if self.from_bonus != 0 {
            self.level = self.from_bonus;
            self.from_bonus = 0;
        } else {
            self.level = if self.level >= 10 { 0 } else { self.level + 1 };
        }
        player.button_palette = 0;
        player.secret_level = 0;
        player.end_of_game = false;
        DukeCampaignTransition::NextLevel {
            volume: self.volume,
            level: self.level,
        }
    }
}

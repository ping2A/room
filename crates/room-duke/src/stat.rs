//! Sprite status list indices used by Duke (`headspritestat[stat]`).
//!
//! Values match Atomic / Chocolate game code (not named in headers — numeric
//! literals in `actors.c` / `game.c`).

/// Free / unused sprites hang off `MAXSTATUS` in the engine lists.
pub const STAT_DEFAULT: i16 = 0;
/// Bad guys / CON actors — `moveactors` walks this list.
pub const STAT_ACTOR: i16 = 1;
/// Sleeping actors — `movefta` wakes these into [`STAT_ACTOR`] when a player is seen.
pub const STAT_DUMMY: i16 = 2;
pub const STAT_PLAYER: i16 = 10;
pub const STAT_PROJECTILE: i16 = 4;
pub const STAT_MISC: i16 = 5;
pub const STAT_EFFECTOR: i16 = 3;
pub const STAT_STANDABLE: i16 = 6;
/// Locators / path nodes (`LOCATORS`).
pub const STAT_LOCATOR: i16 = 7;
/// Activators (`ACTIVATOR` / `ACTIVATORLOCKED`).
pub const STAT_ACTIVATOR: i16 = 8;
pub const STAT_TRANSPORT: i16 = 9;
pub const STAT_FX: i16 = 11;
/// Tagged destructible map sprites in Chocolate `movefallers`.
pub const STAT_FALLER: i16 = 12;
/// `PLAYERONWATER` dummy body sprites moved by Chocolate `movedummyplayers`.
pub const STAT_DUMMYPLAYER: i16 = 13;

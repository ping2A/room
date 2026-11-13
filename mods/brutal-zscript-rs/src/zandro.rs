//! `class ZandroCompat` (`zscript/BD_WCS.zc`): the static functions Brutal
//! Doom's ACS reaches with `ScriptCall("ZandroCompat", …)` when it does not
//! run on Zandronum (`src/zancmpat.bcs`).
//!
//! Each function below is the ZScript one, in the same order, with the
//! ZScript kept as a comment where the Rust reads differently.

use crate::host::Host;
use crate::lumps::LumpTable;

/// ACS `ScriptCall` class name.
pub const CLASS: &str = "ZandroCompat";

/// `LumpOpen` flag: look the name up as a full path.
pub const LUMP_OPEN_FULLPATH: i32 = 1;
/// `LumpGetInfo*`: lump size.
pub const LUMP_INFO_SIZE: i32 = 0;
/// `LumpGetInfo*`: lump name.
pub const LUMP_INFO_NAME: i32 = 1;
/// `LumpRead` types.
pub const LUMP_READ_BYTE: i32 = 0;
/// Unsigned byte.
pub const LUMP_READ_UBYTE: i32 = 1;
/// Signed 16-bit.
pub const LUMP_READ_SHORT: i32 = 2;
/// Unsigned 16-bit.
pub const LUMP_READ_USHORT: i32 = 3;
/// 32-bit.
pub const LUMP_READ_INT: i32 = 4;
/// 32-bit float (Brutal Doom never implemented it).
pub const LUMP_READ_FLOAT: i32 = 5;
/// `MAXPLAYERS`.
pub const MAXPLAYERS: i32 = 8;

/// Every `ZandroCompat` function, in registration order (the callback
/// index is the position in [`Func::ALL`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    /// `int LumpOpen(String name, int start, int flags)`.
    LumpOpen,
    /// `void LumpClose(int lump)`.
    LumpClose,
    /// `int LumpGetInfoInt(int lump, int info)`.
    LumpGetInfoInt,
    /// `String LumpGetInfoStr(int lump, int info)`.
    LumpGetInfoStr,
    /// `String LumpReadString(int lump, int pos, int len)`.
    LumpReadString,
    /// `int LumpRead(int lump, int pos, int type)`.
    LumpRead,
    /// `bool PlayerInGame(int num)`.
    PlayerInGame,
    /// `int GetPlayerTID(int num)`.
    GetPlayerTid,
    /// `void SetPlayerTID(int num, int newtid)`.
    SetPlayerTid,
    /// `bool IsClient()`.
    IsClient,
    /// `int ConsolePlayerNumber()`.
    ConsolePlayerNumber,
    /// `void GetGender(int num)`.
    GetGender,
    /// `bool HasZScript()`.
    HasZScript,
}

impl Func {
    /// Registration order.
    pub const ALL: [Self; 13] = [
        Self::LumpOpen,
        Self::LumpClose,
        Self::LumpGetInfoInt,
        Self::LumpGetInfoStr,
        Self::LumpReadString,
        Self::LumpRead,
        Self::PlayerInGame,
        Self::GetPlayerTid,
        Self::SetPlayerTid,
        Self::IsClient,
        Self::ConsolePlayerNumber,
        Self::GetGender,
        Self::HasZScript,
    ];

    /// ZScript function name (what ACS passes to `ScriptCall`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::LumpOpen => "LumpOpen",
            Self::LumpClose => "LumpClose",
            Self::LumpGetInfoInt => "LumpGetInfoInt",
            Self::LumpGetInfoStr => "LumpGetInfoStr",
            Self::LumpReadString => "LumpReadString",
            Self::LumpRead => "LumpRead",
            Self::PlayerInGame => "PlayerInGame",
            Self::GetPlayerTid => "GetPlayerTID",
            Self::SetPlayerTid => "SetPlayerTID",
            Self::IsClient => "IsClient",
            Self::ConsolePlayerNumber => "ConsolePlayerNumber",
            Self::GetGender => "GetGender",
            Self::HasZScript => "HasZScript",
        }
    }

    /// Function for a callback index.
    #[must_use]
    pub fn from_index(index: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }
}

/// Arguments of one `ScriptCall` (after the class and function names).
pub trait Args {
    /// Integer argument `i` (0 when ACS passed fewer).
    fn int(&self, i: usize) -> i32;
    /// String argument `i` (empty when it is not a string).
    fn string(&self, i: usize) -> String;
}

/// What a function returns to ACS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ret {
    /// `int` / `bool` / `void` (0).
    Int(i32),
    /// `String` (ACS receives a string handle).
    Str(String),
}

impl From<bool> for Ret {
    fn from(b: bool) -> Self {
        Self::Int(i32::from(b))
    }
}

/// Run `func` with `args`.
pub fn call(host: &mut dyn Host, lumps: &mut LumpTable, func: Func, args: &dyn Args) -> Ret {
    match func {
        Func::LumpOpen => Ret::Int(lump_open(
            host,
            lumps,
            &args.string(0),
            args.int(1),
            args.int(2),
        )),
        Func::LumpClose => {
            lump_close(lumps, args.int(0));
            Ret::Int(0)
        }
        Func::LumpGetInfoInt => Ret::Int(lump_get_info_int(host, lumps, args.int(0), args.int(1))),
        Func::LumpGetInfoStr => Ret::Str(lump_get_info_str(host, lumps, args.int(0), args.int(1))),
        Func::LumpReadString => Ret::Str(lump_read_string(
            host,
            lumps,
            args.int(0),
            args.int(1),
            args.int(2),
        )),
        Func::LumpRead => Ret::Int(lump_read(host, lumps, args.int(0), args.int(1), args.int(2))),
        Func::PlayerInGame => player_in_game(host, args.int(0)).into(),
        Func::GetPlayerTid => Ret::Int(get_player_tid(host, args.int(0))),
        Func::SetPlayerTid => {
            set_player_tid(host, args.int(0), args.int(1));
            Ret::Int(0)
        }
        Func::IsClient => is_client().into(),
        Func::ConsolePlayerNumber => Ret::Int(console_player_number()),
        Func::GetGender => {
            get_gender(host, args.int(0));
            Ret::Int(0)
        }
        Func::HasZScript => true.into(),
    }
}

/// `LumpOpen`: find the lump; the first open reads it into the table, later
/// opens only count.
pub fn lump_open(
    host: &mut dyn Host,
    lumps: &mut LumpTable,
    name: &str,
    start: i32,
    flags: i32,
) -> i32 {
    // if (flags & LUMP_OPEN_FULLPATH) lump = Wads.CheckNumForFullName(name);
    // else if (start == -2) lump = Wads.CheckNumForName(name, Wads.GLOBALNAMESPACE);
    // else lump = Wads.FindLump(name, start + 1);
    //
    // Room's lump host has one lookup; `FindLump` from `start + 1` is "the
    // match, if it lies after `start`".
    let found = host.lump_find(name);
    let lump = if flags & LUMP_OPEN_FULLPATH != 0 || start == -2 || found > start {
        found
    } else {
        -1
    };
    if lump == -1 {
        return -1;
    }
    if !lumps.is_open(lump) {
        let data = host.lump_read(lump);
        lumps.insert(lump, name, data);
        return lump;
    }
    lumps.set_ref_count(lump, lumps.ref_count(lump) + 1);
    lump
}

/// `LumpClose`: drop one reference; the last one forgets the lump.
pub fn lump_close(lumps: &mut LumpTable, lump: i32) {
    // The count is written first, even for a lump that was never opened
    // (and for -1, which `ZC_LumpOpen` hands back when nothing was found).
    lumps.set_ref_count(lump, lumps.ref_count(lump) - 1);
    if lump == -1 || !lumps.is_open(lump) || lumps.ref_count(lump) > 0 {
        return;
    }
    lumps.remove(lump);
}

/// `LumpGetInfoInt`: only `LUMP_INFO_SIZE` is an int.
pub fn lump_get_info_int(host: &mut dyn Host, lumps: &LumpTable, lump: i32, info: i32) -> i32 {
    if lump == -1 {
        return 0;
    }
    match info {
        // m.lumpBuf.get(lump).length()
        LUMP_INFO_SIZE => i32::try_from(lumps.data(lump).len()).unwrap_or(i32::MAX),
        LUMP_INFO_NAME => {
            host.print(
                "LumpGetInfo internal error: LUMP_INFO_NAME requested, but GetInfoInt called",
            );
            0
        }
        _ => {
            host.print(&format!("LumpGetInfo: unknown info type {info}"));
            0
        }
    }
}

/// `LumpGetInfoStr`: only `LUMP_INFO_NAME` is a string.
pub fn lump_get_info_str(host: &mut dyn Host, lumps: &LumpTable, lump: i32, info: i32) -> String {
    if lump == -1 {
        return String::new();
    }
    match info {
        LUMP_INFO_SIZE => {
            host.print(
                "LumpGetInfo internal error: LUMP_INFO_SIZE requested, but GetInfoStr called",
            );
            String::new()
        }
        LUMP_INFO_NAME => lumps.name(lump).to_owned(),
        _ => {
            host.print(&format!("LumpGetInfo: unknown info type {info}"));
            String::new()
        }
    }
}

/// `LumpReadString`: `len` bytes from `pos` (`String.Mid`).
pub fn lump_read_string(
    host: &mut dyn Host,
    lumps: &LumpTable,
    lump: i32,
    pos: i32,
    len: i32,
) -> String {
    if lump == -1 || !lumps.is_open(lump) {
        host.print(&format!(
            "LumpReadString: Attempted read on non-existent lump handle ({lump})!"
        ));
        return String::new();
    }
    let data = lumps.data(lump);
    let start = usize::try_from(pos).unwrap_or(0).min(data.len());
    let end = start
        .saturating_add(usize::try_from(len).unwrap_or(0))
        .min(data.len());
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

/// `LumpRead`: one little-endian value at `pos`.
pub fn lump_read(host: &mut dyn Host, lumps: &LumpTable, lump: i32, pos: i32, kind: i32) -> i32 {
    if lump == -1 || !lumps.is_open(lump) {
        host.print("LumpRead: Attempted read on non-existent lump handle!");
        return 0;
    }
    let data = lumps.data(lump);
    // `String.CharCodeAt` past the end is 0.
    let byte = |at: i32| -> i32 {
        usize::try_from(at)
            .ok()
            .and_then(|i| data.get(i))
            .map_or(0, |&b| i32::from(b))
    };
    match kind {
        LUMP_READ_UBYTE => byte(pos),
        LUMP_READ_BYTE => {
            // Brutal Doom negates values above 127 instead of
            // sign-extending them (200 reads as -200, not -56); kept as is.
            let temp = byte(pos);
            if temp > 127 { -temp } else { temp }
        }
        LUMP_READ_USHORT => byte(pos) | (byte(pos + 1) << 8),
        LUMP_READ_SHORT => {
            // Same negation as LUMP_READ_BYTE.
            let temp = byte(pos) | (byte(pos + 1) << 8);
            if temp > 32767 { -temp } else { temp }
        }
        LUMP_READ_INT => {
            byte(pos) | (byte(pos + 1) << 8) | (byte(pos + 2) << 16) | (byte(pos + 3) << 24)
        }
        LUMP_READ_FLOAT => {
            host.print("LumpRead: LUMP_READ_FLOAT not implemented!");
            0
        }
        _ => {
            host.print(&format!("LumpRead: unknown read type {kind}!"));
            0
        }
    }
}

/// `PlayerInGame`: `!!players[num].mo`.
pub fn player_in_game(host: &dyn Host, num: i32) -> bool {
    host.player(num).is_some()
}

/// `GetPlayerTID`.
pub fn get_player_tid(host: &dyn Host, num: i32) -> i32 {
    if !(0..MAXPLAYERS).contains(&num) {
        return 0;
    }
    host.player(num).map_or(0, |mo| host.tid(mo))
}

/// `SetPlayerTID`: `players[num].mo.ChangeTID(newtid)`.
pub fn set_player_tid(host: &mut dyn Host, num: i32, new_tid: i32) {
    if !(0..MAXPLAYERS).contains(&num) {
        return;
    }
    if let Some(mo) = host.player(num) {
        host.change_tid(mo, new_tid);
    }
}

/// `IsClient`: "GZDoom doesn't need the distinction" — `consoleplayer >= 0`.
#[must_use]
pub fn is_client() -> bool {
    console_player_number() >= 0
}

/// `ConsolePlayerNumber`: GZDoom's `consoleplayer`. Room runs one playsim
/// for every seat, and its ZScript host reports 0 here too.
#[must_use]
pub fn console_player_number() -> i32 {
    0
}

/// `GetGender`: pick the player's voice and the face token the status bar
/// draws — female when `bd_Gender` is 1 or the player's gender is female.
pub fn get_gender(host: &mut dyn Host, num: i32) {
    if !(0..MAXPLAYERS).contains(&num) {
        return;
    }
    // `plr.mo.…` on a player without a body aborts the ZScript call before
    // anything changes; the port just returns.
    let Some(mo) = host.player(num) else {
        return;
    };
    let gender = host.player_gender(num);
    if host.player_cvar(num, "bd_Gender") == 1 || gender == 1 {
        host.set_sound_class(mo, "fplayer");
        host.take(mo, "GenderMale", 1);
        host.give(mo, "GenderFemale", 1);
    } else {
        host.set_sound_class(mo, "player");
        host.take(mo, "GenderFemale", 1);
        host.give(mo, "GenderMale", 1);
    }
}

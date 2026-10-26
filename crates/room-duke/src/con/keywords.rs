//! CON 1.5 keyword table — indices match Chocolate `keyw[]` / `GAMEDEF.C`.

/// Number of keywords in Atomic / 1.5 CON (`NUMKEYWORDS`).
pub const NUM_KEYWORDS: usize = 112;

/// Vanilla keyword strings in opcode order (case-sensitive match as stored;
/// the lexer lowercases before lookup).
pub const KEYWORDS: [&str; NUM_KEYWORDS] = [
    "definelevelname",   // 0
    "actor",             // 1
    "addammo",           // 2
    "ifrnd",             // 3
    "enda",              // 4
    "ifcansee",          // 5
    "ifhitweapon",       // 6
    "action",            // 7
    "ifpdistl",          // 8
    "ifpdistg",          // 9
    "else",              // 10
    "strength",          // 11
    "break",             // 12
    "shoot",             // 13
    "palfrom",           // 14
    "sound",             // 15
    "fall",              // 16
    "state",             // 17
    "ends",              // 18
    "define",            // 19
    "//",                // 20
    "ifai",              // 21
    "killit",            // 22
    "addweapon",         // 23
    "ai",                // 24
    "addphealth",        // 25
    "ifdead",            // 26
    "ifsquished",        // 27
    "sizeto",            // 28
    "{",                 // 29
    "}",                 // 30
    "spawn",             // 31
    "move",              // 32
    "ifwasweapon",       // 33
    "ifaction",          // 34
    "ifactioncount",     // 35
    "resetactioncount",  // 36
    "debris",            // 37
    "pstomp",            // 38
    "/*",                // 39
    "cstat",             // 40
    "ifmove",            // 41
    "resetplayer",       // 42
    "ifonwater",         // 43
    "ifinwater",         // 44
    "ifcanshoottarget",  // 45
    "ifcount",           // 46
    "resetcount",        // 47
    "addinventory",      // 48
    "ifactornotstayput", // 49
    "hitradius",         // 50
    "ifp",               // 51
    "count",             // 52
    "ifactor",           // 53
    "music",             // 54
    "include",           // 55
    "ifstrength",        // 56
    "definesound",       // 57
    "guts",              // 58
    "ifspawnedby",       // 59
    "gamestartup",       // 60
    "wackplayer",        // 61
    "ifgapzl",           // 62
    "ifhitspace",        // 63
    "ifoutside",         // 64
    "ifmultiplayer",     // 65
    "operate",           // 66
    "ifinspace",         // 67
    "debug",             // 68
    "endofgame",         // 69
    "ifbulletnear",      // 70
    "ifrespawn",         // 71
    "iffloordistl",      // 72
    "ifceilingdistl",    // 73
    "spritepal",         // 74
    "ifpinventory",      // 75
    "betaname",          // 76
    "cactor",            // 77
    "ifphealthl",        // 78
    "definequote",       // 79
    "quote",             // 80
    "ifinouterspace",    // 81
    "ifnotmoving",       // 82
    "respawnhitag",      // 83
    "tip",               // 84
    "ifspritepal",       // 85
    "money",             // 86
    "soundonce",         // 87
    "addkills",          // 88
    "stopsound",         // 89
    "ifawayfromwall",    // 90
    "ifcanseetarget",    // 91
    "globalsound",       // 92
    "lotsofglass",       // 93
    "ifgotweaponce",     // 94
    "getlastpal",        // 95
    "pkick",             // 96
    "mikesnd",           // 97
    "useractor",         // 98
    "sizeat",            // 99
    "addstrength",       // 100
    "cstator",           // 101
    "mail",              // 102
    "paper",             // 103
    "tossweapon",        // 104
    "sleeptime",         // 105
    "nullop",            // 106
    "definevolumename",  // 107
    "defineskillname",   // 108
    "ifnosounds",        // 109
    "clipdist",          // 110
    "ifangdiffl",        // 111
];

/// Look up a token in [`KEYWORDS`] with Chocolate `strcmp` semantics (case-sensitive
/// against the lowercase table — CON keywords must be lowercase).
#[must_use]
pub fn keyword_index(tok: &str) -> Option<i32> {
    KEYWORDS.iter().position(|&k| k == tok).map(|i| i as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_is_19() {
        assert_eq!(keyword_index("define"), Some(19));
    }

    #[test]
    fn last_keyword_is_111() {
        assert_eq!(keyword_index("ifangdiffl"), Some(111));
        assert_eq!(KEYWORDS.len(), NUM_KEYWORDS);
    }
}

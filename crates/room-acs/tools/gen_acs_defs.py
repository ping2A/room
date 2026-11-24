#!/usr/bin/env python3
"""Generate `src/compile/defs.rs` (ACS compiler built-ins) from the vendored
UZDoom sources: line specials (`actionspecials.h`), CallFunc functions and
`APROP_` / `LEVELINFO_` / `PLAYERINFO_` / `ACTOR_` / `SOUND_` constants
(`p_acs.cpp`), `AAPTR_` / `PTROP_` (`actor.h`, `actorptrselect.h`), plus the usual `zdefs.acs`
constants written out below.

    python3 crates/room-acs/tools/gen_acs_defs.py
"""
import pathlib, re

ROOT = pathlib.Path(__file__).resolve().parents[3]
UZ = ROOT / "third_party" / "UZDoom" / "src"
OUT = ROOT / "crates" / "room-acs" / "src" / "compile" / "defs.rs"

specials = []
for m in re.finditer(r"DEFINE_SPECIAL\((\w+),\s*(\d+),\s*(-?\d+),\s*(-?\d+),\s*(-?\d+)\)", (UZ / "playsim/actionspecials.h").read_text()):
    name, num, lo, hi, _ = m.groups()
    specials.append((name, int(num), int(lo), int(hi)))

def enum_values(text, prefix):
    """Entries `PREFIX_Name [= value]` of every C enum mentioning the prefix."""
    out = {}
    for block in re.finditer(r"enum\s*\w*\s*\{(.*?)\}", text, re.S):
        body = re.sub(r"//[^\n]*", "", block.group(1))
        body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
        if prefix not in body:
            continue
        cur = -1
        for ent in body.split(","):
            ent = ent.strip()
            if not ent:
                continue
            mm = re.match(r"(\w+)\s*(?:=\s*(.+))?$", ent, re.S)
            if not mm:
                continue
            name, val = mm.group(1), mm.group(2)
            if val is not None:
                val = val.strip()
                try:
                    cur = int(eval(val.replace("u", "").replace("U", ""), {}, {k: v for k, v in out.items()}))
                except Exception:
                    cur = out.get(val, cur + 1)
            else:
                cur += 1
            if name.startswith(prefix):
                out[name] = cur
    return out

acs_cpp = (UZ / "playsim/p_acs.cpp").read_text(errors="replace")
actor_h = (UZ / "playsim/actor.h").read_text(errors="replace") + (UZ / "playsim/actorptrselect.h").read_text(errors="replace")
acsf = enum_values(acs_cpp, "ACSF_")
consts = {}
for prefix in ["APROP_", "LEVELINFO_", "PLAYERINFO_", "ACTOR_", "SOUND_"]:
    for k, v in enum_values(acs_cpp, prefix).items():
        consts[k.upper()] = v
for prefix in ["AAPTR_", "PTROP_"]:
    for k, v in enum_values(actor_h, prefix).items():
        consts[k.upper()] = v

HAND = """
TRUE 1 FALSE 0 ON 1 OFF 0 YES 1 NO 0
LINE_FRONT 0 LINE_BACK 1 SIDE_FRONT 0 SIDE_BACK 1
TEXTURE_TOP 0 TEXTURE_MIDDLE 1 TEXTURE_BOTTOM 2
TEXFLAG_TOP 1 TEXFLAG_MIDDLE 2 TEXFLAG_BOTTOM 4 TEXFLAG_ADDOFFSET 8
GAME_SINGLE_PLAYER 0 GAME_NET_COOPERATIVE 1 GAME_NET_DEATHMATCH 2 GAME_TITLE_MAP 3
CLASS_FIGHTER 0 CLASS_CLERIC 1 CLASS_MAGE 2
SKILL_VERY_EASY 0 SKILL_EASY 1 SKILL_NORMAL 2 SKILL_HARD 3 SKILL_VERY_HARD 4
BLOCK_NOTHING 0 BLOCK_CREATURES 1 BLOCK_EVERYTHING 2 BLOCK_RAILING 3 BLOCK_PLAYERS 4
SCROLL 0 CARRY 1 SCROLL_AND_CARRY 2
CR_UNTRANSLATED -1 CR_BRICK 0 CR_TAN 1 CR_GRAY 2 CR_GREY 2 CR_GREEN 3 CR_BROWN 4 CR_GOLD 5 CR_RED 6
CR_BLUE 7 CR_ORANGE 8 CR_WHITE 9 CR_YELLOW 10 CR_BLACK 12 CR_LIGHTBLUE 13 CR_CREAM 14 CR_OLIVE 15
CR_DARKGREEN 16 CR_DARKRED 17 CR_DARKBROWN 18 CR_PURPLE 19 CR_DARKGRAY 20 CR_DARKGREY 20 CR_CYAN 21
CR_ICE 22 CR_FIRE 23 CR_SAPPHIRE 24 CR_TEAL 25
HUDMSG_PLAIN 0 HUDMSG_FADEOUT 1 HUDMSG_TYPEON 2 HUDMSG_FADEINOUT 3
HUDMSG_LOG 0x80000000 HUDMSG_COLORSTRING 0x40000000 HUDMSG_ADDBLEND 0x20000000 HUDMSG_ALPHA 0x10000000
HUDMSG_NOWRAP 0x08000000 HUDMSG_LAYER_OVERHUD 0 HUDMSG_LAYER_UNDERHUD 0x1000 HUDMSG_LAYER_OVERMAP 0x2000
HUDMSG_NOTWITH3DVIEW 0x10000 HUDMSG_NOTWITHFULLMAP 0x20000 HUDMSG_NOTWITHOVERLAYMAP 0x40000
CHAN_AUTO 0 CHAN_WEAPON 1 CHAN_VOICE 2 CHAN_ITEM 3 CHAN_BODY 4 CHAN_5 5 CHAN_6 6 CHAN_7 7
CHAN_LISTENERZ 8 CHAN_MAYBE_LOCAL 16 CHAN_UI 32 CHAN_NOPAUSE 64 CHAN_LOOP 256 CHAN_NOSTOP 4096 CHAN_OVERLAP 8192
ATTN_NONE 0 ATTN_NORM 65536 ATTN_IDLE 65601 ATTN_STATIC 196608
STYLE_NONE 0 STYLE_NORMAL 1 STYLE_FUZZY 2 STYLE_SOULTRANS 3 STYLE_OPTFUZZY 4 STYLE_STENCIL 5
STYLE_ADDSTENCIL 6 STYLE_ADDSHADED 7 STYLE_TRANSLUCENT 64 STYLE_ADD 65 STYLE_SHADED 66
STYLE_TRANSLUCENTSTENCIL 67 STYLE_SHADOW 68 STYLE_SUBTRACT 69
INPUT_OLDBUTTONS 0 INPUT_BUTTONS 1 INPUT_PITCH 2 INPUT_YAW 3 INPUT_ROLL 4 INPUT_FORWARDMOVE 5
INPUT_SIDEMOVE 6 INPUT_UPMOVE 7 MODINPUT_OLDBUTTONS 8 MODINPUT_BUTTONS 9 MODINPUT_PITCH 10 MODINPUT_YAW 11
MODINPUT_ROLL 12 MODINPUT_FORWARDMOVE 13 MODINPUT_SIDEMOVE 14 MODINPUT_UPMOVE 15
BT_ATTACK 1 BT_USE 2 BT_JUMP 4 BT_CROUCH 8 BT_TURN180 16 BT_ALTATTACK 32 BT_RELOAD 64 BT_ZOOM 128
BT_SPEED 256 BT_STRAFE 512 BT_MOVERIGHT 1024 BT_MOVELEFT 2048 BT_BACK 4096 BT_FORWARD 8192 BT_RIGHT 16384
BT_LEFT 32768 BT_LOOKUP 65536 BT_LOOKDOWN 131072 BT_MOVEUP 262144 BT_MOVEDOWN 524288 BT_SHOWSCORES 1048576
BT_USER1 2097152 BT_USER2 4194304 BT_USER3 8388608 BT_USER4 16777216
PROP_FROZEN 0 PROP_NOTARGET 1 PROP_INSTANTWEAPONSWITCH 2 PROP_FLY 3 PROP_TOTALLYFROZEN 4
PROP_INVULNERABILITY 5 PROP_STRENGTH 6 PROP_INVISIBILITY 7 PROP_RADIATIONSUIT 8 PROP_ALLMAP 9
PROP_INFRARED 10 PROP_WEAPONLEVEL2 11 PROP_FLIGHT 12 PROP_SPEED 15 PROP_BUDDHA 16
DAMAGE_PLAYERS 1 DAMAGE_NONPLAYERS 2 DAMAGE_IN_AIR 4 DAMAGE_SUBCLASSES_PROTECT 8
PRINTNAME_LEVELNAME -1 PRINTNAME_LEVEL -2 PRINTNAME_SKILL -3 PRINTNAME_NEXTLEVEL -4 PRINTNAME_NEXTSECRET -5
NOT_BOTTOM 1 NOT_MIDDLE 2 NOT_TOP 4 NOT_FLOOR 8 NOT_CEILING 16
TEAM_BLUE 0 TEAM_RED 1 NO_TEAM 2
FHF_NORANDOMPUFFZ 1 FHF_NOIMPACTDECAL 2
SDF_ABSANGLE 1 SDF_PERMANENT 2
CSF_NOFAKEFLOORS 1 CSF_NOBLOCKALL 2
PICKAF_FORCETID 1 PICKAF_RETURNTID 2
MRF_OLDEFFECTS 0 MRF_ADDSTAMINA 1 MRF_FULLHEALTH 2 MRF_UNDOBYTOMEOFPOWER 4 MRF_UNDOBYCHAOSDEVICE 8
MRF_FAILNOTELEFRAG 16 MRF_FAILNOLAUGH 32 MRF_WHENINVULNERABLE 64 MRF_LOSEACTUALWEAPON 128
MRF_NEWTIDBEHAVIOUR 256 MRF_UNDOBYDEATH 512 MRF_UNDOBYDEATHFORCED 1024 MRF_UNDOBYDEATHSAVES 2048
MRF_UNDOALWAYS 4096 MRF_TRANSFERTRANSLATION 8192
SECSEQ_FLOOR 1 SECSEQ_CEILING 2 SECSEQ_FULLHEIGHT 3 SECSEQ_INTERIOR 4
"""
for name, val in re.findall(r"([A-Z][A-Z0-9_]*)\s+(-?(?:0x[0-9A-Fa-f]+|\d+))", HAND):
    v = int(val, 0)
    if v >= 2**31:
        v -= 2**32
    consts[name] = v

def rs_str(s):
    return '"' + s + '"'

lines = []
lines.append("//! ACS compiler built-ins: line specials, CallFunc functions and constants.")
lines.append("//!")
lines.append("//! Generated by `crates/room-acs/tools/gen_acs_defs.py` from the vendored UZDoom")
lines.append("//! sources (`actionspecials.h`, `p_acs.cpp`, `actor.h`); do not edit by hand.")
lines.append("")
lines.append("/// Line specials: (name, number, min args, max args); `-1` = any count up to 5.")
lines.append("pub(crate) const SPECIALS: &[(&str, i32, i8, i8)] = &[")
for name, num, lo, hi in specials:
    lines.append(f"    ({rs_str(name)}, {num}, {lo}, {hi}),")
lines.append("];")
lines.append("")
lines.append("/// `PCD_CALLFUNC` functions (GZDoom `EACSFunctions`): (name, index).")
lines.append("pub(crate) const CALLFUNCS: &[(&str, i32)] = &[")
for name, v in acsf.items():
    lines.append(f"    ({rs_str(name[5:])}, {v}),")
lines.append("];")
lines.append("")
lines.append("/// Named constants (`zdefs.acs`), upper case.")
lines.append("pub(crate) const CONSTANTS: &[(&str, i32)] = &[")
for name in sorted(consts):
    v = consts[name]
    if not (-2**31 <= v < 2**31):
        continue
    lines.append(f"    ({rs_str(name)}, {v}),")
lines.append("];")
OUT.write_text("\n".join(lines) + "\n")
print(f"{len(specials)} specials, {len(acsf)} callfuncs, {len(consts)} constants -> {OUT}")

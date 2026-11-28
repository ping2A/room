//! Extra actor flags beyond the legacy `u64` (`docs/36` UZ1).
//!
//! The 64 legacy `FLAG_*` bits stay the playsim source of truth for Doom
//! demos. Every UZDoom `DEFINE_FLAG` name is stored here so DECORATE can
//! set it, and a few Doom-relevant names also change behaviour.

/// Words in [`ActorFlags`] (384 bits).
pub const FLAG_WORDS: usize = 6;

/// Bitset of UZDoom flag names. Index 0 is the first `DEFINE_FLAG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ActorFlags([u64; FLAG_WORDS]);

/// `DEFINE_FLAG` names from UZDoom `thingdef_data.cpp` (`67bdb4b7`), in order.
pub static UZDOOM_FLAGS: &[&str] = &[
    // Bit 0 is unused: the generator once took the table's `name` header
    // for a flag, and saves since then store bits by these indices, so the
    // slot stays reserved (it cannot match any `+FLAG`).
    RESERVED_FLAG_SLOT,
    "INCHASE",
    "UNMORPHED",
    "FLY",
    "ONMOBJ",
    "ARGSDEFINED",
    "NOSIGHTCHECK",
    "CRASHED",
    "WARNBOT",
    "HUNTPLAYERS",
    "NOHATEPLAYERS",
    "SCROLLMOVE",
    "VFRICTION",
    "BOSSSPAWNED",
    "AVOIDINGDROPOFF",
    "CHASEGOAL",
    "INCONVERSATION",
    "ARMED",
    "FALLING",
    "LINEDONE",
    "SHATTERING",
    "KILLED",
    "BOSSCUBE",
    "INTRYMOVE",
    "HANDLENODELAY",
    "FLYCHEAT",
    "RESPAWNINVUL",
    "PICKUP",
    "SPECIAL",
    "SOLID",
    "SHOOTABLE",
    "AMBUSH",
    "JUSTHIT",
    "JUSTATTACKED",
    "SPAWNCEILING",
    "NOGRAVITY",
    "DROPOFF",
    "NOCLIP",
    "FLOAT",
    "TELEPORT",
    "MISSILE",
    "DROPPED",
    "SHADOW",
    "NOBLOOD",
    "CORPSE",
    "INFLOAT",
    "COUNTKILL",
    "COUNTITEM",
    "SKULLFLY",
    "NOTDMATCH",
    "SPAWNSOUNDSOURCE",
    "FRIENDLY",
    "NOLIFTDROP",
    "STEALTH",
    "ICECORPSE",
    "DONTREFLECT",
    "WINDTHRUST",
    "DONTSEEKINVISIBLE",
    "BLASTED",
    "FLOORCLIP",
    "SPAWNFLOAT",
    "NOTELEPORT",
    "PUSHABLE",
    "CANNOTPUSH",
    "THRUGHOST",
    "BOSS",
    "DONTTRANSLATE",
    "TELESTOMP",
    "FLOATBOB",
    "THRUACTORS",
    "CANTLEAVEFLOORPIC",
    "NONSHOOTABLE",
    "INVULNERABLE",
    "DORMANT",
    "SEEKERMISSILE",
    "REFLECTIVE",
    "FLOORHUGGER",
    "CEILINGHUGGER",
    "NORADIUSDMG",
    "GHOST",
    "SPECIALFLOORCLIP",
    "ALWAYSPUFF",
    "DONTSPLASH",
    "DONTOVERLAP",
    "DONTMORPH",
    "DONTSQUASH",
    "EXPLOCOUNT",
    "FULLVOLACTIVE",
    "ISMONSTER",
    "SKYEXPLODE",
    "STAYMORPHED",
    "DONTBLAST",
    "CANBLAST",
    "NOTARGET",
    "DONTGIB",
    "NOBLOCKMONST",
    "FULLVOLDEATH",
    "AVOIDMELEE",
    "SCREENSEEKER",
    "FOILINVUL",
    "NOTELEOTHER",
    "BLOODLESSIMPACT",
    "NOEXPLODEFLOOR",
    "PUFFONACTORS",
    "QUICKTORETALIATE",
    "NOICEDEATH",
    "RANDOMIZE",
    "FIXMAPTHINGPOS",
    "ACTLIKEBRIDGE",
    "STRIFEDAMAGE",
    "CANUSEWALLS",
    "FORCERADIUSDMG",
    "DONTFALL",
    "SEESDAGGERS",
    "INCOMBAT",
    "LOOKALLAROUND",
    "STANDSTILL",
    "SPECTRAL",
    "NOSPLASHALERT",
    "SYNCHRONIZED",
    "NOTARGETSWITCH",
    "DONTHARMCLASS",
    "SHIELDREFLECT",
    "DEFLECT",
    "ALLOWPARTICLES",
    "EXTREMEDEATH",
    "NOEXTREMEDEATH",
    "FRIGHTENED",
    "NOSKIN",
    "BOSSDEATH",
    "DONTDRAIN",
    "GETOWNER",
    "NODROPOFF",
    "NOFORWARDFALL",
    "COUNTSECRET",
    "NODAMAGE",
    "BLOODSPLATTER",
    "OLDRADIUSDMG",
    "DEHEXPLOSION",
    "PIERCEARMOR",
    "NOBLOODDECALS",
    "USESPECIAL",
    "NOPAIN",
    "ALWAYSFAST",
    "NEVERFAST",
    "ALWAYSRESPAWN",
    "NEVERRESPAWN",
    "DONTRIP",
    "NOINFIGHTING",
    "NOINTERACTION",
    "NOTIMEFREEZE",
    "PUFFGETSOWNER",
    "SPECIALFIREDAMAGE",
    "SUMMONEDMONSTER",
    "NOVERTICALMELEERANGE",
    "BRIGHT",
    "CANTSEEK",
    "PAINLESS",
    "MOVEWITHSECTOR",
    "NOBOSSRIP",
    "THRUSPECIES",
    "MTHRUSPECIES",
    "FORCEPAIN",
    "NOFEAR",
    "BUMPSPECIAL",
    "DONTHARMSPECIES",
    "STEPMISSILE",
    "NOTELEFRAG",
    "TOUCHY",
    "CANJUMP",
    "JUMPDOWN",
    "VULNERABLE",
    "NOTRIGGER",
    "ADDITIVEPOISONDAMAGE",
    "ADDITIVEPOISONDURATION",
    "BLOCKEDBYSOLIDACTORS",
    "NOMENU",
    "SEEINVISIBLE",
    "DONTCORPSE",
    "DOHARMSPECIES",
    "POISONALWAYS",
    "NOTAUTOAIMED",
    "NOTONAUTOMAP",
    "RELATIVETOFLOOR",
    "NEVERTARGET",
    "NOTELESTOMP",
    "ALWAYSTELEFRAG",
    "WEAPONSPAWN",
    "HARMFRIENDS",
    "BUDDHA",
    "FOILBUDDHA",
    "DONTTHRUST",
    "ALLOWPAIN",
    "CAUSEPAIN",
    "THRUREFLECT",
    "MIRRORREFLECT",
    "AIMREFLECT",
    "HITTARGET",
    "HITMASTER",
    "HITTRACER",
    "NODECAL",
    "FORCEDECAL",
    "LAXTELEFRAGDMG",
    "ICESHATTER",
    "ALLOWTHRUFLAGS",
    "USEKILLSCRIPTS",
    "NOKILLSCRIPTS",
    "SPRITEANGLE",
    "SMASHABLE",
    "NOSHIELDREFLECT",
    "FORCEZERORADIUSDMG",
    "NOINFIGHTSPECIES",
    "FORCEINFIGHTING",
    "FRIGHTENING",
    "BLOCKASPLAYER",
    "DONTFACETALKER",
    "HITOWNER",
    "NOFRICTION",
    "NOFRICTIONBOUNCE",
    "RETARGETAFTERSLAM",
    "STOPRAILS",
    "FALLDAMAGE",
    "MINVISIBLE",
    "MVISBLOCKED",
    "ABSVIEWANGLES",
    "ALLOWTHRUBITS",
    "FULLVOLSEE",
    "E1M8BOSS",
    "E2M8BOSS",
    "E3M8BOSS",
    "E4M8BOSS",
    "E4M6BOSS",
    "MAP07BOSS1",
    "MAP07BOSS2",
    "AVOIDHAZARDS",
    "STAYONLIFT",
    "DONTFOLLOWPLAYERS",
    "SEEFRIENDLYMONSTERS",
    "CROSSLINECHECK",
    "MASTERNOSEE",
    "ADDLIGHTLEVEL",
    "ONLYSLAMSOLID",
    "SHADOWAIM",
    "DOSHADOWBLOCK",
    "SHADOWBLOCK",
    "SHADOWAIMVERT",
    "DECOUPLEDANIMATIONS",
    "NOSECTORDAMAGE",
    "FORCESECTORDAMAGE",
    "NOAUTOOFFSKULLFLY",
    "VISIBILITYPULSE",
    "INVISIBLE",
    "FORCEYBILLBOARD",
    "FORCEXYBILLBOARD",
    "ROLLSPRITE",
    "FLATSPRITE",
    "WALLSPRITE",
    "DONTFLIP",
    "ROLLCENTER",
    "MASKROTATION",
    "ABSMASKANGLE",
    "ABSMASKPITCH",
    "XFLIP",
    "YFLIP",
    "INTERPOLATEANGLES",
    "DONTINTERPOLATE",
    "SPRITEFLIP",
    "ZDOOMTRANS",
    "CASTSPRITESHADOW",
    "NOSPRITESHADOW",
    "INVISIBLEINMIRRORS",
    "ONLYVISIBLEINMIRRORS",
    "BILLBOARDFACECAMERA",
    "BILLBOARDNOFACECAMERA",
    "FLIPSPRITEOFFSETX",
    "FLIPSPRITEOFFSETY",
    "CAMFOLLOWSPLAYER",
    "ISOMETRICSPRITES",
    "SQUAREPIXELS",
    "STRETCHPIXELS",
    "LIGHTMULTALPHA",
    "ANGLEDROLL",
    "INTERPOLATESCALE",
    "INTERPOLATEALPHA",
    "NODYNAMICLIGHTING",
    // `DEFINE_FLAG2` names the first scanner skipped. Bounce names that already
    // have a legacy `u64` bit are stored here too so `is_known_flag` accepts them.
    "MBFBOUNCER",
    "RIPPER",
    "SLIDESONWALLS",
    "CANPASS",
    "NODAMAGETHRUST",
    "ACTIVATEIMPACT",
    "CANPUSHWALLS",
    "ACTIVATEMCROSS",
    "ACTIVATEPCROSS",
    "ROCKETTRAIL",
    "GRENADETRAIL",
    "BOUNCEONWALLS",
    "BOUNCEONFLOORS",
    "BOUNCEONCEILINGS",
    "ALLOWBOUNCEONACTORS",
    "BOUNCEAUTOOFF",
    "BOUNCELIKEHERETIC",
    "CANBOUNCEWATER",
    "NOWALLBOUNCESND",
    "NOBOUNCESOUND",
    "BOUNCEONACTORS",
    "EXPLODEONWATER",
    "BOUNCEAUTOOFFFLOORONLY",
    "USEBOUNCESTATE",
    "DONTBOUNCEONSHOOTABLES",
    "BOUNCEONUNRIPPABLES",
    "DONTBOUNCEONSKY",
    "KEEPBOUNCEANGLE",
    "BOUNCEMODIFIESPITCH",
    "NOSAVEGAME",
    // Room-only (appended so saved bit indices stay stable): the class's
    // `Scale` also scales its collision box. Set by the Build/Duke converter,
    // whose generated actors are sized that way; GZDoom content never sets it.
    "ROOMSCALEDCOLLISION",
];

/// Uppercase flag name → bit; the first entry wins for a repeated name, as
/// the old scan did.
fn flag_index_map() -> &'static std::collections::HashMap<String, u16> {
    static MAP: std::sync::OnceLock<std::collections::HashMap<String, u16>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = std::collections::HashMap::with_capacity(UZDOOM_FLAGS.len());
        for (i, n) in UZDOOM_FLAGS.iter().enumerate() {
            if n.starts_with('<') {
                continue;
            }
            if let Ok(i) = u16::try_from(i) {
                map.entry(n.to_ascii_uppercase()).or_insert(i);
            }
        }
        map
    })
}

/// Placeholder for [`UZDOOM_FLAGS`] bit 0.
const RESERVED_FLAG_SLOT: &str = "<reserved>";

/// Room-only flags appended after the UZDoom names.
#[cfg(test)]
const ROOM_ONLY_FLAGS: usize = 1;

impl ActorFlags {
    /// Bit index of `name`, if it is a UZDoom flag.
    ///
    /// Called per actor per tic by the movement code, so it is a hash
    /// lookup on a stack copy of the name (no allocation), not a scan of
    /// the 300-odd names.
    #[must_use]
    pub fn index_of(name: &str) -> Option<u16> {
        let name = name.trim().trim_start_matches('+');
        if name.is_empty() || name.starts_with('<') {
            return None;
        }
        let mut buf = [0u8; 48];
        let key = name.as_bytes();
        let upper: std::borrow::Cow<'_, str> = if key.len() <= buf.len() {
            let out = &mut buf[..key.len()];
            out.copy_from_slice(key);
            out.make_ascii_uppercase();
            // ASCII case changes keep UTF-8 valid.
            std::borrow::Cow::Borrowed(std::str::from_utf8(&buf[..key.len()]).ok()?)
        } else {
            std::borrow::Cow::Owned(name.to_ascii_uppercase())
        };
        flag_index_map().get(upper.as_ref()).copied()
    }

    /// Whether bit `index` is set.
    #[must_use]
    pub fn get(&self, index: u16) -> bool {
        let i = usize::from(index);
        let word = i >> 6;
        let bit = i & 63;
        self.0.get(word).is_some_and(|w| (w >> bit) & 1 == 1)
    }

    /// Set or clear bit `index`.
    pub fn set(&mut self, index: u16, on: bool) {
        let i = usize::from(index);
        let word = i >> 6;
        let bit = i & 63;
        if let Some(slot) = self.0.get_mut(word) {
            if on {
                *slot |= 1 << bit;
            } else {
                *slot &= !(1 << bit);
            }
        }
    }

    /// Read a flag by DECORATE name.
    #[must_use]
    pub fn get_name(&self, name: &str) -> bool {
        Self::index_of(name).is_some_and(|i| self.get(i))
    }

    /// Set a flag by DECORATE name. Returns false when the name is unknown.
    pub fn set_name(&mut self, name: &str, on: bool) -> bool {
        let Some(i) = Self::index_of(name) else {
            return false;
        };
        self.set(i, on);
        true
    }

    /// Raw words for the save trailer. Absent on older `.fsave` blobs.
    #[must_use]
    pub fn to_words(&self) -> [u64; FLAG_WORDS] {
        self.0
    }

    /// Replace the bitset from a save trailer. Extra words are ignored.
    pub fn load_words(&mut self, words: &[u64]) {
        self.0 = [0; FLAG_WORDS];
        for (slot, word) in self.0.iter_mut().zip(words) {
            *slot = *word;
        }
    }
}

/// The UZDoom flag names by bit, for script `bFLAG` reads.
#[must_use]
pub fn flag_names() -> Vec<String> {
    UZDOOM_FLAGS.iter().map(|n| (*n).to_owned()).collect()
}

/// An actor's UZDoom flags as words: its extra flag set with the core flags
/// the playsim keeps in `Actor::flags` merged in (`SHOOTABLE`, `SOLID`, …).
#[must_use]
pub fn actor_flag_words(a: &crate::actor::Actor) -> Vec<u64> {
    use crate::class as c;
    let mut set = a.extra_flags;
    for (bit, name) in [
        (c::FLAG_SOLID, "SOLID"),
        (c::FLAG_SHOOTABLE, "SHOOTABLE"),
        (c::FLAG_NOGRAVITY, "NOGRAVITY"),
        (c::FLAG_FLOAT, "FLOAT"),
        (c::FLAG_DROPOFF, "DROPOFF"),
        (c::FLAG_DORMANT, "DORMANT"),
        (c::FLAG_FRIENDLY, "FRIENDLY"),
        (c::FLAG_NOTARGET, "NOTARGET"),
        (c::FLAG_INVULNERABLE, "INVULNERABLE"),
        (c::FLAG_NODAMAGE, "NODAMAGE"),
        (c::FLAG_NOBLOOD, "NOBLOOD"),
        (c::FLAG_FLOATBOB, "FLOATBOB"),
        (c::FLAG_AMBUSH, "AMBUSH"),
        (c::FLAG_MISSILE, "MISSILE"),
        (c::FLAG_RIPPER, "RIPPER"),
        (c::FLAG_COUNTKILL, "COUNTKILL"),
        (c::FLAG_CORPSE, "CORPSE"),
    ] {
        set.set_name(name, a.flags & bit != 0);
    }
    set.to_words().to_vec()
}

/// True when `name` is one of the UZDoom `DEFINE_FLAG` names.
#[must_use]
pub fn is_uzdoom_flag(name: &str) -> bool {
    ActorFlags::index_of(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash lookup answers exactly as the old linear scan did, for every
    /// name in any case, with `+`, spaces, and for names that are not flags.
    #[test]
    fn index_of_matches_the_linear_scan() {
        fn scan(name: &str) -> Option<u16> {
            let upper = name.trim().trim_start_matches('+').to_ascii_uppercase();
            if upper.is_empty() || upper.starts_with('<') {
                return None;
            }
            UZDOOM_FLAGS
                .iter()
                .position(|n| n.eq_ignore_ascii_case(&upper))
                .and_then(|i| u16::try_from(i).ok())
        }
        let mut names: Vec<String> = Vec::new();
        for n in UZDOOM_FLAGS {
            names.push((*n).to_owned());
            names.push(n.to_ascii_lowercase());
            names.push(format!(" +{} ", n.to_ascii_lowercase()));
            names.push(format!("{n}X"));
        }
        for n in [
            "",
            "+",
            "<reserved>",
            "NOTAFLAG",
            "Inventory.AlwaysPickup",
            "é",
            &"A".repeat(80),
        ] {
            names.push(n.to_owned());
        }
        for n in &names {
            assert_eq!(ActorFlags::index_of(n), scan(n), "{n:?}");
        }
        assert_eq!(ActorFlags::index_of("+inchase"), Some(1));
    }

    #[test]
    fn every_define_flag_has_a_bit() {
        // Reserved bit 0 + the UZDoom `DEFINE_FLAG` names + Room-only tail.
        let uzdoom = UZDOOM_FLAGS.len() - 1 - ROOM_ONLY_FLAGS;
        assert_eq!(uzdoom, 314);
        assert_eq!(UZDOOM_FLAGS[UZDOOM_FLAGS.len() - 1], "ROOMSCALEDCOLLISION");
        assert!(UZDOOM_FLAGS.len() <= FLAG_WORDS * 64);
        for (i, name) in UZDOOM_FLAGS.iter().enumerate().skip(1) {
            assert_eq!(ActorFlags::index_of(name), Some(i as u16), "{name}");
        }
        assert_eq!(ActorFlags::index_of("name"), None, "the old header row");
        assert_eq!(ActorFlags::index_of(RESERVED_FLAG_SLOT), None);
    }

    #[test]
    fn set_and_clear_roundtrip() {
        let mut f = ActorFlags::default();
        assert!(f.set_name("NOFRICTION", true));
        assert!(f.set_name("nofear", true));
        assert!(f.get_name("NOFRICTION"));
        assert!(f.get_name("NOFEAR"));
        assert!(f.set_name("NOFRICTION", false));
        assert!(!f.get_name("NOFRICTION"));
        assert!(f.get_name("NOFEAR"));
        assert!(!f.set_name("NOT_A_REAL_FLAG", true));
    }
}

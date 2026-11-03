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
    "name",
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
];

impl ActorFlags {
    /// Bit index of `name`, if it is a UZDoom flag.
    #[must_use]
    pub fn index_of(name: &str) -> Option<u16> {
        let upper = name.trim().trim_start_matches('+').to_ascii_uppercase();
        UZDOOM_FLAGS
            .iter()
            .position(|n| n.eq_ignore_ascii_case(&upper))
            .and_then(|i| u16::try_from(i).ok())
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

/// True when `name` is one of the UZDoom `DEFINE_FLAG` names.
#[must_use]
pub fn is_uzdoom_flag(name: &str) -> bool {
    ActorFlags::index_of(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_define_flag_has_a_bit() {
        assert_eq!(UZDOOM_FLAGS.len(), 315);
        assert!(UZDOOM_FLAGS.len() <= FLAG_WORDS * 64);
        for (i, name) in UZDOOM_FLAGS.iter().enumerate() {
            assert_eq!(ActorFlags::index_of(name), Some(i as u16), "{name}");
        }
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

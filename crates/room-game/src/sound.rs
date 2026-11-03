//! Bridge sim sound events → mixer via SNDINFO + VFS sample decode.

use room_audio::{ENGINE_SAMPLE_RATE, SampleBank, SoundEvent, SoundFlags, SoundOrigin};
use room_core::{DVec3, SoundId};
use room_defs::{SoundRegistry, parse_sndinfo};
use room_formats::decode_sound;
use room_sim::{SimSoundAdjust, SimSoundEvent};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Resolved sound table: logical SNDINFO names → [`SoundId`] + PCM bank.
#[derive(Clone, Debug)]
pub struct SoundSystem {
    /// Parsed SNDINFO.
    pub registry: SoundRegistry,
    /// Logical name (lowercase) → sound id.
    ids: BTreeMap<String, SoundId>,
    /// Lump name (uppercase) → sound id (shared when aliases collide).
    lump_ids: BTreeMap<String, SoundId>,
    /// Decoded samples (at [`Self::sample_rate`]).
    pub bank: SampleBank,
    /// Target PCM rate for the mixer / device.
    pub sample_rate: u32,
    next_id: u32,
}

impl Default for SoundSystem {
    fn default() -> Self {
        Self {
            registry: SoundRegistry::default(),
            ids: BTreeMap::new(),
            lump_ids: BTreeMap::new(),
            bank: SampleBank::new(),
            sample_rate: ENGINE_SAMPLE_RATE,
            next_id: 0,
        }
    }
}

impl SoundSystem {
    /// Empty system at the engine default rate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty system targeting a device output rate.
    #[must_use]
    pub fn with_sample_rate(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            ..Self::default()
        }
    }

    /// Parse SNDINFO text into the registry (replaces prior defs; keeps loaded samples).
    pub fn load_sndinfo(&mut self, text: &str) -> Result<(), String> {
        self.registry = parse_sndinfo(text).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load every `SNDINFO` lump from `vfs` (concatenated in mount order).
    pub fn load_sndinfo_from_vfs(&mut self, vfs: &Vfs) -> Result<(), String> {
        let mut combined = String::new();
        for id in vfs.find_all("SNDINFO", Namespace::Global) {
            let bytes = vfs.read(id).map_err(|e| e.to_string())?;
            combined.push_str(&String::from_utf8_lossy(&bytes));
            combined.push('\n');
        }
        if combined.is_empty() {
            if let Some(id) = vfs.find_any_ns("SNDINFO") {
                let bytes = vfs.read(id).map_err(|e| e.to_string())?;
                combined.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
        if combined.is_empty() {
            return Err("no SNDINFO lump".into());
        }
        self.load_sndinfo(&combined)
    }

    /// Resolve a logical name to a [`SoundId`], loading the lump from `vfs` if needed.
    ///
    /// `$random` names are re-resolved each call (index selects the variant); plain
    /// lump aliases are cached on the logical name.
    pub fn resolve(&mut self, vfs: &Vfs, logical: &str, random_index: u32) -> Option<SoundId> {
        let key = logical.to_ascii_lowercase();
        let is_random = matches!(
            self.registry.sounds.get(&key),
            Some(room_defs::SoundDef::Random(_))
        );
        if !is_random {
            if let Some(&id) = self.ids.get(&key) {
                return Some(id);
            }
        }
        let lump = self
            .registry
            .resolve_lump(logical, random_index)?
            .to_owned();
        let lump_key = lump.to_ascii_uppercase();
        if let Some(&id) = self.lump_ids.get(&lump_key) {
            if !is_random {
                self.ids.insert(key, id);
            }
            return Some(id);
        }
        let id = vfs
            .find(&lump_key, Namespace::Sounds)
            .or_else(|| vfs.find(&lump_key, Namespace::Global))
            .or_else(|| vfs.find_any_ns(&lump_key))?;
        let bytes = vfs.read(id).ok()?;
        let decoded = decode_sound(&bytes).ok()?;
        // Doom DMX lumps are often 11025 Hz; play at the mixer/device rate.
        let sid = SoundId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.bank
            .insert_resampled(sid, &decoded.samples, decoded.rate.max(1), self.sample_rate);
        self.lump_ids.insert(lump_key, sid);
        if !is_random {
            self.ids.insert(key, sid);
        }
        Some(sid)
    }

    /// Translate drained sim events into mixer commands (unknown sounds skipped).
    pub fn translate(
        &mut self,
        vfs: &Vfs,
        events: &[SimSoundEvent],
        random_index: u32,
    ) -> Vec<SoundEvent> {
        let mut out = Vec::with_capacity(events.len());
        for (i, ev) in events.iter().enumerate() {
            if ev.adjust == SimSoundAdjust::StopAll {
                out.push(SoundEvent::StopAll);
                continue;
            }
            if ev.adjust == SimSoundAdjust::Volume || ev.adjust == SimSoundAdjust::Pitch {
                let origin = if ev.attenuation <= 0.0 {
                    SoundOrigin::None
                } else {
                    SoundOrigin::Point(DVec3::new(ev.x, ev.y, ev.z))
                };
                if ev.adjust == SimSoundAdjust::Pitch {
                    out.push(SoundEvent::SetPitch {
                        origin,
                        channel: ev.channel,
                        pitch: ev.pitch,
                    });
                } else {
                    out.push(SoundEvent::SetVolume {
                        origin,
                        channel: ev.channel,
                        volume: ev.volume,
                    });
                }
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let idx = random_index.wrapping_mul(31).wrapping_add(i as u32);
            let Some(sound) = self.resolve(vfs, &ev.name, idx) else {
                continue;
            };
            let attenuation = ev.attenuation;
            let (origin, flags) = if attenuation <= 0.0 {
                (SoundOrigin::None, SoundFlags(SoundFlags::ATTN_NONE))
            } else {
                (
                    SoundOrigin::Point(DVec3::new(ev.x, ev.y, ev.z)),
                    SoundFlags::default(),
                )
            };
            out.push(SoundEvent::Start {
                origin,
                channel: ev.channel,
                sound,
                volume: ev.volume,
                attenuation,
                pitch: ev.pitch,
                flags,
            });
        }
        out
    }

    /// UI / menu cue (2D / no attenuation).
    pub fn ui_event(&mut self, vfs: &Vfs, logical: &str) -> Option<SoundEvent> {
        let sound = self.resolve(vfs, logical, 0)?;
        Some(SoundEvent::Start {
            origin: SoundOrigin::None,
            channel: 0,
            sound,
            volume: 1.0,
            attenuation: 0.0,
            pitch: 1.0,
            flags: SoundFlags(SoundFlags::UI),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::VfsBuilder;
    use std::path::PathBuf;

    #[test]
    fn freedoom_sndinfo_resolves_pistol() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let eng = root.join("../../wadsrc/room");
        let wad = root.join("../../testdata/freedoom/freedoom2.wad");
        if !eng.join("sndinfo.txt").is_file() || !wad.is_file() {
            return;
        }
        let vfs = VfsBuilder::new()
            .mount_path(&eng)
            .unwrap()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut sys = SoundSystem::new();
        sys.load_sndinfo_from_vfs(&vfs).unwrap();
        assert!(sys.registry.sounds.contains_key("weapons/pistol"));
        let id = sys.resolve(&vfs, "weapons/pistol", 0).unwrap();
        assert!(sys.bank.get(id).is_some());
        assert!(!sys.bank.get(id).unwrap().is_empty());
    }

    #[test]
    fn translate_skips_unknown() {
        let mut sys = SoundSystem::new();
        sys.load_sndinfo("weapons/pistol DSPISTOL\n").unwrap();
        let vfs = VfsBuilder::new().build().unwrap();
        let ev = [SimSoundEvent {
            name: "missing/sound".into(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            volume: 1.0,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            attenuation: 1.0,
            seat: None,
        }];
        assert!(sys.translate(&vfs, &ev, 0).is_empty());
    }

    fn minimal_dmx() -> Vec<u8> {
        let mut dmx = Vec::new();
        dmx.extend_from_slice(&3u16.to_le_bytes());
        dmx.extend_from_slice(&11025u16.to_le_bytes());
        dmx.extend_from_slice(&(36u32).to_le_bytes());
        dmx.extend(std::iter::repeat_n(128u8, 36));
        dmx
    }

    #[test]
    fn resolve_random_picks_vary_with_index() {
        let mut sys = SoundSystem::new();
        sys.load_sndinfo(
            "$random grunt/death { grunt/death1 grunt/death2 }\n\
             grunt/death1 DSPODTH1\n\
             grunt/death2 DSPODTH2\n",
        )
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("DSPODTH1", minimal_dmx())
            .unwrap()
            .mount_memory("DSPODTH2", minimal_dmx())
            .unwrap()
            .build()
            .unwrap();
        let a = sys.resolve(&vfs, "grunt/death", 0).unwrap();
        let b = sys.resolve(&vfs, "grunt/death", 1).unwrap();
        assert_ne!(
            a, b,
            "different $random indices should yield different samples"
        );
        let a2 = sys.resolve(&vfs, "grunt/death", 0).unwrap();
        assert_eq!(a, a2, "same index should resolve to the same lump id");
    }

    #[test]
    fn translate_zero_attenuation_is_full_volume() {
        let mut sys = SoundSystem::new();
        sys.load_sndinfo("cyber/death DSCYBDTH\n").unwrap();
        // Minimal DMX lump (type 3, 11025 Hz, 36 sample bytes with pads).
        let mut dmx = Vec::new();
        dmx.extend_from_slice(&3u16.to_le_bytes());
        dmx.extend_from_slice(&11025u16.to_le_bytes());
        dmx.extend_from_slice(&(36u32).to_le_bytes());
        dmx.extend(std::iter::repeat_n(128u8, 36));
        let vfs = VfsBuilder::new()
            .mount_memory("DSCYBDTH", dmx)
            .unwrap()
            .build()
            .unwrap();
        let ev = [SimSoundEvent {
            name: "cyber/death".into(),
            x: 100.0,
            y: 200.0,
            z: 0.0,
            volume: 1.0,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            attenuation: 0.0,
            seat: None,
        }];
        let out = sys.translate(&vfs, &ev, 0);
        assert_eq!(out.len(), 1);
        match &out[0] {
            SoundEvent::Start {
                origin,
                attenuation,
                flags,
                ..
            } => {
                assert_eq!(*origin, SoundOrigin::None);
                assert_eq!(*attenuation, 0.0);
                assert_eq!(flags.0 & SoundFlags::ATTN_NONE, SoundFlags::ATTN_NONE);
            }
            other => panic!("expected Start, got {other:?}"),
        }
    }
}

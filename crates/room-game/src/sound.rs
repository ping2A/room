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
            .resolve_lump(logical, random_index)
            .unwrap_or(logical)
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
            if ev.adjust == SimSoundAdjust::StopActor {
                if let Some(source_id) = ev.source_id {
                    out.push(SoundEvent::StopActor { source_id });
                }
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
            let Some(sound) = self.resolve_event_sound(vfs, ev, &ev.name, idx) else {
                continue;
            };
            let check_sound = if ev.adjust == SimSoundAdjust::StartIfNotSame {
                ev.check_name
                    .as_deref()
                    .and_then(|name| self.resolve_event_sound(vfs, ev, name, idx))
            } else {
                None
            };
            let mut attenuation = ev.attenuation;
            // SNDINFO `$rolloff name [type] min max`: the sound fades to silence
            // at `max` instead of Doom's 1200 (Brutal Doom's distant gunfire
            // carries far, blood drips stay local).
            if attenuation > 0.0 {
                if let Some(raw) = self.registry.rolloffs.get(&ev.name.to_ascii_lowercase()) {
                    let nums: Vec<f64> = raw
                        .split_whitespace()
                        .filter_map(|t| t.parse::<f64>().ok())
                        .collect();
                    if let Some(&max) = nums.last() {
                        if max > 0.0 {
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                attenuation *= (1200.0 / max) as f32;
                            }
                        }
                    }
                }
            }
            let (origin, mut flags) = if attenuation <= 0.0 {
                (SoundOrigin::None, SoundFlags(SoundFlags::ATTN_NONE))
            } else {
                (
                    SoundOrigin::Point(DVec3::new(ev.x, ev.y, ev.z)),
                    SoundFlags::default(),
                )
            };
            if ev.looping {
                flags.0 |= SoundFlags::LOOP;
            }
            if ev.channel_flags & (1 << 5) != 0 {
                flags.0 |= SoundFlags::UI;
            }
            if ev.channel_flags & (1 << 6) != 0 {
                flags.0 |= SoundFlags::NOPAUSE;
            }
            if ev.channel_flags & (1 << 13) != 0 {
                flags.0 |= SoundFlags::OVERLAP;
            }
            if ev.channel_flags & (1 << 12) != 0 {
                flags.0 |= SoundFlags::NOSTOP;
            }
            if ev.channel_flags & (1 << 14) != 0 {
                flags.0 |= SoundFlags::LOCAL;
            }
            if ev.channel_flags & (1 << 15) != 0 {
                flags.0 |= SoundFlags::TRANSIENT;
            }
            if ev.channel_flags & (1 << 3) != 0 {
                flags.0 |= SoundFlags::LISTENER_Z;
            }
            out.push(SoundEvent::Start {
                origin,
                channel: ev.channel,
                sound,
                check_sound,
                start_time: ev.start_time.max(0.0),
                volume: ev.volume,
                attenuation,
                pitch: ev.pitch,
                flags,
                source_id: ev.source_id,
                no_restart: ev.no_restart,
            });
        }
        out
    }

    fn resolve_event_sound(
        &mut self,
        vfs: &Vfs,
        event: &SimSoundEvent,
        logical: &str,
        random_index: u32,
    ) -> Option<SoundId> {
        if logical.starts_with('*') {
            let class = event.sound_class.as_deref().unwrap_or("player");
            let gender = event.sound_gender.as_deref().unwrap_or("male");
            let resolved = self
                .registry
                .resolve_player_lump(class, gender, logical)
                .unwrap_or(logical)
                .to_owned();
            self.resolve(vfs, &resolved, random_index)
        } else {
            self.resolve(vfs, logical, random_index)
        }
    }

    /// UI / menu cue (2D / no attenuation).
    pub fn ui_event(&mut self, vfs: &Vfs, logical: &str) -> Option<SoundEvent> {
        let sound = self.resolve(vfs, logical, 0)?;
        Some(SoundEvent::Start {
            origin: SoundOrigin::None,
            channel: 0,
            sound,
            check_sound: None,
            start_time: 0.0,
            volume: 1.0,
            attenuation: 0.0,
            pitch: 1.0,
            flags: SoundFlags(SoundFlags::UI),
            source_id: None,
            no_restart: false,
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
            check_name: None,
            start_time: 0.0,
            sound_class: None,
            sound_gender: None,
            source_id: None,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            volume: 1.0,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            looping: false,
            no_restart: false,
            channel_flags: 0,
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
            check_name: None,
            start_time: 0.0,
            sound_class: None,
            sound_gender: None,
            source_id: None,
            x: 100.0,
            y: 200.0,
            z: 0.0,
            volume: 1.0,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            looping: false,
            no_restart: false,
            channel_flags: 0,
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

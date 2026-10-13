//! Runtime SNDSEQ table + tic-driven playback (GZDoom sound sequences subset).

use crate::world::World;
use std::collections::BTreeMap;

/// One playable step after defs → sim conversion.
#[derive(Clone, Debug, PartialEq)]
pub enum SndSeqStep {
    /// Play a logical sound name once.
    Play(String),
    /// Play every tic until the sequence is stopped (`playrepeat`).
    PlayRepeat(String),
    /// Play, wait `tics`, then repeat until stopped (`playloop`).
    PlayLoop {
        /// Logical sound name.
        sound: String,
        /// Period between plays, in tics.
        tics: i32,
    },
    /// Wait `tics` before the next step.
    Delay(i32),
    /// Absolute volume 0..=1.
    Volume(f32),
    /// Multiply current volume by `factor` (from SNDSEQ `volumerel`).
    VolumeRel(f32),
    /// Set attenuation scale (`0` = none, `1` = normal).
    Attenuation(f32),
    /// Stop (end of sequence).
    End,
}

/// Named sequence of steps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SndSeqDef {
    /// Uppercase name.
    pub name: String,
    /// Ordered steps.
    pub steps: Vec<SndSeqStep>,
    /// Optional `stopsound` played when the sequence finishes.
    pub stopsound: Option<String>,
}

/// Level SNDSEQ registry (names uppercase).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SndSeqTable {
    /// Sequences by uppercase name.
    pub sequences: BTreeMap<String, SndSeqDef>,
    /// Door slot index → sequence name (uppercase).
    pub door_slots: BTreeMap<i32, String>,
    /// Platform slot index → sequence name (uppercase).
    pub platform_slots: BTreeMap<i32, String>,
}

impl SndSeqTable {
    /// Look up by name (case-insensitive) or numeric door/platform slot.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SndSeqDef> {
        let key = name.to_ascii_uppercase();
        if let Some(def) = self.sequences.get(&key) {
            return Some(def);
        }
        if let Ok(slot) = key.parse::<i32>() {
            if let Some(seq) = self.door_slots.get(&slot) {
                return self.sequences.get(seq);
            }
            if let Some(seq) = self.platform_slots.get(&slot) {
                return self.sequences.get(seq);
            }
        }
        None
    }
}

/// One in-flight sequence instance.
#[derive(Clone, Debug)]
pub struct ActiveSndSeq {
    /// Sequence name (uppercase).
    pub name: String,
    /// Index into `SndSeqDef.steps`.
    pub step: usize,
    /// Remaining delay tics (0 = advance).
    pub delay: i32,
    /// Current volume.
    pub volume: f32,
    /// Attenuation scale for plays.
    pub attenuation: f32,
    /// Origin X.
    pub x: f64,
    /// Origin Y.
    pub y: f64,
    /// Origin Z.
    pub z: f64,
}

/// Start a named sequence at a world position. Returns false if unknown.
pub fn start_sequence(world: &mut World, name: &str, x: f64, y: f64, z: f64) -> bool {
    if name.is_empty() {
        return false;
    }
    let key = name.to_ascii_uppercase();
    let Some(def) = world.sndseq.get(&key) else {
        return false;
    };
    if def.steps.is_empty() {
        return false;
    }
    // Replace any prior run of the same name (bootstrap).
    stop_sequence(world, &key);
    world.active_sndseqs.push(ActiveSndSeq {
        name: key,
        step: 0,
        delay: 0,
        volume: 1.0,
        attenuation: 1.0,
        x,
        y,
        z,
    });
    true
}

/// Stop a named active sequence (plays `stopsound` if any). Returns false if none.
pub fn stop_sequence(world: &mut World, name: &str) -> bool {
    let key = name.to_ascii_uppercase();
    let Some(idx) = world.active_sndseqs.iter().position(|a| a.name == key) else {
        return false;
    };
    play_stopsound(world, idx);
    world.active_sndseqs.remove(idx);
    true
}

/// Advance all active sequences one tic.
pub fn tick_sequences(world: &mut World) {
    if world.active_sndseqs.is_empty() {
        return;
    }
    let mut finished = Vec::new();
    for i in 0..world.active_sndseqs.len() {
        if !advance_one(world, i) {
            finished.push(i);
        }
    }
    for i in finished.into_iter().rev() {
        play_stopsound(world, i);
        world.active_sndseqs.remove(i);
    }
}

fn play_stopsound(world: &mut World, idx: usize) {
    let Some(active) = world.active_sndseqs.get(idx).cloned() else {
        return;
    };
    let Some(def) = world.sndseq.get(&active.name) else {
        return;
    };
    let Some(stop) = def.stopsound.clone() else {
        return;
    };
    if stop.is_empty() {
        return;
    }
    world.play_sound_attn(
        &stop,
        active.x,
        active.y,
        active.z,
        active.volume,
        active.attenuation,
    );
}

fn advance_one(world: &mut World, idx: usize) -> bool {
    loop {
        let Some(active) = world.active_sndseqs.get(idx).cloned() else {
            return false;
        };
        if active.delay > 0 {
            if let Some(a) = world.active_sndseqs.get_mut(idx) {
                a.delay -= 1;
            }
            return true;
        }
        let Some(def) = world.sndseq.get(&active.name).cloned() else {
            return false;
        };
        if active.step >= def.steps.len() {
            return false;
        }
        match &def.steps[active.step] {
            SndSeqStep::Play(sound) => {
                if !sound.is_empty() {
                    world.play_sound_attn(
                        sound,
                        active.x,
                        active.y,
                        active.z,
                        active.volume,
                        active.attenuation,
                    );
                }
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.step += 1;
                }
            }
            SndSeqStep::PlayRepeat(sound) => {
                if !sound.is_empty() {
                    world.play_sound_attn(
                        sound,
                        active.x,
                        active.y,
                        active.z,
                        active.volume,
                        active.attenuation,
                    );
                }
                // Stay on this step; keep alive until `stop_sequence`.
                return true;
            }
            SndSeqStep::PlayLoop { sound, tics } => {
                if !sound.is_empty() {
                    world.play_sound_attn(
                        sound,
                        active.x,
                        active.y,
                        active.z,
                        active.volume,
                        active.attenuation,
                    );
                }
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.delay = (*tics).max(0);
                }
                // Stay on this step; delay gates the next play.
                return true;
            }
            SndSeqStep::Delay(tics) => {
                let wait = (*tics).max(0);
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.step += 1;
                    a.delay = wait;
                }
                return true;
            }
            SndSeqStep::Volume(v) => {
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.volume = v.clamp(0.0, 1.0);
                    a.step += 1;
                }
            }
            SndSeqStep::VolumeRel(factor) => {
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.volume = (a.volume * factor.clamp(0.0, 4.0)).clamp(0.0, 1.0);
                    a.step += 1;
                }
            }
            SndSeqStep::Attenuation(attn) => {
                if let Some(a) = world.active_sndseqs.get_mut(idx) {
                    a.attenuation = attn.max(0.0);
                    a.step += 1;
                }
            }
            SndSeqStep::End => return false,
        }
        // Continue to next step in the same tic until a delay or end.
        if world
            .active_sndseqs
            .get(idx)
            .is_none_or(|a| a.step >= def.steps.len())
        {
            return false;
        }
        if world.active_sndseqs.get(idx).is_some_and(|a| a.delay > 0) {
            return true;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::world::World;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn plays_then_delays() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.sndseq.sequences.insert(
            "DOOR".into(),
            SndSeqDef {
                name: "DOOR".into(),
                steps: vec![
                    SndSeqStep::Play("DSDOOR".into()),
                    SndSeqStep::Delay(2),
                    SndSeqStep::Play("DSSTOP".into()),
                    SndSeqStep::End,
                ],
                stopsound: None,
            },
        );
        assert!(start_sequence(&mut w, "door", 1.0, 2.0, 3.0));
        tick_sequences(&mut w);
        assert_eq!(w.sound_events.len(), 1);
        assert_eq!(w.sound_events[0].name, "DSDOOR");
        w.sound_events.clear();
        tick_sequences(&mut w); // delay 2 → 1
        assert!(w.sound_events.is_empty());
        tick_sequences(&mut w); // delay 1 → 0
        assert!(w.sound_events.is_empty());
        tick_sequences(&mut w); // play stop
        assert_eq!(w.sound_events.len(), 1);
        assert_eq!(w.sound_events[0].name, "DSSTOP");
        tick_sequences(&mut w); // end
        assert!(w.active_sndseqs.is_empty());
    }

    #[test]
    fn attenuation_volumerel_stopsound() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.sndseq.sequences.insert(
            "PLAT".into(),
            SndSeqDef {
                name: "PLAT".into(),
                steps: vec![
                    SndSeqStep::Volume(0.8),
                    SndSeqStep::VolumeRel(0.5),
                    SndSeqStep::Attenuation(0.0),
                    SndSeqStep::Play("DSPLAT".into()),
                    SndSeqStep::End,
                ],
                stopsound: Some("DSPSTOP".into()),
            },
        );
        assert!(start_sequence(&mut w, "plat", 1.0, 2.0, 3.0));
        tick_sequences(&mut w);
        assert_eq!(w.sound_events.len(), 2);
        assert_eq!(w.sound_events[0].name, "DSPLAT");
        assert!((w.sound_events[0].volume - 0.4).abs() < 1e-5);
        assert_eq!(w.sound_events[0].attenuation, 0.0);
        assert_eq!(w.sound_events[1].name, "DSPSTOP");
        assert!(w.active_sndseqs.is_empty());
    }

    #[test]
    fn playrepeat_and_playloop_until_stop() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.sndseq.sequences.insert(
            "MOVE".into(),
            SndSeqDef {
                name: "MOVE".into(),
                steps: vec![SndSeqStep::PlayRepeat("DSMOVE".into())],
                stopsound: Some("DSSTOP".into()),
            },
        );
        w.sndseq.sequences.insert(
            "LOOP".into(),
            SndSeqDef {
                name: "LOOP".into(),
                steps: vec![SndSeqStep::PlayLoop {
                    sound: "DSLOOP".into(),
                    tics: 2,
                }],
                stopsound: None,
            },
        );
        assert!(start_sequence(&mut w, "move", 0.0, 0.0, 0.0));
        tick_sequences(&mut w);
        tick_sequences(&mut w);
        assert_eq!(w.sound_events.len(), 2);
        assert!(w.sound_events.iter().all(|e| e.name == "DSMOVE"));
        assert_eq!(w.active_sndseqs.len(), 1);
        w.sound_events.clear();
        assert!(stop_sequence(&mut w, "move"));
        assert_eq!(w.sound_events.len(), 1);
        assert_eq!(w.sound_events[0].name, "DSSTOP");
        assert!(w.active_sndseqs.is_empty());

        w.sound_events.clear();
        assert!(start_sequence(&mut w, "loop", 0.0, 0.0, 0.0));
        tick_sequences(&mut w); // play + delay 2
        assert_eq!(w.sound_events.len(), 1);
        w.sound_events.clear();
        tick_sequences(&mut w); // delay 2→1
        assert!(w.sound_events.is_empty());
        tick_sequences(&mut w); // delay 1→0
        assert!(w.sound_events.is_empty());
        tick_sequences(&mut w); // play again
        assert_eq!(w.sound_events.len(), 1);
        assert_eq!(w.sound_events[0].name, "DSLOOP");
        assert!(stop_sequence(&mut w, "loop"));
        assert!(w.active_sndseqs.is_empty());
    }
}

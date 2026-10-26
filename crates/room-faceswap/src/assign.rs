//! Face assignment rules (`docs/26` §6). Playsim-RNG free.

use crate::profile::FaceId;
use std::collections::BTreeMap;

/// How to pick a face for a matching actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PickMode {
    /// Same face for the whole class.
    PerClass,
    /// Hash of `(spawn_index, map_name)` — not the sim RNG.
    PerInstance,
    /// Always the first listed face.
    Fixed,
}

/// One assignment rule.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssignmentRule {
    /// DECORATE / Duke class names.
    pub classes: Vec<String>,
    /// Gallery face ids (string labels resolved by the host).
    pub faces: Vec<String>,
    /// Pick strategy.
    pub pick: PickMode,
}

/// Full assignment table.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaceAssignment {
    /// Ordered rules (first match wins).
    pub rules: Vec<AssignmentRule>,
}

/// Resolve a face label for an actor. Returns the gallery key string.
#[must_use]
pub fn resolve_face_key(
    assignment: &FaceAssignment,
    class_name: &str,
    spawn_index: u32,
    map_name: &str,
) -> Option<String> {
    for rule in &assignment.rules {
        if !rule
            .classes
            .iter()
            .any(|c| c.eq_ignore_ascii_case(class_name))
        {
            continue;
        }
        if rule.faces.is_empty() {
            return None;
        }
        return Some(match rule.pick {
            PickMode::Fixed | PickMode::PerClass => rule.faces[0].clone(),
            PickMode::PerInstance => {
                let h = instance_hash(spawn_index, map_name, class_name);
                let i = (h as usize) % rule.faces.len();
                rule.faces[i].clone()
            }
        });
    }
    None
}

/// Stable non-sim hash for per-instance picks.
#[must_use]
pub fn instance_hash(spawn_index: u32, map_name: &str, class_name: &str) -> u64 {
    use xxhash_rust::xxh3::xxh3_64;
    let mut buf = Vec::new();
    buf.extend_from_slice(&spawn_index.to_le_bytes());
    buf.extend_from_slice(map_name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(class_name.as_bytes());
    xxh3_64(&buf)
}

/// Map gallery string keys → [`FaceId`] for runtime lookups.
#[derive(Clone, Debug, Default)]
pub struct FaceCatalog {
    /// Label → id.
    pub by_name: BTreeMap<String, FaceId>,
}

impl FaceCatalog {
    /// Insert a named face.
    pub fn insert(&mut self, name: impl Into<String>, id: FaceId) {
        self.by_name.insert(name.into(), id);
    }

    /// Lookup.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<FaceId> {
        self.by_name.get(name).copied()
    }
}

/// Renderer-side: which face overlays which actor (never written into sim).
#[must_use]
pub fn face_for_actor(
    assignment: &FaceAssignment,
    catalog: &FaceCatalog,
    class_name: &str,
    spawn_index: u32,
    map_name: &str,
) -> Option<FaceId> {
    let key = resolve_face_key(assignment, class_name, spawn_index, map_name)?;
    catalog.get(&key)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn per_instance_stable() {
        let a = FaceAssignment {
            rules: vec![AssignmentRule {
                classes: vec!["ZombieMan".into()],
                faces: vec!["me".into(), "alex".into()],
                pick: PickMode::PerInstance,
            }],
        };
        let k1 = resolve_face_key(&a, "ZombieMan", 3, "MAP01").unwrap();
        let k2 = resolve_face_key(&a, "ZombieMan", 3, "MAP01").unwrap();
        assert_eq!(k1, k2);
        let k3 = resolve_face_key(&a, "ZombieMan", 4, "MAP01").unwrap();
        // Different spawn may differ; just ensure function runs.
        let _ = k3;
    }
}

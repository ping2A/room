//! Flat state table compiled from DECORATE (M5 bootstrap).

use crate::class::ClassRegistry;
use room_core::{ClassId, StateId};
use room_decorate::{ActorDef, ClassDefIr, StateFlow, StateItem};
use std::collections::{BTreeMap, BTreeSet};

/// One compiled state (`FState` subset).
#[derive(Clone, Debug)]
pub struct State {
    /// Owning class.
    pub owner: ClassId,
    /// Sprite name (4 chars).
    pub sprite: String,
    /// Frame letter index (`'A'` = 0).
    pub frame: u8,
    /// Duration (`-1` = forever).
    pub tics: i16,
    /// Action function name (`A_Look`), if any.
    pub action: Option<String>,
    /// Action arguments from DECORATE (`A_SetScale(2)`).
    pub action_args: Vec<String>,
    /// DECORATE `Bright` — fullbright sprite for this frame.
    pub bright: bool,
    /// DECORATE `Fast` — tics halved (rounded up) under fast monsters.
    pub fast: bool,
    /// Next state (`None` = Stop).
    pub next: Option<StateId>,
}

/// Global state table + per-class label → state map.
#[derive(Clone, Debug, Default)]
pub struct StateTable {
    states: Vec<State>,
    /// `(class, lowercase label)` → first state id.
    labels: BTreeMap<(u32, String), StateId>,
}

impl StateTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile all actor states from DECORATE IR using an existing class registry.
    pub fn from_decorate(ir: &ClassDefIr, classes: &ClassRegistry) -> Self {
        let mut table = Self::new();
        for def in &ir.actors {
            let Some(cid) = classes.find_name(&def.name) else {
                continue;
            };
            table.compile_actor(cid, def);
        }
        table.inherit_labels(classes);
        table
    }

    /// DECORATE inheritance: a class keeps every parent label it does not
    /// redefine (pointing at the parent's states). Parents are resolved first.
    pub(crate) fn inherit_labels(&mut self, classes: &ClassRegistry) {
        let mut done: BTreeSet<u32> = BTreeSet::new();
        for class in classes.iter() {
            self.inherit_labels_for(classes, class.id, &mut done, 0);
        }
    }

    fn inherit_labels_for(
        &mut self,
        classes: &ClassRegistry,
        class: ClassId,
        done: &mut BTreeSet<u32>,
        depth: u32,
    ) {
        if depth > 64 || !done.insert(class.raw()) {
            return;
        }
        let Some(parent) = classes.get(class).and_then(|c| c.parent) else {
            return;
        };
        if parent == class {
            return;
        }
        self.inherit_labels_for(classes, parent, done, depth + 1);
        let inherited: Vec<(String, StateId)> = self
            .labels_of(parent)
            .map(|(l, id)| (l.to_owned(), id))
            .collect();
        for (label, id) in inherited {
            self.labels.entry((class.raw(), label)).or_insert(id);
        }
    }

    fn compile_actor(&mut self, class: ClassId, def: &ActorDef) {
        let Some(block) = &def.states else {
            return;
        };
        // Pass 1: allocate states for each frame letter; record label → first index.
        let start_index = self.states.len();
        let mut label_at: BTreeMap<String, usize> = BTreeMap::new();
        let mut pending_labels: Vec<String> = Vec::new();
        let mut flow_slots: Vec<(usize, StateFlow)> = Vec::new();

        for item in &block.items {
            match item {
                StateItem::Label(name) => {
                    pending_labels.push(name.clone());
                }
                StateItem::Line(line) => {
                    let first = self.states.len();
                    for lab in pending_labels.drain(..) {
                        label_at.insert(lab.to_ascii_lowercase(), first);
                        self.labels.insert(
                            (class.raw(), lab.to_ascii_lowercase()),
                            StateId::from_raw(first as u32),
                        );
                    }
                    for (fi, ch) in line.frames.chars().enumerate() {
                        let frame = match ch {
                            'A'..='Z' => (ch as u8) - b'A',
                            'a'..='z' => (ch as u8) - b'a',
                            '[' => 26,
                            '\\' => 27,
                            ']' => 28,
                            _ => 0,
                        };
                        #[allow(clippy::cast_possible_truncation)]
                        let tics = line.duration as i16;
                        let bright = line
                            .modifiers
                            .iter()
                            .any(|m| m.eq_ignore_ascii_case("bright"));
                        let fast = line
                            .modifiers
                            .iter()
                            .any(|m| m.eq_ignore_ascii_case("fast"));
                        let id = self.states.len();
                        self.states.push(State {
                            owner: class,
                            sprite: line.sprite.clone(),
                            frame,
                            tics,
                            action: line.action.clone(),
                            action_args: line.action_args.clone(),
                            bright,
                            fast,
                            // Temporary: chain to next frame; flows patched later.
                            next: Some(StateId::from_raw((id + 1) as u32)),
                        });
                        let _ = fi;
                    }
                }
                StateItem::Flow(flow) => {
                    if let Some(last) = self.states.len().checked_sub(1) {
                        if last >= start_index {
                            flow_slots.push((last, flow.clone()));
                        }
                    }
                }
            }
        }
        // Drop dangling next past end.
        if let Some(last) = self.states.len().checked_sub(1) {
            if last >= start_index {
                self.states[last].next = None;
            }
        }
        // Pass 2: apply flow keywords to the last state of the preceding line.
        for (idx, flow) in flow_slots {
            match flow {
                StateFlow::Stop | StateFlow::Fail => {
                    self.states[idx].next = None;
                }
                StateFlow::Wait => {
                    self.states[idx].next = Some(StateId::from_raw(idx as u32));
                }
                StateFlow::Loop => {
                    // Jump to most recent label before this state.
                    let target = label_at
                        .values()
                        .copied()
                        .filter(|&i| i <= idx)
                        .max()
                        .unwrap_or(start_index);
                    self.states[idx].next = Some(StateId::from_raw(target as u32));
                }
                StateFlow::Goto { label, offset } => {
                    let key = label.to_ascii_lowercase();
                    let base = label_at.get(&key).copied().or_else(|| {
                        // Super::Label — resolve later; for now treat as local.
                        let short = key.rsplit("::").next()?.to_owned();
                        label_at.get(&short).copied()
                    });
                    if let Some(b) = base {
                        let dest = (b as i32 + offset).max(0) as usize;
                        if dest < self.states.len() {
                            self.states[idx].next = Some(StateId::from_raw(dest as u32));
                        }
                    }
                }
            }
        }
    }

    /// Borrow a state.
    #[must_use]
    pub fn get(&self, id: StateId) -> Option<&State> {
        self.states.get(id.raw() as usize)
    }

    /// Resolve a class label.
    #[must_use]
    pub fn find_label(&self, class: ClassId, label: &str) -> Option<StateId> {
        self.labels
            .get(&(class.raw(), label.to_ascii_lowercase()))
            .copied()
    }

    /// A class's own labels (lowercase) and their first states, by label name.
    pub fn labels_of(&self, class: ClassId) -> impl Iterator<Item = (&str, StateId)> + '_ {
        let raw = class.raw();
        self.labels
            .range((raw, String::new())..)
            .take_while(move |((c, _), _)| *c == raw)
            .map(|((_, l), &id)| (l.as_str(), id))
    }

    /// State count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::ClassRegistry;
    use room_decorate::parse_decorate;

    #[test]
    fn compiles_zombieman_spawn_loop() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  States
  {
  Spawn:
    POSS AB 10 A_Look
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let table = StateTable::from_decorate(&ir, &classes);
        let cid = classes.find_name("ZombieMan").unwrap();
        let spawn = table.find_label(cid, "Spawn").unwrap();
        assert_eq!(spawn.raw(), 0);
        let a = table.get(spawn).unwrap();
        assert_eq!(a.sprite, "POSS");
        assert_eq!(a.frame, 0);
        assert_eq!(a.tics, 10);
        assert_eq!(a.action.as_deref(), Some("A_Look"));
        let b = table.get(a.next.unwrap()).unwrap();
        assert_eq!(b.frame, 1);
        // Loop → back to Spawn (frame A).
        assert_eq!(b.next.unwrap().raw(), spawn.raw());
        let labels: Vec<_> = table.labels_of(cid).map(|(l, _)| l.to_owned()).collect();
        assert_eq!(labels, ["spawn"]);
    }

    #[test]
    fn bundled_doom_monsters_have_vanilla_gib_and_raise_states() {
        let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let t = StateTable::from_decorate(&ir, &classes);
        let label = |c: &str, l: &str| t.find_label(classes.find_name(c).unwrap(), l);
        for c in [
            "ZombieMan",
            "ShotgunGuy",
            "ChaingunGuy",
            "DoomImp",
            "WolfensteinSS",
        ] {
            assert!(label(c, "XDeath").is_some(), "{c} has no XDeath");
        }
        for c in [
            "ZombieMan",
            "ShotgunGuy",
            "ChaingunGuy",
            "DoomImp",
            "Demon",
            "BaronOfHell",
            "HellKnight",
            "Cacodemon",
            "Revenant",
            "Arachnotron",
            "Fatso",
            "WolfensteinSS",
            "PainElemental",
        ] {
            assert!(label(c, "Raise").is_some(), "{c} has no Raise");
        }
        // Not resurrectable in Doom (no raisestate).
        for c in ["LostSoul", "Archvile", "Cyberdemon", "SpiderMastermind"] {
            assert!(label(c, "Raise").is_none(), "{c} must not have Raise");
        }
        // Arch-vile heal uses the `[`, `\`, `]` sprite frames.
        let mut at = label("Archvile", "Heal");
        let mut frames = Vec::new();
        for _ in 0..3 {
            let s = t.get(at.unwrap()).unwrap();
            frames.push(s.frame);
            at = s.next;
        }
        assert_eq!(frames, [26, 27, 28]);
    }

    #[test]
    fn compiles_bright_modifier() {
        let ir = parse_decorate(
            r#"
actor BulletPuff
{
  States
  {
  Spawn:
    PUFF A 4 Bright
    PUFF B 4
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let table = StateTable::from_decorate(&ir, &classes);
        let cid = classes.find_name("BulletPuff").unwrap();
        let spawn = table.find_label(cid, "Spawn").unwrap();
        assert!(table.get(spawn).unwrap().bright);
        assert!(
            !table
                .get(table.get(spawn).unwrap().next.unwrap())
                .unwrap()
                .bright
        );
    }
}

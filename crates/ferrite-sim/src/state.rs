//! Flat state table compiled from DECORATE (M5 bootstrap).

use crate::class::ClassRegistry;
use ferrite_core::{ClassId, StateId};
use ferrite_decorate::{ActorDef, ClassDefIr, StateFlow, StateItem};
use std::collections::BTreeMap;

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
        table
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
                        let id = self.states.len();
                        self.states.push(State {
                            owner: class,
                            sprite: line.sprite.clone(),
                            frame,
                            tics,
                            action: line.action.clone(),
                            action_args: line.action_args.clone(),
                            bright,
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
    use ferrite_decorate::parse_decorate;

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

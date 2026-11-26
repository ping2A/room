//! Flat state table compiled from DECORATE (M5 bootstrap).

use crate::class::ClassRegistry;
use room_core::{ClassId, StateId};
use room_decorate::{ActorDef, ClassDefIr, StateFlow, StateItem};
use room_script::RegCall;
use room_script_abi::{state_flags, state_flow_kind};
use std::collections::{BTreeMap, BTreeSet};

/// DECORATE frame `#` — do not change the actor's current frame.
pub(crate) const FRAME_NOCHANGE: u8 = 255;

/// `####` / `----` — do not change the actor's current sprite.
#[must_use]
pub(crate) fn sprite_unchanged(sprite: &str) -> bool {
    sprite.eq_ignore_ascii_case("####") || sprite.eq_ignore_ascii_case("----")
}

/// Parent-chain length, so a subclass's `Goto Super::…` compiles after the parent.
fn zscript_parent_depth(classes: &ClassRegistry, name: &str) -> u32 {
    let mut depth = 0u32;
    let mut cur = classes.find_name(name);
    for _ in 0..64 {
        let Some(id) = cur else { break };
        let Some(parent) = classes.get(id).and_then(|c| c.parent) else {
            break;
        };
        if parent == id {
            break;
        }
        depth += 1;
        cur = Some(parent);
    }
    depth
}

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
    /// DECORATE `CanRaise`.
    pub canraise: bool,
    /// DECORATE `NoDelay`.
    pub nodelay: bool,
    /// Weapon psprite OFFSET x (0 = unset).
    pub offset_x: i32,
    /// Weapon psprite OFFSET y (0 = unset).
    pub offset_y: i32,
    /// WASM `room_dispatch` callback (−1 = none).
    pub callback_id: i32,
    /// Module that owns [`Self::callback_id`].
    pub module_index: usize,
    /// Guest `StateRef` from registration (0 if DECORATE).
    pub guest_ref: u32,
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
            table.compile_actor(cid, def, classes);
        }
        table.inherit_labels(classes);
        table
    }

    /// Merge ZScript `States { … }` bodies (ZS3) onto matching DECORATE classes.
    ///
    /// Each body is wrapped as a synthetic DECORATE actor and compiled in place.
    /// Unknown class names are skipped.
    pub fn merge_zscript_states(
        &mut self,
        classes: &ClassRegistry,
        class_states: &BTreeMap<String, String>,
    ) {
        // Parents first, so `Goto Super::Missile.Pistol` resolves while the
        // parent labels already exist.
        let mut names: Vec<&String> = class_states.keys().collect();
        names.sort_by(|a, b| {
            zscript_parent_depth(classes, a)
                .cmp(&zscript_parent_depth(classes, b))
                .then(a.cmp(b))
        });
        for name in names {
            let Some(body) = class_states.get(name) else {
                continue;
            };
            let Some(cid) = classes.find_name(name) else {
                continue;
            };
            let synthetic = format!("actor {name} : Actor {{\nStates\n{body}\n}}");
            let Ok(ir) = room_decorate::parse_decorate(&synthetic) else {
                continue;
            };
            let Some(def) = ir.actors.first() else {
                continue;
            };
            self.compile_actor(cid, def, classes);
        }
        if !class_states.is_empty() {
            self.inherit_labels(classes);
        }
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

    /// Resolve `Goto Label` against this class's labels, then the parent chain
    /// (so `Goto LightDone` on a gun finds `Weapon.LightDone`).
    fn resolve_goto_base(
        &self,
        classes: &ClassRegistry,
        class: ClassId,
        label_at: &BTreeMap<String, usize>,
        key: &str,
    ) -> Option<usize> {
        let short = key.rsplit("::").next().unwrap_or(key);
        if let Some(&i) = label_at.get(short) {
            return Some(i);
        }
        let mut cur = classes.get(class).and_then(|c| c.parent);
        for _ in 0..64 {
            let pid = cur?;
            if let Some(&sid) = self.labels.get(&(pid.raw(), short.to_owned())) {
                return Some(sid.raw() as usize);
            }
            cur = classes.get(pid).and_then(|c| c.parent);
        }
        None
    }

    fn compile_actor(&mut self, class: ClassId, def: &ActorDef, classes: &ClassRegistry) {
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
                            // `#` keeps the frame already on the actor (GZDoom).
                            '#' => FRAME_NOCHANGE,
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
                        let (offset_x, offset_y) = offset_from_modifiers(&line.modifiers);
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
                            canraise: line
                                .modifiers
                                .iter()
                                .any(|m| m.eq_ignore_ascii_case("canraise")),
                            nodelay: line
                                .modifiers
                                .iter()
                                .any(|m| m.eq_ignore_ascii_case("nodelay")),
                            offset_x,
                            offset_y,
                            callback_id: -1,
                            module_index: 0,
                            guest_ref: 0,
                            // Temporary: chain to next frame; flows patched later.
                            next: Some(StateId::from_raw((id + 1) as u32)),
                        });
                        let _ = fi;
                    }
                }
                StateItem::Flow(flow) => {
                    // `Missile: Goto Super::Missile.Pistol` has a label and no
                    // frames. Hang both on a 0-tic state so the jump is kept.
                    if !pending_labels.is_empty() {
                        let id = self.states.len();
                        for lab in pending_labels.drain(..) {
                            label_at.insert(lab.to_ascii_lowercase(), id);
                            self.labels.insert(
                                (class.raw(), lab.to_ascii_lowercase()),
                                StateId::from_raw(id as u32),
                            );
                        }
                        self.states.push(State {
                            owner: class,
                            sprite: "TNT1".into(),
                            frame: 0,
                            tics: 0,
                            action: None,
                            action_args: Vec::new(),
                            bright: false,
                            fast: false,
                            canraise: false,
                            nodelay: false,
                            offset_x: 0,
                            offset_y: 0,
                            callback_id: -1,
                            module_index: 0,
                            guest_ref: 0,
                            next: None,
                        });
                    }
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
            let _ = self.apply_flow(classes, class, &label_at, start_index, idx, &flow, false);
        }
    }

    fn apply_flow(
        &mut self,
        classes: &ClassRegistry,
        class: ClassId,
        label_at: &BTreeMap<String, usize>,
        start_index: usize,
        idx: usize,
        flow: &StateFlow,
        strict_goto: bool,
    ) -> Result<(), String> {
        match flow {
            StateFlow::Stop | StateFlow::Fail => {
                self.states[idx].next = None;
            }
            StateFlow::Wait => {
                self.states[idx].next = Some(StateId::from_raw(idx as u32));
            }
            StateFlow::Loop => {
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
                if let Some(b) = self.resolve_goto_base(classes, class, label_at, &key) {
                    let dest = (b as i32 + offset).max(0) as usize;
                    if dest < self.states.len() {
                        self.states[idx].next = Some(StateId::from_raw(dest as u32));
                    } else if strict_goto {
                        return Err(format!("Goto {label}+{offset} out of range"));
                    } else {
                        self.states[idx].next = None;
                    }
                } else if strict_goto {
                    return Err(format!("unresolved Goto {label}"));
                } else {
                    // Unresolved Goto must not fall through (docs/30 N1).
                    self.states[idx].next = None;
                }
            }
        }
        Ok(())
    }

    /// Merge module `reg_state*` calls into this table for `class` (`docs/31` MA1).
    ///
    /// # Errors
    ///
    /// Unresolved `Goto`, empty action name, or flow with no preceding state.
    pub fn import_script_states(
        &mut self,
        classes: &ClassRegistry,
        class: ClassId,
        module_index: usize,
        calls: &[RegCall],
    ) -> Result<(), String> {
        if calls.is_empty() {
            return Ok(());
        }
        let start_index = self.states.len();
        let mut label_at: BTreeMap<String, usize> = BTreeMap::new();
        let mut pending_labels: Vec<String> = Vec::new();
        let mut flow_slots: Vec<(usize, StateFlow)> = Vec::new();

        for call in calls {
            match call {
                RegCall::StateLabel { label } => {
                    pending_labels.push(label.clone());
                }
                RegCall::State {
                    sprite,
                    frame,
                    tics,
                    flags,
                    callback_id,
                    state_ref,
                } => {
                    let first = self.states.len();
                    for lab in pending_labels.drain(..) {
                        let key = lab.to_ascii_lowercase();
                        label_at.insert(key.clone(), first);
                        self.labels
                            .insert((class.raw(), key), StateId::from_raw(first as u32));
                    }
                    let frame = match *frame {
                        b @ b'A'..=b'Z' => b - b'A',
                        b @ b'a'..=b'z' => b - b'a',
                        n if n < 29 => n,
                        _ => 0,
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    let tics_i16 = (*tics).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                    let id = self.states.len();
                    self.states.push(State {
                        owner: class,
                        sprite: sprite.clone(),
                        frame,
                        tics: tics_i16,
                        action: None,
                        action_args: Vec::new(),
                        bright: flags & state_flags::BRIGHT != 0,
                        fast: flags & state_flags::FAST != 0,
                        canraise: flags & state_flags::CANRAISE != 0,
                        nodelay: flags & state_flags::NODELAY != 0,
                        offset_x: 0,
                        offset_y: 0,
                        callback_id: *callback_id,
                        module_index,
                        guest_ref: *state_ref,
                        next: Some(StateId::from_raw((id + 1) as u32)),
                    });
                }
                RegCall::StateAction { name, args } => {
                    let Some(last) = self.states.len().checked_sub(1) else {
                        return Err("reg_state_action with no state".into());
                    };
                    if last < start_index {
                        return Err("reg_state_action with no state".into());
                    }
                    if name.is_empty() {
                        return Err("empty state action name".into());
                    }
                    self.states[last].action = Some(name.clone());
                    self.states[last].action_args = split_decorate_args(args);
                }
                RegCall::StateOffset { x, y } => {
                    let Some(last) = self.states.len().checked_sub(1) else {
                        return Err("reg_state_offset with no state".into());
                    };
                    if last < start_index {
                        return Err("reg_state_offset with no state".into());
                    }
                    self.states[last].offset_x = *x;
                    self.states[last].offset_y = *y;
                }
                RegCall::StateFlow {
                    kind,
                    label,
                    offset,
                } => {
                    let Some(last) = self.states.len().checked_sub(1) else {
                        return Err("reg_state_flow with no state".into());
                    };
                    if last < start_index {
                        return Err("reg_state_flow with no state".into());
                    }
                    let flow = match *kind {
                        state_flow_kind::LOOP => StateFlow::Loop,
                        state_flow_kind::STOP => StateFlow::Stop,
                        state_flow_kind::WAIT => StateFlow::Wait,
                        state_flow_kind::FAIL => StateFlow::Fail,
                        state_flow_kind::GOTO => StateFlow::Goto {
                            label: label.clone(),
                            offset: *offset,
                        },
                        k => return Err(format!("unknown state flow kind {k}")),
                    };
                    flow_slots.push((last, flow));
                }
                _ => {}
            }
        }
        if let Some(last) = self.states.len().checked_sub(1) {
            if last >= start_index {
                self.states[last].next = None;
            }
        }
        for (idx, flow) in flow_slots {
            self.apply_flow(classes, class, &label_at, start_index, idx, &flow, true)?;
        }
        Ok(())
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

/// Split a DECORATE-style argument list (`"Foo", 1, true`) into tokens.
/// Parse `Offset(x,y)` from DECORATE state modifiers (BD1 D1).
fn offset_from_modifiers(modifiers: &[String]) -> (i32, i32) {
    for m in modifiers {
        let t = m.trim();
        let Some(rest) = t
            .strip_prefix("Offset(")
            .or_else(|| t.strip_prefix("offset("))
        else {
            continue;
        };
        let Some(inner) = rest.strip_suffix(')') else {
            continue;
        };
        let mut parts = inner.split(',');
        let x = parts
            .next()
            .and_then(|s| s.trim().trim_matches('"').parse().ok())
            .unwrap_or(0);
        let y = parts
            .next()
            .and_then(|s| s.trim().trim_matches('"').parse().ok())
            .unwrap_or(0);
        return (x, y);
    }
    (0, 0)
}

fn split_decorate_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in raw.chars() {
        match ch {
            '"' if !in_str => in_str = true,
            '"' if in_str => in_str = false,
            ',' if !in_str => {
                let t = cur.trim().to_owned();
                if !t.is_empty() {
                    out.push(t);
                }
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    let t = cur.trim().to_owned();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::ClassRegistry;
    use room_decorate::parse_decorate;

    #[test]
    fn import_script_states_spawn_loop_and_action() {
        use room_script::RegCall;
        use room_script_abi::state_flow_kind;
        let ir = parse_decorate(
            r#"
actor Actor 0 { States { Spawn: TNT1 A -1 Stop } }
actor ZombieMan : Actor 3004 { States { Spawn: POSS A 10 Loop } }
"#,
        )
        .unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let mut table = StateTable::from_decorate(&ir, &classes);
        let cid = classes.find_name("ZombieMan").unwrap();
        // Replace Spawn with a two-frame Look loop from a module.
        let calls = vec![
            RegCall::StateLabel {
                label: "Spawn".into(),
            },
            RegCall::State {
                sprite: "POSS".into(),
                frame: b'A',
                tics: 10,
                flags: 0,
                callback_id: -1,
                state_ref: 1000,
            },
            RegCall::StateAction {
                name: "A_Look".into(),
                args: String::new(),
            },
            RegCall::State {
                sprite: "POSS".into(),
                frame: b'B',
                tics: 10,
                flags: 0,
                callback_id: -1,
                state_ref: 1001,
            },
            RegCall::StateAction {
                name: "A_Look".into(),
                args: String::new(),
            },
            RegCall::StateFlow {
                kind: state_flow_kind::LOOP,
                label: String::new(),
                offset: 0,
            },
        ];
        table
            .import_script_states(&classes, cid, 0, &calls)
            .unwrap();
        let spawn = table.find_label(cid, "Spawn").unwrap();
        let a = table.get(spawn).unwrap();
        assert_eq!(a.action.as_deref(), Some("A_Look"));
        assert_eq!(a.guest_ref, 1000);
        let b = table.get(a.next.unwrap()).unwrap();
        assert_eq!(b.frame, 1);
        assert_eq!(b.next.unwrap(), spawn);
    }

    #[test]
    fn import_script_states_unresolved_goto_errors() {
        use room_script::RegCall;
        use room_script_abi::state_flow_kind;
        let ir = parse_decorate("actor Actor 0 { States { Spawn: TNT1 A -1 Stop } }").unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let mut table = StateTable::from_decorate(&ir, &classes);
        let cid = classes.find_name("Actor").unwrap();
        let calls = vec![
            RegCall::StateLabel {
                label: "Spawn".into(),
            },
            RegCall::State {
                sprite: "TNT1".into(),
                frame: b'A',
                tics: 1,
                flags: 0,
                callback_id: -1,
                state_ref: 1000,
            },
            RegCall::StateFlow {
                kind: state_flow_kind::GOTO,
                label: "Missing".into(),
                offset: 0,
            },
        ];
        let err = table
            .import_script_states(&classes, cid, 0, &calls)
            .unwrap_err();
        assert!(err.contains("unresolved"), "{err}");
    }

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
    fn weapon_lightdone_goto_stops_not_fallthrough() {
        let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let t = StateTable::from_decorate(&ir, &classes);
        let cg = classes.find_name("Chaingun").unwrap();
        let flash = t.find_label(cg, "Flash").unwrap();
        let a = t.get(flash).unwrap();
        assert_eq!(a.sprite.to_ascii_uppercase(), "CHGF");
        assert_eq!(a.frame, 0);
        // Goto LightDone → Weapon.LightDone (A_Light0), not CHGF B.
        let next = a.next.expect("flash A must Goto LightDone");
        let ld = t.get(next).unwrap();
        assert_eq!(ld.action.as_deref(), Some("A_Light0"));
        assert!(ld.next.is_none(), "LightDone must Stop");
        // Offset 1 is the second flash segment (CHGF B).
        let b = t.get(StateId::from_raw(flash.raw() + 1)).unwrap();
        assert_eq!(b.sprite.to_ascii_uppercase(), "CHGF");
        assert_eq!(b.frame, 1);
        let ld2 = t.get(b.next.expect("flash B Goto")).unwrap();
        assert_eq!(ld2.action.as_deref(), Some("A_Light0"));
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

//! Apply a parsed patch onto [`ClassDefIr`].

use crate::error::DehackedError;
use crate::parse::DehackedPatch;
use crate::tables;
use room_decorate::{ClassDefIr, FlagOp, PropertyStmt};

/// Apply thing / weapon / ammo / misc / frame / codeptr patches onto DECORATE IR.
///
/// Frame / Pointer / `[CODEPTR]` application uses a bootstrap vanilla-frame map
/// (ZombieMan S_POSS_*); full `states.txt` lands in `room.pk3/dehacked/`.
pub fn apply_patch(ir: &mut ClassDefIr, patch: &DehackedPatch) -> Result<(), DehackedError> {
    for thing in &patch.things {
        let class_name = thing
            .name
            .clone()
            .or_else(|| tables::thing_class_name(thing.index))
            .ok_or_else(|| {
                DehackedError::Message(format!(
                    "Thing {} has no name and no vanilla mapping",
                    thing.index
                ))
            })?;
        ensure_actor(ir, &class_name);
        let Some(actor) = ir
            .actors
            .iter_mut()
            .find(|a| a.name.eq_ignore_ascii_case(&class_name))
        else {
            continue;
        };
        if let Some(hp) = thing.hit_points {
            set_prop(actor, "Health", &[hp.to_string()]);
        }
        if let Some(w) = thing.width {
            // DeHackEd Width is diameter; Doom radius = width/2.
            set_prop(actor, "Radius", &[(w / 2).to_string()]);
        }
        if let Some(h) = thing.height {
            set_prop(actor, "Height", &[h.to_string()]);
        }
        if let Some(s) = thing.speed {
            set_prop(actor, "Speed", &[s.to_string()]);
        }
        if let Some(m) = thing.mass {
            set_prop(actor, "Mass", &[m.to_string()]);
        }
        if let Some(r) = thing.reaction_time {
            set_prop(actor, "ReactionTime", &[r.to_string()]);
        }
        if let Some(p) = thing.pain_chance {
            set_prop(actor, "PainChance", &[p.to_string()]);
        }
        if let Some(d) = thing.damage {
            set_prop(actor, "Damage", &[d.to_string()]);
        }
        if let Some(dn) = thing.doomednum {
            actor.doomednum = Some(dn);
        }
        if let Some(bits) = thing.bits {
            apply_bits(actor, bits);
        }
        if let Some(bits) = thing.mbf21_bits {
            apply_mbf21_bits(actor, bits);
        }
        if let Some(s) = thing.fast_speed {
            set_prop(actor, "FastSpeed", &[s.to_string()]);
        }
        if let Some(r) = thing.melee_range {
            set_prop(actor, "MeleeRange", &[r.to_string()]);
        }
        if let Some(g) = thing.infighting_group {
            set_prop(actor, "InfightingGroup", &[g.to_string()]);
        }
        if let Some(g) = thing.projectile_group {
            set_prop(actor, "ProjectileGroup", &[g.to_string()]);
        }
        if let Some(g) = thing.splash_group {
            set_prop(actor, "SplashGroup", &[g.to_string()]);
        }
        apply_thing_sound(actor, patch, "SeeSound", thing.alert_sound);
        apply_thing_sound(actor, patch, "AttackSound", thing.attack_sound);
        apply_thing_sound(actor, patch, "PainSound", thing.pain_sound);
        apply_thing_sound(actor, patch, "DeathSound", thing.death_sound);
        apply_thing_sound(actor, patch, "ActiveSound", thing.action_sound);
    }
    for weapon in &patch.weapons {
        let Some(class_name) = tables::weapon_name(weapon.index) else {
            continue;
        };
        let Some(actor) = ir
            .actors
            .iter_mut()
            .find(|a| a.name.eq_ignore_ascii_case(class_name))
        else {
            continue;
        };
        if let Some(ammo_idx) = weapon.ammo_type {
            if let Some(ammo_name) = tables::ammo_name(ammo_idx) {
                set_prop(actor, "Weapon.AmmoType", &[ammo_name.to_owned()]);
            }
        }
        if let Some(n) = weapon.ammo_per_shot {
            set_prop(actor, "Weapon.AmmoUse", &[n.to_string()]);
        }
    }
    for ammo in &patch.ammos {
        let Some(class_name) = tables::ammo_name(ammo.index) else {
            continue;
        };
        let Some(actor) = ir
            .actors
            .iter_mut()
            .find(|a| a.name.eq_ignore_ascii_case(class_name))
        else {
            continue;
        };
        if let Some(max) = ammo.max_ammo {
            set_prop(actor, "Inventory.MaxAmount", &[max.to_string()]);
        }
        if let Some(per) = ammo.per_ammo {
            set_prop(actor, "Inventory.Amount", &[per.to_string()]);
        }
    }
    if let Some(misc) = &patch.misc {
        // Initial health / max health land on the player pawn classes.
        for name in ["DoomPlayer", "PlayerPawn"] {
            let Some(actor) = ir
                .actors
                .iter_mut()
                .find(|a| a.name.eq_ignore_ascii_case(name))
            else {
                continue;
            };
            if let Some(hp) = misc.initial_health {
                set_prop(actor, "Health", &[hp.to_string()]);
            }
            if let Some(max) = misc.max_health {
                // Soulsphere / health pickups often key off MaxAmount; keep as Health cap hint.
                set_prop(actor, "Inventory.MaxAmount", &[max.to_string()]);
            }
            if let Some(bullets) = misc.initial_bullets {
                set_prop(actor, "StartBullets", &[bullets.to_string()]);
            }
        }
    }
    // BEX `[STRINGS]`: map common GOT* mnemonics onto Inventory.PickupMessage.
    for (key, value) in &patch.strings {
        let Some(class_name) = string_pickup_class(key) else {
            continue;
        };
        let Some(actor) = ir
            .actors
            .iter_mut()
            .find(|a| a.name.eq_ignore_ascii_case(class_name))
        else {
            continue;
        };
        set_prop(
            actor,
            "Inventory.PickupMessage",
            std::slice::from_ref(value),
        );
    }
    apply_frame_patches(ir, patch);
    apply_bex_sprites(ir, patch);
    Ok(())
}

fn apply_thing_sound(
    actor: &mut room_decorate::ActorDef,
    patch: &DehackedPatch,
    prop: &str,
    index: Option<i32>,
) {
    let Some(idx) = index else {
        return;
    };
    let Some(logical) = tables::sound_name(idx) else {
        if idx <= 0 {
            set_prop(actor, prop, &[String::new()]);
        }
        return;
    };
    let name = patch
        .bex_sounds
        .get(logical)
        .cloned()
        .unwrap_or_else(|| logical.to_owned());
    set_prop(actor, prop, &[name]);
}

fn apply_bex_sprites(ir: &mut ClassDefIr, patch: &DehackedPatch) {
    if patch.bex_sprites.is_empty() {
        return;
    }
    for actor in &mut ir.actors {
        let Some(states) = actor.states.as_mut() else {
            continue;
        };
        for item in &mut states.items {
            let room_decorate::StateItem::Line(line) = item else {
                continue;
            };
            if let Some(repl) = patch.bex_sprites.get(line.sprite.as_str()) {
                line.sprite = repl.clone();
            }
        }
    }
}

/// Resolve vanilla `states[]` index → `(class, states.items index)` via `states.txt`.
///
/// Multi-letter DECORATE lines count as multiple vanilla frames; the offset lands on
/// the **containing** compressed line (apply edits that line until 1:1 expansion).
fn resolve_frame_item(ir: &ClassDefIr, frame: i32) -> Option<(String, usize)> {
    let (class, label, offset) = tables::frame_target(frame)?;
    let item_idx = find_label_offset_item(ir, class, label, offset)?;
    Some((class.to_owned(), item_idx))
}

fn find_label_offset_item(
    ir: &ClassDefIr,
    class: &str,
    label: &str,
    offset: usize,
) -> Option<usize> {
    let actor = ir
        .actors
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(class))?;
    let states = actor.states.as_ref()?;
    // StateMap offsets walk the flattened frame stream from `label` onward
    // (across later DECORATE labels), matching GZDoom NextState chaining.
    let mut started = false;
    let mut off = 0usize;
    for (i, item) in states.items.iter().enumerate() {
        match item {
            room_decorate::StateItem::Label(l) if !started => {
                started = l.eq_ignore_ascii_case(label);
            }
            room_decorate::StateItem::Line(line) if started => {
                let n = line.frames.chars().count().max(1);
                if offset < off + n {
                    return Some(i);
                }
                off += n;
            }
            _ => {}
        }
    }
    None
}

fn state_line_mut<'a>(
    ir: &'a mut ClassDefIr,
    class: &str,
    item_idx: usize,
) -> Option<&'a mut room_decorate::StateLine> {
    let actor = ir
        .actors
        .iter_mut()
        .find(|a| a.name.eq_ignore_ascii_case(class))?;
    let states = actor.states.as_mut()?;
    match states.items.get_mut(item_idx)? {
        room_decorate::StateItem::Line(line) => Some(line),
        _ => None,
    }
}

fn read_state_action(ir: &ClassDefIr, class: &str, item_idx: usize) -> Option<String> {
    let actor = ir
        .actors
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(class))?;
    let states = actor.states.as_ref()?;
    match states.items.get(item_idx)? {
        room_decorate::StateItem::Line(line) => line.action.clone(),
        _ => None,
    }
}

fn apply_frame_patches(ir: &mut ClassDefIr, patch: &DehackedPatch) {
    for frame in &patch.frames {
        let Some((class, item_idx)) = resolve_frame_item(ir, frame.index) else {
            continue;
        };
        let Some(line) = state_line_mut(ir, &class, item_idx) else {
            continue;
        };
        if let Some(d) = frame.duration {
            line.duration = d;
        }
        if let Some(spr) = frame.sprite_number {
            if let Some(name) = tables::sprite_name(spr) {
                let mapped = patch
                    .bex_sprites
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.to_owned());
                line.sprite = mapped;
            }
        }
        if let Some(sub) = frame.sprite_subnumber {
            let bright = sub & 0x8000 != 0;
            let letter = (sub & 0x7fff) as u8;
            if letter < 26 {
                line.frames = ((b'A' + letter) as char).to_string();
            }
            if bright
                && !line
                    .modifiers
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case("Bright"))
            {
                line.modifiers.push("Bright".into());
            }
        }
        if let Some(src) = frame.codep_frame {
            if let Some(action) = patch.codeptrs.get(&src) {
                let a = normalize_action(action);
                if a.is_empty() {
                    line.action = None;
                } else {
                    line.action = Some(a);
                }
            }
        }
    }
    for (frame_idx, mnemonic) in &patch.codeptrs {
        let Some((class, item_idx)) = resolve_frame_item(ir, *frame_idx) else {
            continue;
        };
        let Some(line) = state_line_mut(ir, &class, item_idx) else {
            continue;
        };
        let a = normalize_action(mnemonic);
        if a.is_empty() {
            line.action = None;
        } else {
            line.action = Some(a);
        }
    }
    for ptr in &patch.pointers {
        let Some((class, item_idx)) = resolve_frame_item(ir, ptr.frame) else {
            continue;
        };
        let Some(src) = ptr.codep_frame else {
            continue;
        };
        let action = patch.codeptrs.get(&src).cloned().or_else(|| {
            let (sc, si) = resolve_frame_item(ir, src)?;
            read_state_action(ir, &sc, si)
        });
        let Some(action) = action else {
            continue;
        };
        let Some(line) = state_line_mut(ir, &class, item_idx) else {
            continue;
        };
        let a = normalize_action(&action);
        if a.is_empty() {
            line.action = None;
        } else {
            line.action = Some(a);
        }
    }
}

fn normalize_action(mnemonic: &str) -> String {
    let m = mnemonic.trim();
    if m.is_empty() || m.eq_ignore_ascii_case("NULL") {
        return String::new();
    }
    if let Some(mapped) = tables::codepointer_action(m) {
        return mapped.to_owned();
    }
    if m.starts_with("A_") || m.starts_with("a_") {
        return m.to_owned();
    }
    format!("A_{m}")
}

/// BEX string mnemonic → actor that owns that pickup message (M5 subset).
fn string_pickup_class(mnemonic: &str) -> Option<&'static str> {
    match mnemonic.to_ascii_uppercase().as_str() {
        "GOTARMOR" => Some("GreenArmor"),
        "GOTMEGA" => Some("BlueArmor"),
        "GOTSTIM" => Some("Stimpack"),
        // GOTMEDINEED is health-gated at pickup time; only the normal message is IR data.
        "GOTMEDIKIT" => Some("Medikit"),
        "GOTCLIP" => Some("Clip"),
        "GOTCLIPBOX" => Some("ClipBox"),
        "GOTSHELLS" => Some("Shell"),
        "GOTSHELLBOX" => Some("ShellBox"),
        "GOTROCKET" => Some("RocketAmmo"),
        "GOTROCKBOX" => Some("RocketBox"),
        "GOTCELL" => Some("Cell"),
        "GOTCELLBOX" => Some("CellPack"),
        "GOTHEALTHBONUS" | "GOTHTHBONUS" => Some("HealthBonus"),
        "GOTARMBONUS" => Some("ArmorBonus"),
        "GOTSUPER" => Some("Soulsphere"),
        "GOTMSPHERE" => Some("Megasphere"),
        "GOTINVUL" => Some("InvulnerabilitySphere"),
        "GOTBERSERK" => Some("Berserk"),
        "GOTINVIS" => Some("BlurSphere"),
        "GOTSUIT" => Some("RadSuit"),
        "GOTMAP" => Some("Allmap"),
        "GOTVISOR" => Some("Infrared"),
        "GOTBACKPACK" => Some("Backpack"),
        "GOTSHOTGUN" => Some("Shotgun"),
        "GOTSHOTGUN2" => Some("SuperShotgun"),
        "GOTCHAINGUN" => Some("Chaingun"),
        "GOTLAUNCHER" => Some("RocketLauncher"),
        "GOTPLASMA" => Some("PlasmaRifle"),
        "GOTBFG9000" => Some("BFG9000"),
        "GOTCHAINSAW" => Some("Chainsaw"),
        "GOTBLUECARD" => Some("BlueCard"),
        "GOTYELWCARD" => Some("YellowCard"),
        "GOTREDCARD" => Some("RedCard"),
        "GOTBLUESKUL" => Some("BlueSkull"),
        "GOTYELWSKUL" => Some("YellowSkull"),
        "GOTREDSKUL" | "GOTREDSKULL" => Some("RedSkull"),
        _ => None,
    }
}

fn set_prop(actor: &mut room_decorate::ActorDef, name: &str, args: &[String]) {
    if let Some(p) = actor
        .properties
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(name))
    {
        p.args = args.to_vec();
    } else {
        actor.properties.push(PropertyStmt {
            name: name.to_owned(),
            args: args.to_vec(),
        });
    }
}

/// Ensure a class exists for DSDHacked / free DeHackEd slots (`Deh_Actor_N`).
fn ensure_actor(ir: &mut ClassDefIr, class_name: &str) {
    if ir
        .actors
        .iter()
        .any(|a| a.name.eq_ignore_ascii_case(class_name))
    {
        return;
    }
    use room_decorate::{ActorDef, StateFlow, StateItem, StateLine, StatesBlock};
    ir.actors.push(ActorDef {
        name: class_name.to_owned(),
        parent: Some("Actor".into()),
        replaces: None,
        doomednum: None,
        flags: Vec::new(),
        properties: vec![PropertyStmt {
            name: "Health".into(),
            args: vec!["1000".into()],
        }],
        states: Some(StatesBlock {
            items: vec![
                StateItem::Label("Spawn".into()),
                StateItem::Line(StateLine {
                    sprite: "TNT1".into(),
                    frames: "A".into(),
                    duration: -1,
                    modifiers: Vec::new(),
                    action: None,
                    action_args: Vec::new(),
                }),
                StateItem::Flow(StateFlow::Stop),
            ],
        }),
    });
}

fn apply_bits(actor: &mut room_decorate::ActorDef, bits: u32) {
    // Clear SOLID/SHOOTABLE then set from bits.
    actor
        .flags
        .retain(|f| f.name != "SOLID" && f.name != "SHOOTABLE");
    if bits & 1 != 0 {
        actor.flags.push(FlagOp {
            set: true,
            name: "SOLID".into(),
        });
    }
    if bits & 2 != 0 {
        actor.flags.push(FlagOp {
            set: true,
            name: "SHOOTABLE".into(),
        });
    }
}

/// MBF21 thing bits → DECORATE flags / properties (subset with sim support).
fn apply_mbf21_bits(actor: &mut room_decorate::ActorDef, bits: u32) {
    const NORADIUSDMG: u32 = 1 << 3;
    const BOSS: u32 = 1 << 9;
    const LOGRAV: u32 = 1 << 0;
    const NOTHRESHOLD: u32 = 1 << 7;
    if bits & NORADIUSDMG != 0 {
        push_flag(actor, "NORADIUSDMG");
    }
    if bits & BOSS != 0 {
        push_flag(actor, "BOSS");
    }
    if bits & LOGRAV != 0 {
        set_prop(actor, "Gravity", &["0.125".into()]);
    }
    if bits & NOTHRESHOLD != 0 {
        set_prop(actor, "Threshold", &["0".into()]);
        set_prop(actor, "DefThreshold", &["0".into()]);
    }
}

fn push_flag(actor: &mut room_decorate::ActorDef, name: &str) {
    actor.flags.retain(|f| f.name != name);
    actor.flags.push(FlagOp {
        set: true,
        name: name.into(),
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::parse_dehacked;
    use room_decorate::parse_decorate;

    #[test]
    fn allocates_dsdhacked_deh_actor_slot() {
        let mut ir = ClassDefIr::default();
        let patch = parse_dehacked(
            r#"
Thing 300 (CustomThing)
Hit points = 50
ID # = 9001
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = ir
            .actors
            .iter()
            .find(|a| a.name == "CustomThing" || a.name == "Deh_Actor_300")
            .unwrap();
        // Named header wins over Deh_Actor_N.
        assert_eq!(a.name, "CustomThing");
        assert_eq!(a.doomednum, Some(9001));
        let hp = a
            .properties
            .iter()
            .find(|p| p.name == "Health")
            .unwrap()
            .args[0]
            .parse::<i32>()
            .unwrap();
        assert_eq!(hp, 50);
    }

    #[test]
    fn allocates_deh_actor_when_index_unknown() {
        let mut ir = ClassDefIr::default();
        let patch = parse_dehacked(
            r#"
Thing 300
Hit points = 77
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = ir
            .actors
            .iter()
            .find(|a| a.name == "Deh_Actor_300")
            .unwrap();
        assert_eq!(
            a.properties
                .iter()
                .find(|p| p.name == "Health")
                .unwrap()
                .args[0],
            "77"
        );
    }

    #[test]
    fn applies_mbf21_fast_speed_and_bits() {
        let mut ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Speed 8
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Thing 2 (ZombieMan)
Fast speed = 16
Melee range = 96
MBF21 Bits = NORADIUSDMG+BOSS
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = &ir.actors[0];
        assert_eq!(
            a.properties
                .iter()
                .find(|p| p.name == "FastSpeed")
                .unwrap()
                .args[0],
            "16"
        );
        assert_eq!(
            a.properties
                .iter()
                .find(|p| p.name == "MeleeRange")
                .unwrap()
                .args[0],
            "96"
        );
        assert!(a.flags.iter().any(|f| f.name == "NORADIUSDMG" && f.set));
        assert!(a.flags.iter().any(|f| f.name == "BOSS" && f.set));
    }

    #[test]
    fn applies_health_to_zombieman() {
        let mut ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SOLID
  +SHOOTABLE
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Thing 2 (ZombieMan)
Hit points = 40
Width = 40
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = &ir.actors[0];
        let hp = a
            .properties
            .iter()
            .find(|p| p.name == "Health")
            .unwrap()
            .args[0]
            .parse::<i32>()
            .unwrap();
        assert_eq!(hp, 40);
        let rad = a
            .properties
            .iter()
            .find(|p| p.name == "Radius")
            .unwrap()
            .args[0]
            .parse::<i32>()
            .unwrap();
        assert_eq!(rad, 20); // width 40 → radius 20
    }

    #[test]
    fn applies_speed_and_maps_archvile_index() {
        let mut ir = parse_decorate(
            r#"
actor Archvile : Actor 64
{
  Health 700
  Speed 15
  PainChance 10
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Thing 4
Hit points = 800
Speed = 20
Pain chance = 50
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = &ir.actors[0];
        assert_eq!(prop_i32(a, "Health"), 800);
        assert_eq!(prop_i32(a, "Speed"), 20);
        assert_eq!(prop_i32(a, "PainChance"), 50);
    }

    fn prop_i32(a: &room_decorate::ActorDef, name: &str) -> i32 {
        a.properties.iter().find(|p| p.name == name).unwrap().args[0]
            .parse()
            .unwrap()
    }

    #[test]
    fn applies_bex_strings_to_pickup_message() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let mut ir = parse_decorate(src).unwrap();
        let patch = parse_dehacked(
            r#"
[STRINGS]
GOTARMOR = Custom green armor text.
GOTCLIP = Custom clip text.
GOTCHAINGUN = Custom chaingun text.
UNKNOWNMNEMONIC = ignored
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        assert_eq!(
            prop_str(
                ir.actors
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case("GreenArmor"))
                    .unwrap(),
                "Inventory.PickupMessage"
            ),
            "Custom green armor text."
        );
        assert_eq!(
            prop_str(
                ir.actors
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case("Clip"))
                    .unwrap(),
                "Inventory.PickupMessage"
            ),
            "Custom clip text."
        );
        assert_eq!(
            prop_str(
                ir.actors
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case("Chaingun"))
                    .unwrap(),
                "Inventory.PickupMessage"
            ),
            "Custom chaingun text."
        );
    }

    #[test]
    fn patches_doom_basic_zombieman_by_index() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let mut ir = parse_decorate(src).unwrap();
        let before = ir
            .actors
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("ZombieMan"))
            .map(prop_i32_health)
            .unwrap();
        let patch = parse_dehacked(
            r#"
Thing 2
Hit points = 99
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let after = ir
            .actors
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("ZombieMan"))
            .map(prop_i32_health)
            .unwrap();
        assert_eq!(before, 20);
        assert_eq!(after, 99);
    }

    fn prop_i32_health(a: &room_decorate::ActorDef) -> i32 {
        prop_i32(a, "Health")
    }

    fn prop_str(a: &room_decorate::ActorDef, name: &str) -> String {
        a.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.args.first().cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    #[test]
    fn applies_weapon_ammo_and_misc() {
        let mut ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
}
actor Shotgun : Actor 2001
{
  Weapon.AmmoType "Shell"
  Weapon.AmmoUse 1
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Weapon 2
Ammo type = 0
Ammo per shot = 2

Ammo 0
Max ammo = 400
Per ammo = 25

Misc 0
Initial Health = 150
Max Health = 250
Initial Bullets = 75
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let shotgun = ir.actors.iter().find(|a| a.name == "Shotgun").unwrap();
        assert_eq!(prop_str(shotgun, "Weapon.AmmoType"), "Clip");
        assert_eq!(prop_i32(shotgun, "Weapon.AmmoUse"), 2);
        let clip = ir.actors.iter().find(|a| a.name == "Clip").unwrap();
        assert_eq!(prop_i32(clip, "Inventory.MaxAmount"), 400);
        assert_eq!(prop_i32(clip, "Inventory.Amount"), 25);
        let pawn = ir.actors.iter().find(|a| a.name == "PlayerPawn").unwrap();
        assert_eq!(prop_i32(pawn, "Health"), 150);
        assert_eq!(prop_i32(pawn, "Inventory.MaxAmount"), 250);
        assert_eq!(prop_i32(pawn, "StartBullets"), 75);
    }

    #[test]
    fn applies_frame_duration_and_codeptr() {
        let mut ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  states
  {
  Spawn:
    POSS AB 10 A_Look
    Loop
  See:
    POSS AABBCCDD 4 A_Chase
    Loop
  Missile:
    POSS E 10 A_FaceTarget
    POSS F 8 A_PosAttack
    POSS E 8
    Goto See
  }
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Frame 174
Duration = 5

[CODEPTR]
FRAME 176 = Wander
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let actor = ir.actors.iter().find(|a| a.name == "ZombieMan").unwrap();
        let states = actor.states.as_ref().unwrap();
        let lines: Vec<_> = states
            .items
            .iter()
            .filter_map(|i| match i {
                room_decorate::StateItem::Line(l) => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines[0].duration, 5);
        assert_eq!(lines[1].action.as_deref(), Some("A_Wander"));
    }

    #[test]
    fn applies_frame_via_states_txt_shotgunguy() {
        // ShotgunGuy Spawn starts at vanilla frame 207 (`states.txt`).
        let mut ir = parse_decorate(
            r#"
actor ShotgunGuy : Actor 9
{
  states
  {
  Spawn:
    SPOS AB 10 A_Look
    Loop
  See:
    SPOS AABBCCDD 3 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Frame 207
Duration = 7
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let actor = ir.actors.iter().find(|a| a.name == "ShotgunGuy").unwrap();
        let line = match &actor.states.as_ref().unwrap().items[1] {
            room_decorate::StateItem::Line(l) => l,
            _ => panic!("expected Spawn line"),
        };
        assert_eq!(line.duration, 7);
    }

    #[test]
    fn applies_thing_sounds_and_bex_sprites() {
        let mut ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  SeeSound "grunt/sight"
  AttackSound "grunt/attack"
  states
  {
  Spawn:
    POSS A 10
    Loop
  }
}
"#,
        )
        .unwrap();
        let patch = parse_dehacked(
            r#"
Thing 2
Alert sound = 1
Attack sound = 0

Frame 174
Sprite number = 0

[SPRITES]
TROO = SARG
[SOUNDS]
weapons/pistol = custom/pistol
"#,
        )
        .unwrap();
        apply_patch(&mut ir, &patch).unwrap();
        let a = &ir.actors[0];
        assert_eq!(prop_str(a, "SeeSound"), "custom/pistol");
        assert_eq!(prop_str(a, "AttackSound"), "");
        let line = match &a.states.as_ref().unwrap().items[1] {
            room_decorate::StateItem::Line(l) => l,
            _ => panic!("expected state line"),
        };
        assert_eq!(line.sprite, "SARG");
    }
}

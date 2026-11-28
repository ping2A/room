//! ZS3 / ZS4 play-integration smoke (`docs/34`).

// Test helpers: a failed setup step should panic the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use room_decorate::parse_decorate;
use room_nodebuild::apply_nodes;
use room_sim::{
    ClassRegistry, StateTable, World, ZscriptRuntime, count_inventory, damage_mobj, give_inventory,
};
use room_testkit::{MapBuilder, rect};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

fn load_bd_like() -> room_zvm::CompiledProgram {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/zscript/bd_like");
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("bd_like dir") {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            e.eq_ignore_ascii_case("zs")
                || e.eq_ignore_ascii_case("zc")
                || e.eq_ignore_ascii_case("zsc")
        }) {
            let text = std::fs::read_to_string(&p).unwrap();
            sources.push((p.display().to_string(), text));
        }
    }
    sources.sort_by(|a, b| {
        let score = |n: &str| {
            let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
            if base.eq_ignore_ascii_case("zscript.zc") {
                0
            } else {
                1
            }
        };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
    let result = room_zscript::check(&sources);
    assert!(
        result.ok(),
        "bd_like check failed: {:?}",
        result
            .diagnostics
            .iter()
            .filter(|d| d.severity == room_zscript::Severity::Error)
            .take(5)
            .collect::<Vec<_>>()
    );
    result.program.expect("compiled program")
}

#[test]
fn zs4_zandrocompat_lump_open_via_script_call() {
    let prog = load_bd_like();
    let mut rt = ZscriptRuntime::default();
    rt.load_program(prog);
    let mut lumps = BTreeMap::new();
    lumps.insert("maps/map01.wad".into(), "MAPHASHDATA".into());
    rt.set_wad_lumps(lumps);
    rt.sync_host(3, 0, 0);
    let lump = rt
        .script_call_with("ZandroCompat.LumpOpen", &[], &["maps/map01.wad"])
        .expect("LumpOpen");
    assert_eq!(lump, 0, "first lump index");
    let n = rt
        .script_call("ZandroCompat.ConsolePlayerNumber")
        .expect("ConsolePlayerNumber");
    assert_eq!(n, 0);
}

#[test]
fn zs4_bd_hash_lump_read_mid_length() {
    // BD_Hash-shaped path: ReadLump → Mid → Length (docs/34 ZS4 / docs/33 BD6).
    let src = r#"
class BdHashProbe
{
    static int SliceLen(int lump, int offset, int len)
    {
        string data = Wads.ReadLump(lump);
        return data.Mid(offset, len).Length();
    }
}
"#;
    let result = room_zscript::check(&[("bd_hash_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    let mut lumps = BTreeMap::new();
    lumps.insert("maps/map01.wad".into(), "ABCDEFGHIJ".into());
    rt.set_wad_lumps(lumps);
    rt.sync_host(0, 0, 0);
    let lump = rt
        .script_call_with("Wads.CheckNumForFullName", &[], &["maps/map01.wad"])
        .unwrap_or({
            // Fallback: lump index 0 when CheckNum is a native returning i via host.
            0
        });
    let _ = lump;
    let n = rt
        .script_call_with("BdHashProbe.SliceLen", &[0, 3, 4], &[])
        .expect("SliceLen");
    assert_eq!(n, 4, "Mid(3,4) of ABCDEFGHIJ → DEFG len 4; got {n}");
}

#[test]
fn zs4_mapinfo_addeventhandlers_wires_runtime() {
    let info = room_defs::parse_mapinfo(
        r#"
        gameinfo { AddEventHandlers = "Script_DamageMult", "ZC_TMapHack" }
    "#,
    )
    .expect("mapinfo");
    assert_eq!(info.event_handlers.len(), 2);
    let mut rt = ZscriptRuntime::default();
    rt.set_event_handlers(info.event_handlers);
    assert_eq!(rt.event_handlers[0], "Script_DamageMult");
    assert_eq!(rt.event_handlers[1], "ZC_TMapHack");
}

#[test]
fn zs4_activate_map_handlers_onregister_and_worldloaded() {
    let src = r#"
class LoadProbe : EventHandler
{
    int ready;
    override void OnRegister()
    {
        ready = 1;
    }
    override void WorldLoaded(WorldEvent e)
    {
        Console.Printf("worldloaded");
        ready = ready + 1;
    }
}
"#;
    let result = room_zscript::check(&[("load_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    rt.set_event_handlers(["LoadProbe".into()]);
    let _ = rt.take_printf();
    rt.activate_map_handlers().expect("activate");
    rt.activate_map_handlers().expect("activate idempotent");
    assert!(rt.map_handlers_active);
    assert!(rt.handler_objs.contains_key("LoadProbe"));
    let h = rt.handler_objs["LoadProbe"];
    let ready = match rt.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.first().copied().unwrap_or(0),
        _ => 0,
    };
    assert_eq!(ready, 2, "OnRegister sets 1, WorldLoaded increments");
    let msgs = rt.take_printf();
    assert!(
        msgs.iter().any(|m| m.contains("worldloaded")),
        "WorldLoaded should Printf; got {msgs:?}"
    );
}

#[test]
fn zs4_cvar_get_int_reads_host_store() {
    let src = r#"
class CVarProbe
{
    static int Read(string name)
    {
        return CVar.GetCVar(name).GetInt();
    }
}
"#;
    let result = room_zscript::check(&[("cvar_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    let mut cvars = BTreeMap::new();
    cvars.insert("bd_bloodamount".into(), 42);
    rt.set_cvars(cvars);
    let v = rt
        .script_call_with("CVarProbe.Read", &[], &["bd_bloodamount"])
        .expect("Read");
    assert_eq!(v, 42);
}

#[test]
fn zs4_damage_handler_grants_on_skill_ge_6() {
    let prog = load_bd_like();
    let mut rt = ZscriptRuntime::default();
    rt.load_program(prog);
    rt.set_event_handlers(["Script_DamageMult".into()]);
    rt.clear_grants();

    rt.sync_host(5, 0, 0);
    rt.dispatch_handlers("WorldThingSpawned")
        .expect("dispatch skill5");
    assert!(
        rt.take_grants().is_empty(),
        "skill 5 must not grant Damage_Control"
    );

    rt.sync_host(6, 0, 0);
    rt.dispatch_handlers("WorldThingSpawned")
        .expect("dispatch skill6");
    let grants = rt.take_grants();
    assert!(
        grants
            .iter()
            .any(|(_, item, amt)| { item.eq_ignore_ascii_case("Damage_Control") && *amt == 1 }),
        "skill ≥ 6 must GiveInventory Damage_Control; got {grants:?}"
    );
}

#[test]
fn zs3_tick_override_checksum_stable() {
    let src = r#"
class CounterActor
{
    int counter;
    override void Tick()
    {
        counter = counter + 1;
    }
}
"#;
    let result = room_zscript::check(&[("counter.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.expect("program");
    let mut rt = ZscriptRuntime::default();
    rt.load_program(prog);
    rt.call_actor_virtual(1, "CounterActor", "Tick")
        .expect("tick1");
    rt.call_actor_virtual(1, "CounterActor", "Tick")
        .expect("tick2");
    let a = rt.checksum_bytes().expect("checksum");
    let mut rt2 = ZscriptRuntime::default();
    let result2 = room_zscript::check(&[("counter.zs".into(), src.into())]);
    rt2.load_program(result2.program.unwrap());
    rt2.call_actor_virtual(1, "CounterActor", "Tick").unwrap();
    rt2.call_actor_virtual(1, "CounterActor", "Tick").unwrap();
    let b = rt2.checksum_bytes().unwrap();
    assert_eq!(a, b, "Tick checksum must be deterministic across runs");
    let h = rt.actor_fields[&1];
    let fields = match rt.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.clone(),
        _ => panic!("missing object"),
    };
    assert_eq!(fields.first().copied().unwrap_or(0), 2);
}

#[test]
fn zs3_tick_and_postbeginplay_run_in_world_tick() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor CounterZombie : ZombieMan 9901
{
  Health 20
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class CounterZombie
{
    int counter;
    int begun;
    override void PostBeginPlay()
    {
        begun = 1;
    }
    override void Tick()
    {
        counter = counter + 1;
    }
}
"#;
    let result = room_zscript::check(&[("counter_zombie.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let zid = classes.find_name("CounterZombie").unwrap();
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(100.0, 100.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    // PostBeginPlay on spawn.
    let h = w.zscript.actor_fields[&id.index()];
    let begun = match w.zscript.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.get(1).copied().unwrap_or(0),
        _ => 0,
    };
    assert_eq!(begun, 1, "PostBeginPlay should set begun=1 on spawn");
    w.tick();
    w.tick();
    let h = w.zscript.actor_fields[&id.index()];
    let fields = match w.zscript.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.clone(),
        _ => panic!("missing script object"),
    };
    // field order: counter then begun (declaration order).
    assert_eq!(
        fields.first().copied().unwrap_or(0),
        2,
        "Tick ×2 → counter=2; got {fields:?}"
    );
    let zs_a = w.zscript.checksum_bytes().unwrap();
    let world_a = w.checksum();

    // Second identical run must match (ZS3 acceptance).
    let result2 = room_zscript::check(&[("counter_zombie.zs".into(), zs.into())]);
    assert!(result2.ok(), "{:?}", result2.diagnostics);
    let mut map2 = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map2);
    let mut w2 =
        World::from_map_with_classes(map2, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w2.load_zscript_program(result2.program.unwrap());
    let id2 = w2
        .spawn_at(
            zid,
            room_core::math::DVec3::new(100.0, 100.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    let _ = id2;
    w2.tick();
    w2.tick();
    assert_eq!(
        zs_a,
        w2.zscript.checksum_bytes().unwrap(),
        "ZScript heap checksum must match across two world runs"
    );
    assert_eq!(
        world_a,
        w2.checksum(),
        "World checksum must match across two runs with the same ZScript actor"
    );
}

#[test]
fn zs3_compile_captures_states_body() {
    let zs = r#"
class StateProbe : Actor
{
    Default
    {
        Health 88;
        +NOGRAVITY
    }
    States
    {
    Spawn:
        POSS A -1
        Stop
    }
}
"#;
    let result = room_zscript::check(&[("state_probe.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.expect("program");
    let body = prog
        .class_states
        .get("StateProbe")
        .or_else(|| {
            prog.class_states
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("StateProbe"))
                .map(|(_, v)| v)
        })
        .expect("States body stored on program");
    assert!(
        body.to_ascii_uppercase().contains("POSS"),
        "expected POSS in states body, got {body:?}"
    );
    let defs = prog
        .class_defaults
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("StateProbe"))
        .map(|(_, v)| v.as_str())
        .expect("Default body stored on program");
    assert!(
        defs.to_ascii_lowercase().contains("health"),
        "expected Health in defaults, got {defs:?}"
    );
}

#[test]
fn zs3_zscript_states_merge_into_state_table() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor StateMergeZombie : ZombieMan 9902
{
  Health 20
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class StateMergeZombie
{
    States
    {
    Spawn:
        POSS B -1
        Stop
    See:
        POSS AB 4
        Loop
    }
}
"#;
    let result = room_zscript::check(&[("merge.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    let cid = classes.find_name("StateMergeZombie").unwrap();
    // Before merge, See may come from ZombieMan parent; after ZScript merge See must exist.
    w.load_zscript_program(result.program.unwrap());
    assert!(
        w.states.find_label(cid, "See").is_some(),
        "ZScript See label should be in StateTable after merge"
    );
    let spawn = w.states.find_label(cid, "Spawn").expect("Spawn");
    let st = w.states.get(spawn).expect("spawn state");
    assert_eq!(st.frame, b'B' - b'A', "ZScript Spawn should use POSS B");
}

#[test]
fn zs6_save_load_mid_level_restores_script_fields() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor SaveZombie : ZombieMan 9903
{
  Health 20
  States { Spawn: POSS A -1 Stop }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class SaveZombie
{
    int counter;
    override void Tick()
    {
        counter = counter + 1;
    }
}
"#;
    let result = room_zscript::check(&[("save_zombie.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.unwrap();

    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(prog.clone());
    let zid = classes.find_name("SaveZombie").unwrap();
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(120.0, 120.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    w.tick();
    w.tick();
    w.tick();
    let blob = w.zscript.save_blob().expect("save blob");
    let checksum_before = w.zscript.checksum_bytes().unwrap();
    let fields_before = {
        let h = w.zscript.actor_fields[&id.index()];
        match w.zscript.vm.as_ref().unwrap().heap.get(h) {
            Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.clone(),
            _ => panic!("missing"),
        }
    };
    assert_eq!(fields_before.first().copied().unwrap_or(0), 3);

    // Fresh world + same program, restore blob without re-ticking.
    let mut map2 = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map2);
    let mut w2 =
        World::from_map_with_classes(map2, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w2.load_zscript_program(prog);
    w2.zscript
        .load_blob(&blob, false)
        .expect("load blob must accept matching program hash");
    let fields_after = {
        let h = *w2
            .zscript
            .actor_fields
            .get(&id.index())
            .expect("restored actor fields for original id");
        match w2.zscript.vm.as_ref().unwrap().heap.get(h) {
            Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.clone(),
            _ => panic!("missing after load"),
        }
    };
    assert_eq!(fields_after, fields_before);
    assert_eq!(
        w2.zscript.checksum_bytes().unwrap(),
        checksum_before,
        "script checksum after load must match mid-level save"
    );
}

#[test]
fn zs3_default_block_overrides_health_and_flag() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor DefaultZombie : ZombieMan 9906
{
  Health 20
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class DefaultZombie
{
    Default
    {
        Health 42;
        +NOGRAVITY
    }
}
"#;
    let result = room_zscript::check(&[("def.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let zid = classes.find_name("DefaultZombie").unwrap();
    assert_eq!(w.classes.get(zid).unwrap().health, 42);
    assert_ne!(
        w.classes.get(zid).unwrap().flags & room_sim::FLAG_NOGRAVITY,
        0,
        "NOGRAVITY should be set from Default"
    );
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(70.0, 70.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(w.actors.get(id).unwrap().health, 42);
}

#[test]
fn zs3_anonymous_state_function_via_zscript_merge() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor AnonZombie : ZombieMan 9905
{
  Health 20
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class AnonZombie
{
    States
    {
    Spawn:
        TNT1 A 0 NoDelay
        {
            A_SetHealth(77);
        }
        POSS A -1
        Stop
    }
}
"#;
    let result = room_zscript::check(&[("anon.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let zid = classes.find_name("AnonZombie").unwrap();
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(80.0, 80.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    let spawn = w.states.find_label(zid, "Spawn").expect("merged Spawn");
    let st = w.states.get(spawn).expect("spawn state");
    assert_eq!(
        st.action.as_deref(),
        Some("A_SetHealth"),
        "anon brace should compile to A_SetHealth on Spawn"
    );
    // GZDoom: the first spawn frame's action runs in the first tick, and
    // only with `NoDelay` (`CheckNoDelay`).
    w.tick();
    let health = w.actors.get(id).map(|a| a.health).unwrap_or(0);
    assert_eq!(
        health, 77,
        "anonymous A_SetHealth should run via merged States"
    );
}

#[test]
fn zs6_bd9_two_peer_zscript_lockstep_checksum() {
    // Synthetic 2-peer lockstep: identical ZScript + map → identical checksums (ZS6/BD9).
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor PeerZombie : ZombieMan 9904
{
  Health 20
  States { Spawn: POSS A -1 Stop }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class PeerZombie
{
    int counter;
    override void Tick()
    {
        counter = counter + 3;
    }
}
"#;
    let result = room_zscript::check(&[("peer.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.unwrap();
    let zid = classes.find_name("PeerZombie").unwrap();

    let run = |prog: room_zvm::CompiledProgram| {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        w.load_zscript_program(prog);
        let _ = w.spawn_at(
            zid,
            room_core::math::DVec3::new(90.0, 90.0, 0.0),
            room_core::math::Angle::ZERO,
        );
        for _ in 0..16 {
            w.tick();
        }
        (w.checksum(), w.zscript.checksum_bytes().unwrap())
    };

    let (w_a, z_a) = run(prog.clone());
    let (w_b, z_b) = run(prog);
    assert_eq!(z_a, z_b, "ZScript peer checksums must match");
    assert_eq!(w_a, w_b, "World peer checksums must match");
}

/// BD9/ZS6: two peers load commercial BD ZScript + identical map → matching checksums.
#[test]
fn bd9_commercial_zscript_two_peer_checksum_gated() {
    let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
        return;
    };
    if !PathBuf::from(&pk3).is_file() {
        return;
    }
    let mut sources = Vec::new();
    for path in [
        "zscript.zc",
        "zscript/BD_WCS.zc",
        "zscript/DamageHandler.zc",
    ] {
        let out = std::process::Command::new("unzip")
            .args(["-p", &pk3, path])
            .output()
            .expect("unzip");
        assert!(out.status.success() && !out.stdout.is_empty(), "{path}");
        sources.push((path.to_owned(), String::from_utf8(out.stdout).unwrap()));
    }
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.expect("program");

    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let run = |prog: room_zvm::CompiledProgram| {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        w.load_zscript_program(prog);
        w.zscript
            .set_event_handlers(["ZC_TMapHack".into(), "Script_DamageMult".into()]);
        w.zscript.activate_map_handlers().expect("handlers");
        for _ in 0..32 {
            w.tick();
        }
        (w.checksum(), w.zscript.checksum_bytes().unwrap())
    };

    let (w_a, z_a) = run(prog.clone());
    let (w_b, z_b) = run(prog);
    assert_eq!(z_a, z_b, "commercial ZScript peer checksums");
    assert_eq!(w_a, w_b, "commercial ZScript world peer checksums");
}

#[test]
fn bd6_map_enhancement_worldloaded_uses_hash_slice() {
    // docs/33 BD6: BD_Hash Mid → Map Enhancement gate on WorldLoaded (fixture-shaped).
    let src = r#"
class MapEnhancementProbe : EventHandler
{
    int ready;
    override void WorldLoaded(WorldEvent e)
    {
        string data = Wads.ReadLump(0);
        int n = data.Mid(0, 8).Length();
        if (n >= 8)
        {
            ready = n;
            Console.Printf("MES:spawn");
        }
    }
}
"#;
    let result = room_zscript::check(&[("mes_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    let mut lumps = BTreeMap::new();
    lumps.insert("maps/map01.wad".into(), "MAPHASH01XXXX".into());
    rt.set_wad_lumps(lumps);
    rt.sync_host(0, 0, 0);
    rt.set_event_handlers(["MapEnhancementProbe".into()]);
    let _ = rt.take_printf();
    rt.activate_map_handlers().expect("activate");
    let h = rt.handler_objs["MapEnhancementProbe"];
    let ready = match rt.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.first().copied().unwrap_or(0),
        _ => 0,
    };
    assert_eq!(
        ready, 8,
        "MES WorldLoaded should gate on Mid length; got {ready}"
    );
    let msgs = rt.take_printf();
    assert!(
        msgs.iter().any(|m| m.contains("MES:spawn")),
        "expected MES spawn signal; got {msgs:?}"
    );
}

#[test]
fn zs4_event_handler_find_and_object_new() {
    let src = r#"
class StoreHack : EventHandler
{
    int tag;
    override void OnRegister()
    {
        tag = 7;
    }
}
class FindProbe
{
    static int Run()
    {
        let h = EventHandler.Find("StoreHack");
        if (h)
        {
            return 1;
        }
        let o = Object.New("ZC_IntStrMap");
        if (o)
        {
            return 2;
        }
        return 0;
    }
}
"#;
    let result = room_zscript::check(&[("find_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    rt.set_event_handlers(["StoreHack".into()]);
    rt.activate_map_handlers().expect("activate");
    // Direct native path: Find must return the same instance as activate created.
    let h = rt.handler_objs["StoreHack"];
    assert!(!h.is_null());
    // Call Find via a tiny host-side native invoke by dispatching through script_call if compiled.
    // Fallback: publish + call FindProbe.Run — may not compile `let`/casts; use field read instead.
    let tag = match rt.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.first().copied().unwrap_or(0),
        _ => 0,
    };
    assert_eq!(tag, 7, "OnRegister should set tag");
    // Object.New via native table
    let vm = rt.vm.as_mut().unwrap();
    let mut i = [0i64; 4];
    let mut f = [0f64; 2];
    let mut s = [room_zvm::StrHandle::EMPTY; 2];
    let mut a = [room_zvm::ObjHandle::NULL; 2];
    s[0] = vm.heap.intern_string("ZC_IntStrMap");
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        let (_, f_new) = vm.natives.get_named("Object.New").expect("Object.New");
        f_new(&mut ctx, &mut regs).expect("Object.New");
    }
    assert!(!a[0].is_null(), "Object.New must allocate");
    s[0] = vm.heap.intern_string("StoreHack");
    a[0] = room_zvm::ObjHandle::NULL;
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut f,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        let (_, f_find) = vm.natives.get_named("EventHandler.Find").expect("Find");
        f_find(&mut ctx, &mut regs).expect("Find");
    }
    assert_eq!(a[0], h, "EventHandler.Find must return registered handler");
}

#[test]
fn zs4_player_in_game_and_change_tid_natives() {
    let mut rt = ZscriptRuntime::default();
    let mut f = room_zvm::CompiledFunction {
        name: "nop".into(),
        n_i: 1,
        ..Default::default()
    };
    f.emit(room_zvm::Op::Return, 0, 0, 0);
    let mut p = room_zvm::CompiledProgram {
        functions: vec![f],
        ..Default::default()
    };
    p.finalize();
    rt.load_program(p);
    rt.set_players_in_game(BTreeMap::from([(0, true), (1, false)]));
    let vm = rt.vm.as_mut().unwrap();
    let (_, pig) = vm
        .natives
        .get_named("ZandroCompat.PlayerInGame")
        .expect("PlayerInGame");
    let mut i = [1i64; 2];
    let mut fregs = [0f64; 1];
    let mut s = [room_zvm::StrHandle::EMPTY; 1];
    let mut a = [room_zvm::ObjHandle::NULL; 1];
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut fregs,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        pig(&mut ctx, &mut regs).unwrap();
    }
    assert_eq!(i[0], 0, "player 1 not in game");
    i[0] = 0;
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut fregs,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        pig(&mut ctx, &mut regs).unwrap();
    }
    assert_eq!(i[0], 1, "player 0 in game");
    a[0] = vm.heap.new_object(0, 1, 0, 0);
    i[0] = 42;
    let (_, ct) = vm.natives.get_named("Actor.ChangeTID").expect("ChangeTID");
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut fregs,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        ct(&mut ctx, &mut regs).unwrap();
    }
    assert_eq!(i[0], 1);
    let changes = std::mem::take(&mut vm.play.tid_changes);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].1, 42);
}

#[test]
fn zs4_get_set_player_tid_natives() {
    let mut rt = ZscriptRuntime::default();
    let mut f = room_zvm::CompiledFunction {
        name: "nop".into(),
        n_i: 2,
        ..Default::default()
    };
    f.emit(room_zvm::Op::Return, 0, 0, 0);
    let mut p = room_zvm::CompiledProgram {
        functions: vec![f],
        ..Default::default()
    };
    p.finalize();
    rt.load_program(p);
    rt.set_player_tids(BTreeMap::from([(0, 100)]));
    let vm = rt.vm.as_mut().unwrap();
    let mut i = [0i64; 2];
    let mut fregs = [0f64; 1];
    let mut s = [room_zvm::StrHandle::EMPTY; 1];
    let mut a = [room_zvm::ObjHandle::NULL; 1];
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut fregs,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        let (_, get) = vm
            .natives
            .get_named("ZandroCompat.GetPlayerTID")
            .expect("GetPlayerTID");
        get(&mut ctx, &mut regs).unwrap();
    }
    assert_eq!(i[0], 100);
    i[0] = 0;
    i[1] = 777;
    {
        let mut regs = room_zvm::NativeRegs {
            i: &mut i,
            f: &mut fregs,
            s: &mut s,
            a: &mut a,
        };
        let mut ctx = room_zvm::ZvmCtx {
            heap: &mut vm.heap,
            host: 0,
            play: &mut vm.play,
        };
        let (_, set) = vm
            .natives
            .get_named("ZandroCompat.SetPlayerTID")
            .expect("SetPlayerTID");
        set(&mut ctx, &mut regs).unwrap();
    }
    assert_eq!(i[0], 1);
    assert_eq!(vm.play.player_tids.get(&0).copied(), Some(777));
}

#[test]
fn zs6_world_checksum_includes_zscript_fields() {
    // Verifier path: World::checksum folds ZScript heap digest (docs/34 ZS6).
    let zs = r#"
class CkZombie : Actor
{
    int counter;
    override void Tick()
    {
        counter = counter + 1;
    }
}
"#;
    let deco = r#"
actor CkZombie : Actor 9401
{
  Health 20
  Radius 20
  Height 56
  States { Spawn: POSS A -1 Stop }
}
"#;
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let result = room_zscript::check(&[("ck.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let prog = result.program.unwrap();
    let zid = classes.find_name("CkZombie").unwrap();

    let run = |ticks: u32| {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        w.load_zscript_program(prog.clone());
        let _ = w.spawn_at(
            zid,
            room_core::math::DVec3::new(90.0, 90.0, 0.0),
            room_core::math::Angle::ZERO,
        );
        for _ in 0..ticks {
            w.tick();
        }
        w.checksum()
    };
    let a = run(4);
    let b = run(8);
    assert_ne!(
        a, b,
        "more Tick increments must change world checksum via ZScript"
    );
    assert_eq!(run(4), a, "same tic count must agree (verifier-stable)");
}

/// BD6/ZS4: commercial Brutal Doom ZScript grants `Damage_Control` at skill ≥ 6.
#[test]
fn bd6_commercial_damage_control_skill_ge_6_gated() {
    let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
        return;
    };
    if !PathBuf::from(&pk3).is_file() {
        return;
    }
    let mut sources = Vec::new();
    for path in [
        "zscript.zc",
        "zscript/BD_WCS.zc",
        "zscript/DamageHandler.zc",
    ] {
        let out = std::process::Command::new("unzip")
            .args(["-p", &pk3, path])
            .output()
            .expect("unzip");
        if !out.status.success() || out.stdout.is_empty() {
            continue;
        }
        let text = String::from_utf8(out.stdout).expect("utf8");
        sources.push((path.to_owned(), text));
    }
    assert_eq!(sources.len(), 3, "expected 3 BD ZScript files");
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "commercial ZScript: {:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.expect("program"));
    rt.set_event_handlers(["Script_DamageMult".into()]);
    rt.clear_grants();
    // GS_LEVEL = 0 in host stub (DamageHandler early-outs otherwise).
    // WorldThingSpawned path grants on monsters (synthetic dispatch defaults monster).
    rt.sync_host(5, 0, 0);
    rt.activate_map_handlers().expect("activate");
    rt.dispatch_handlers("WorldThingSpawned")
        .expect("spawn skill5");
    assert!(
        rt.take_grants().is_empty(),
        "skill 5 must not grant Damage_Control"
    );

    rt.sync_host(6, 0, 0);
    rt.dispatch_handlers("WorldThingSpawned")
        .expect("spawn skill6");
    let grants = rt.take_grants();
    assert!(
        grants
            .iter()
            .any(|(_, item, amt)| { item.eq_ignore_ascii_case("Damage_Control") && *amt == 1 }),
        "skill ≥ 6 WorldThingSpawned must GiveInventory Damage_Control; got {grants:?}"
    );
}

#[test]
fn zs3b_inventory_subclass_is_givable_and_ticks() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class StimToken : Inventory
{
    int ticks;
    int begun;
    Default
    {
        Inventory.MaxAmount 5;
        Inventory.Amount 1;
    }
    override void PostBeginPlay()
    {
        begun = 1;
    }
    override void Tick()
    {
        ticks = ticks + 1;
    }
}
"#;
    let result = room_zscript::check(&[("stim.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let cid = w.classes.find_name("StimToken").expect("registered");
    assert!(w.classes.get(cid).unwrap().is_inventory);
    assert_eq!(w.classes.get(cid).unwrap().inventory_max_amount, 5);
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    assert!(give_inventory(&mut w, player, cid, 1));
    assert!(give_inventory(&mut w, player, cid, 10));
    assert_eq!(count_inventory(&w, player, cid), 5);
    let id = w.summon_in_front(0, "StimToken").expect("summon");
    // Own fields come after the inherited ones, so look the slots up by name.
    let slot = |w: &World, name: &str| -> usize {
        let vm = w.zscript.vm.as_ref().unwrap();
        let layout = vm
            .program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case("StimToken"))
            .unwrap();
        usize::from(layout.fields[name].1)
    };
    let (begun_slot, ticks_slot) = (slot(&w, "begun"), slot(&w, "ticks"));
    let h = w.zscript.actor_fields[&id.index()];
    let begun = match w.zscript.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => {
            fields_i.get(begun_slot).copied().unwrap_or(0)
        }
        _ => 0,
    };
    assert_eq!(begun, 1, "PostBeginPlay runs on the summoned item");
    w.tick();
    let h = w.zscript.actor_fields[&id.index()];
    let ticks = match w.zscript.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => {
            fields_i.get(ticks_slot).copied().unwrap_or(0)
        }
        _ => 0,
    };
    assert_eq!(ticks, 1, "Tick runs with the item's own script object");
}

#[test]
fn zs4b_inventory_modify_damage_scales_the_hit() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class DmgScale : Inventory
{
    override void ModifyDamage(int damage, out int newDamage, bool passive)
    {
        if (passive) newDamage = damage * 3;
    }
}
"#;
    let result = room_zscript::check(&[("dmg.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.skill = 3;
    w.load_zscript_program(result.program.unwrap());
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let before = w.actors.get(player).unwrap().health;
    let cid = w.classes.find_name("DmgScale").expect("registered");
    assert!(give_inventory(&mut w, player, cid, 1));
    let dealt = damage_mobj(&mut w, player, None, None, 10);
    assert_eq!(dealt, 30, "passive ModifyDamage triples the hit");
    assert_eq!(w.actors.get(player).unwrap().health, before - 30);
}

#[test]
fn zs4b_burn_and_monster_source_pick_the_scale() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class DmgTyped : Inventory
{
    override void ModifyDamage(int damage, string damageType, out int newDamage, bool passive, Actor inflictor, Actor source, int flags)
    {
        if (!passive || !source || !inflictor) return;
        if (source.bISMONSTER)
        {
            if (damageType == "Burn") newDamage = damage * 3;
            else newDamage = damage * 7;
        }
    }
}
"#;
    let result = room_zscript::check(&[("dmg.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.skill = 3;
    w.load_zscript_program(result.program.unwrap());
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let cid = w.classes.find_name("DmgTyped").expect("registered");
    assert!(give_inventory(&mut w, player, cid, 1));
    let zid = w.classes.find_name("ZombieMan").unwrap();
    let zombie = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(160.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    w.actors.get_mut(zombie).unwrap().damage_type = "Burn".into();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        30,
        "Burn from a monster is ×3"
    );
    w.actors.get_mut(player).unwrap().health = 200;
    w.actors.get_mut(zombie).unwrap().damage_type.clear();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        70,
        "other monster damage is ×7"
    );
}

#[test]
fn zs4b_real_damage_control_scales_burn_normal_and_lost_soul() {
    let handler = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/zscript/bd_real/DamageHandler.zc"),
    )
    .expect("DamageHandler.zc");
    let result = room_zscript::check(&[("DamageHandler.zc".into(), handler)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States { Spawn: POSS A -1 Stop }
}
actor TehLostSoul : ZombieMan
{
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.skill = 3;
    w.load_zscript_program(result.program.unwrap());
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let cid = w.classes.find_name("Damage_Control").expect("registered");
    assert!(give_inventory(&mut w, player, cid, 1));
    let zombie = w
        .spawn_at(
            w.classes.find_name("ZombieMan").unwrap(),
            room_core::math::DVec3::new(160.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    w.actors.get_mut(zombie).unwrap().damage_type = "Burn".into();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        30
    );
    w.actors.get_mut(player).unwrap().health = 200;
    w.actors.get_mut(zombie).unwrap().damage_type.clear();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        70
    );
    w.actors.get_mut(player).unwrap().health = 200;
    let soul = w
        .spawn_at(
            w.classes.find_name("TehLostSoul").unwrap(),
            room_core::math::DVec3::new(180.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(
        damage_mobj(&mut w, player, Some(soul), Some(soul), 10),
        20,
        "TehLostSoul is ×2"
    );
}

#[test]
fn zs4b_skill6_spawn_grants_damage_control() {
    let handler = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/zscript/bd_real/DamageHandler.zc"),
    )
    .expect("DamageHandler.zc");
    let result = room_zscript::check(&[("DamageHandler.zc".into(), handler)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States { Spawn: POSS A -1 Stop }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    w.zscript.set_event_handlers(["Script_DamageMult".into()]);
    w.skill = 5;
    let low = w
        .spawn_at(
            w.classes.find_name("ZombieMan").unwrap(),
            room_core::math::DVec3::new(140.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    let item = w.classes.find_name("Damage_Control").unwrap();
    assert_eq!(count_inventory(&w, low, item), 0, "skill 5 does not grant");
    w.skill = 6;
    let high = w
        .spawn_at(
            w.classes.find_name("ZombieMan").unwrap(),
            room_core::math::DVec3::new(180.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(
        count_inventory(&w, high, item),
        1,
        "skill 6 WorldThingSpawned gives Damage_Control"
    );
}

#[test]
fn zs4b_skill6_player_entered_grants_damage_control() {
    let handler = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/zscript/bd_real/DamageHandler.zc"),
    )
    .expect("DamageHandler.zc");
    let result = room_zscript::check(&[("DamageHandler.zc".into(), handler)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States { Spawn: POSS A -1 Stop }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    w.zscript.set_event_handlers(["Script_DamageMult".into()]);
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let item = w.classes.find_name("Damage_Control").unwrap();
    w.skill = 5;
    w.fire_player_entered(0);
    assert_eq!(
        count_inventory(&w, player, item),
        0,
        "skill 5 does not grant"
    );
    w.skill = 6;
    w.fire_player_entered(0);
    assert_eq!(
        count_inventory(&w, player, item),
        1,
        "skill 6 PlayerEntered gives Damage_Control"
    );
}

fn damage_handler_world(with_map_zombie: bool) -> World {
    let handler = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/zscript/bd_real/DamageHandler.zc"),
    )
    .expect("DamageHandler.zc");
    let result = room_zscript::check(&[("DamageHandler.zc".into(), handler)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Inventory : Actor
{
  States { Spawn: POSS A -1 Stop }
}
actor TehLostSoul : ZombieMan
{
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut builder = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0);
    if with_map_zombie {
        builder = builder.thing(3004, 200.0, 64.0, 0);
    }
    let mut map = builder.build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    w.zscript.set_event_handlers(["Script_DamageMult".into()]);
    w
}

#[test]
fn bd_damage_control_skill6() {
    let mut w = damage_handler_world(true);
    w.skill = 6;
    w.arm_zscript_level();
    let summoned = w.summon_in_front(0, "ZombieMan").expect("summon");
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let item = w.classes.find_name("Damage_Control").unwrap();
    let zombie = w.classes.find_name("ZombieMan").unwrap();
    assert_eq!(count_inventory(&w, player, item), 1);
    let mut owned = 0;
    for id in w.thinkers.default.iter().copied() {
        let Some(a) = w.actors.get(id) else { continue };
        if a.class != zombie {
            continue;
        }
        assert_eq!(
            count_inventory(&w, id, item),
            1,
            "map and summoned monsters"
        );
        owned += 1;
    }
    assert!(owned >= 2, "map thing plus summon, got {owned}");
    assert_eq!(count_inventory(&w, summoned, item), 1);
}

#[test]
fn bd_damage_scaling() {
    let mut w = damage_handler_world(false);
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    w.skill = 5;
    let weak = w
        .spawn_at(
            w.classes.find_name("ZombieMan").unwrap(),
            room_core::math::DVec3::new(140.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(damage_mobj(&mut w, player, Some(weak), Some(weak), 10), 10);
    w.actors.get_mut(player).unwrap().health = 200;
    w.skill = 6;
    w.fire_player_entered(0);
    let zombie = w
        .spawn_at(
            w.classes.find_name("ZombieMan").unwrap(),
            room_core::math::DVec3::new(160.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        70
    );
    w.actors.get_mut(player).unwrap().health = 200;
    w.actors.get_mut(zombie).unwrap().damage_type = "Burn".into();
    assert_eq!(
        damage_mobj(&mut w, player, Some(zombie), Some(zombie), 10),
        30
    );
    w.actors.get_mut(player).unwrap().health = 200;
    let soul = w
        .spawn_at(
            w.classes.find_name("TehLostSoul").unwrap(),
            room_core::math::DVec3::new(180.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    assert_eq!(damage_mobj(&mut w, player, Some(soul), Some(soul), 10), 20);
}

#[test]
fn bd_player_tid_roundtrip() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/zscript/bd_real/BD_WCS.zc"),
    )
    .expect("BD_WCS.zc");
    let result = room_zscript::check(&[("BD_WCS.zc".into(), src)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    w.zscript.set_player_tids(BTreeMap::new());
    w.zscript
        .vm
        .as_mut()
        .unwrap()
        .call_with("ZandroCompat.SetPlayerTID", &[0, 1000], &[])
        .expect("SetPlayerTID");
    w.apply_script_player_tids();
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    assert_eq!(w.actors.get(player).unwrap().tid, 1000);
    let got = w
        .zscript
        .vm
        .as_mut()
        .unwrap()
        .call_with("ZandroCompat.GetPlayerTID", &[0], &[])
        .expect("GetPlayerTID");
    assert_eq!(got, 1000);
}

#[test]
fn bd_two_runs_same_checksum() {
    fn run() -> u64 {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(3004, 180.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        for _ in 0..2_000 {
            w.tick();
        }
        w.checksum()
    }
    assert_eq!(run(), run());
}

#[test]
fn bd_gender() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/zscript/bd_real/BD_WCS.zc"),
    )
    .expect("BD_WCS.zc");
    let result = room_zscript::check(&[("BD_WCS.zc".into(), src)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor GenderFemale : Actor
{
  States { Spawn: POSS A -1 Stop }
}
actor GenderMale : Actor
{
  States { Spawn: POSS A -1 Stop }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    w.zscript
        .set_cvars(BTreeMap::from([("bd_Gender".into(), 1)]));
    w.zscript.clear_grants();
    w.zscript
        .vm
        .as_mut()
        .unwrap()
        .call_with("ZandroCompat.GetGender", &[0], &[])
        .expect("GetGender");
    w.apply_script_player_effects(0);
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    assert_eq!(w.actors.get(player).unwrap().sound_class, "fplayer");
    let female = w.classes.find_name("GenderFemale").unwrap();
    let male = w.classes.find_name("GenderMale").unwrap();
    assert_eq!(count_inventory(&w, player, female), 1);
    assert_eq!(count_inventory(&w, player, male), 0);
}

#[test]
fn bd_compiles_and_links() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/zscript/bd_real");
    let mut sources = Vec::new();
    for name in ["zscript.zc", "BD_WCS.zc", "DamageHandler.zc"] {
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let path = if name == "zscript.zc" {
            name.to_owned()
        } else {
            format!("zscript/{name}")
        };
        sources.push((path, text));
    }
    let result = room_zscript::check(&sources);
    let errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == room_zscript::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let prog = result.program.expect("compiled program");
    let natives = room_zvm::NativeTable::with_builtins();
    let unbound: Vec<_> = prog
        .native_imports
        .iter()
        .filter(|name| natives.id_ci(name).is_none())
        .cloned()
        .collect();
    assert!(unbound.is_empty(), "unbound natives: {unbound:?}");
    let poison = prog
        .functions
        .iter()
        .filter(|f| {
            f.code
                .iter()
                .any(|word| room_zvm::Op::from_u8((word >> 24) as u8) == Some(room_zvm::Op::Trap))
        })
        .count();
    assert_eq!(poison, 0, "functions containing Trap/poison");
}

fn load_uzdoom_stdlib() -> room_zvm::CompiledProgram {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).expect("zscript.txt");
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = std::fs::read_to_string(root.join(path)).expect(path);
        sources.push((path.replace('\\', "/"), text));
    }
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    result.program.expect("uzdoom program")
}

#[test]
fn scripted_marine_pistol_damages_target() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(load_uzdoom_stdlib());
    let zid = w.classes.find_name("ZombieMan").expect("ZombieMan");
    let mid = w.classes.find_name("MarinePistol").expect("MarinePistol");
    let zombie = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(180.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("zombie");
    let marine = w
        .spawn_at(
            mid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("marine");
    w.actors.get_mut(marine).unwrap().target = Some(zombie);
    w.actors.get_mut(marine).unwrap().angle = room_core::math::Angle::ZERO;
    let before = w.actors.get(zombie).unwrap().health;
    let called = room_sim::call_zscript_action(&mut w, marine, "A_M_FirePistol");
    let after = w.actors.get(zombie).unwrap().health;
    assert!(
        after < before,
        "ScriptedMarine pistol should hit; health {before} -> {after} ({called:?})"
    );
}

#[test]
fn scripted_marine_looks_then_chases() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(load_uzdoom_stdlib());
    let mid = w
        .classes
        .find_name("ScriptedMarine")
        .expect("ScriptedMarine");
    let player = w.players[0].as_ref().and_then(|p| p.mo).expect("player");
    let marine = w
        .spawn_at(
            mid,
            room_core::math::DVec3::new(200.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("marine");
    // Spawn runs `A_MarineLook`, so the player may already be the target.
    room_sim::call_zscript_action(&mut w, marine, "A_MarineLook");
    assert_eq!(
        w.actors.get(marine).unwrap().target,
        Some(player),
        "A_MarineLook should acquire the player"
    );
    let x0 = w.actors.get(marine).unwrap().pos.x();
    room_sim::call_zscript_action(&mut w, marine, "A_MarineChase");
    let x1 = w.actors.get(marine).unwrap().pos.x();
    assert!(
        (x1 - x0).abs() > 0.5,
        "A_MarineChase should step toward the player; x {x0} -> {x1}, speed {}",
        w.actors.get(marine).unwrap().speed
    );
}

#[test]
fn scripted_marine_state_machine_shoots_player() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(load_uzdoom_stdlib());
    let mid = w.classes.find_name("MarinePistol").expect("MarinePistol");
    assert!(
        w.states.find_label(mid, "Spawn").is_some(),
        "ScriptedMarine Spawn should merge from the stdlib"
    );
    assert!(
        w.states.find_label(mid, "Missile").is_some(),
        "MarinePistol Missile should resolve"
    );
    let player = w.players[0].as_ref().and_then(|p| p.mo).expect("player");
    let before = w.actors.get(player).unwrap().health;
    let marine = w
        .spawn_at(
            mid,
            room_core::math::DVec3::new(160.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("marine");
    for _ in 0..120 {
        w.tick();
        if w.actors.get(player).unwrap().health < before {
            return;
        }
    }
    let marine = w.actors.get(marine).unwrap();
    panic!(
        "ticking MarinePistol should fire; player health stayed {before}, marine target {:?} state {:?} pos {:?}",
        marine.target, marine.state, marine.pos
    );
}

#[test]
fn zscript_a_no_blocking_clears_solid() {
    let zs = r#"
class FallProbe : Actor
{
    void DoFall() { A_NoBlocking(); }
}
"#;
    let result = room_zscript::check(&[("fall.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(result.program.expect("program"));
    let cid = w.classes.find_name("FallProbe").expect("FallProbe");
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    w.actors.get_mut(id).unwrap().flags |= room_sim::FLAG_SOLID;
    room_sim::call_zscript_action(&mut w, id, "DoFall");
    assert_eq!(
        w.actors.get(id).unwrap().flags & room_sim::FLAG_SOLID,
        0,
        "A_NoBlocking from ZScript should clear SOLID"
    );
}

#[test]
fn zscript_set_health_then_damage_self() {
    let zs = r#"
class HealthProbe : Actor
{
    void Do() { A_SetHealth(40); A_DamageSelf(15); }
}
"#;
    let result = room_zscript::check(&[("health.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(result.program.expect("program"));
    let cid = w.classes.find_name("HealthProbe").expect("HealthProbe");
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    w.actors.get_mut(id).unwrap().flags |= room_sim::FLAG_SHOOTABLE;
    room_sim::call_zscript_action(&mut w, id, "Do");
    assert_eq!(
        w.actors.get(id).unwrap().health,
        25,
        "A_SetHealth(40) then A_DamageSelf(15)"
    );
}

#[test]
fn zscript_set_size_changes_radius_and_height() {
    let zs = r#"
class SizeProbe : Actor
{
    void Do() { A_SetSize(12, 48); }
}
"#;
    let result = room_zscript::check(&[("size.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(result.program.expect("program"));
    let cid = w.classes.find_name("SizeProbe").expect("SizeProbe");
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    room_sim::call_zscript_action(&mut w, id, "Do");
    let actor = w.actors.get(id).unwrap();
    assert_eq!(actor.radius, 12.0, "A_SetSize radius");
    assert_eq!(actor.height, 48.0, "A_SetSize height");
}

#[test]
fn zscript_start_sound_then_stop_sounds() {
    let zs = r#"
class SoundProbe : Actor
{
    void Play() { A_StartSound("weapons/pistol"); }
    void Stop() { A_StopSounds(0, 0); }
}
"#;
    let result = room_zscript::check(&[("sound.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(result.program.expect("program"));
    let cid = w.classes.find_name("SoundProbe").expect("SoundProbe");
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    room_sim::call_zscript_action(&mut w, id, "Play");
    assert!(
        w.sound_events.iter().any(|ev| ev.name == "weapons/pistol"),
        "A_StartSound should queue weapons/pistol, events {:?}",
        w.sound_events
            .iter()
            .map(|ev| ev.name.as_str())
            .collect::<Vec<_>>()
    );
    room_sim::call_zscript_action(&mut w, id, "Stop");
    // The queued start is dropped; a stop event tells the mixer to silence
    // anything already playing on those channels.
    assert!(
        w.sound_events.iter().all(|ev| ev.name != "weapons/pistol"),
        "A_StopSounds should clear the queued sound"
    );
    assert_eq!(
        w.sound_events.last().map(|ev| ev.adjust),
        Some(room_sim::SimSoundAdjust::StopChannels)
    );
}

#[test]
fn zscript_monster_rail_damages_target() {
    let zs = r#"
class RailProbe : Actor
{
    void Do() { A_MonsterRail(); }
}
"#;
    let result = room_zscript::check(&[("rail.zs".into(), zs.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    w.load_zscript_program_stubbed(result.program.expect("program"));
    let zid = w.classes.find_name("ZombieMan").expect("ZombieMan");
    let zombie = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(80.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("zombie");
    let before = w.actors.get(zombie).unwrap().health;
    let cid = w.classes.find_name("RailProbe").expect("RailProbe");
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(96.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    // UZDoom's `A_MonsterRail` fires at the target and does nothing without one.
    w.actors.get_mut(id).unwrap().target = Some(zombie);
    room_sim::call_zscript_action(&mut w, id, "Do");
    let after = w.actors.get(zombie).unwrap().health;
    assert!(
        after < before,
        "A_MonsterRail should hit; health {before} -> {after}"
    );
}

/// UZDoom stdlib plus one extra mod source.
fn load_uzdoom_stdlib_with(extra: &str) -> room_zvm::CompiledProgram {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).expect("zscript.txt");
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let Some(rest) = line.trim().strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = std::fs::read_to_string(root.join(path)).expect(path);
        sources.push((path.replace('\\', "/"), text));
    }
    sources.push(("mod.zs".to_owned(), extra.to_owned()));
    let result = room_zscript::check(&sources);
    assert!(
        result.ok(),
        "{:?}",
        result
            .diagnostics
            .iter()
            .filter(|d| d.severity == room_zscript::Severity::Error)
            .collect::<Vec<_>>()
    );
    result.program.expect("uzdoom program")
}

#[test]
fn zscript_tick_sees_and_changes_live_actor_fields() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    let prog = load_uzdoom_stdlib_with(
        r#"
class TestMover : Actor
{
    double seenx;
    double seenfloor;
    int seenhp;
    override void Tick()
    {
        seenx = pos.x;
        seenfloor = floorz;
        angle = 33.5;
        vel.z = 2.25;
        if (target) { seenhp = target.health; target.health -= 7; }
    }
}
"#,
    );
    w.load_zscript_program_stubbed(prog);
    let cid = w
        .classes
        .find_name("TestMover")
        .expect("TestMover registered");
    let mover = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(200.5, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("mover");
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    w.actors.get_mut(mover).unwrap().target = Some(player);
    let hp = w.actors.get(player).unwrap().health;
    w.tick();
    let a = w.actors.get(mover).unwrap();
    assert!(
        (a.angle.degrees() - 33.5).abs() < 1e-6,
        "angle {}",
        a.angle.degrees()
    );
    assert!(a.vel.z() > 0.0, "vel.z written back: {}", a.vel.z());
    assert_eq!(w.actors.get(player).unwrap().health, hp - 7);
    // Script fields saw live values.
    let h = w.zscript.actor_fields[&mover.index()];
    let vm = w.zscript.vm.as_ref().unwrap();
    let layout = vm
        .program
        .classes
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("TestMover"))
        .unwrap();
    let (fx, ff, fh) = (
        usize::from(layout.fields["seenx"].1),
        usize::from(layout.fields["seenfloor"].1),
        usize::from(layout.fields["seenhp"].1),
    );
    match vm.heap.get(h) {
        Some(room_zvm::ObjKind::Object {
            fields_f, fields_i, ..
        }) => {
            assert!(
                (fields_f[fx] - 200.5).abs() < 1e-9,
                "seenx {}",
                fields_f[fx]
            );
            assert_eq!(fields_f[ff], 0.0);
            assert_eq!(fields_i[fh], i64::from(hp));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn zscript_world_query_natives_read_live_data() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    let prog = load_uzdoom_stdlib_with(
        r#"
class Probe : Actor
{
    int saw;
    double dist;
    double ang;
    int clips;
    int found;
    override void Tick()
    {
        if (!target) return;
        saw = CheckSight(target);
        dist = Distance2D(target);
        ang = AngleTo(target);
        clips = target.CountInv("Clip");
        let inv = target.FindInventory("Clip");
        found = inv ? inv.Amount : -1;
        vector3 there = Vec3Offset(10, 0, 4);
        SetOrigin(there, false);
    }
}
"#,
    );
    w.load_zscript_program_stubbed(prog);
    let cid = w.classes.find_name("Probe").expect("Probe");
    let probe = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(164.0, 64.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .expect("probe");
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let clip = w.classes.find_name("Clip").expect("Clip");
    let have = room_sim::count_inventory(&w, player, clip);
    w.actors.get_mut(probe).unwrap().target = Some(player);
    w.tick();
    let vm = w.zscript.vm.as_ref().unwrap();
    let layout = vm
        .program
        .classes
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("Probe"))
        .unwrap();
    let slot = |n: &str| usize::from(layout.fields[n].1);
    let h = w.zscript.actor_fields[&probe.index()];
    let (fi, ff) = match vm.heap.get(h) {
        Some(room_zvm::ObjKind::Object {
            fields_i, fields_f, ..
        }) => (fields_i.clone(), fields_f.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(fi[slot("saw")], 1, "player visible");
    assert!(
        (ff[slot("dist")] - 100.0).abs() < 1e-6,
        "dist {}",
        ff[slot("dist")]
    );
    assert!(
        (ff[slot("ang")] - 180.0).abs() < 1e-6,
        "ang {}",
        ff[slot("ang")]
    );
    assert_eq!(fi[slot("clips")], i64::from(have));
    assert_eq!(
        fi[slot("found")],
        if have > 0 { i64::from(have) } else { -1 }
    );
    let a = w.actors.get(probe).unwrap();
    assert!(
        (a.pos.x() - 174.0).abs() < 1e-6,
        "SetOrigin x {}",
        a.pos.x()
    );
    assert!(
        (a.pos.z() - 4.0).abs() < 1e-6 || a.pos.z() <= 4.0,
        "SetOrigin z {}",
        a.pos.z()
    );
}

#[test]
fn zscript_cursector_set_environment_id_after_map_start() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor EchoSetter 9902
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    // UZDoom's `SoundEnvironment.Activate` pattern, against the vendored
    // UZDoom stdlib (`Actor.CurSector` is a native `Sector`).
    let zs = r#"
class EchoSetter : Actor
{
    override void PostBeginPlay()
    {
        CurSector.SetEnvironmentID((8 << 8) | 0);
    }
}
"#;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.replace('\\', "/"),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("echo.zs".into(), zs.into()));
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let sector = 0; // the only room
    assert_eq!(w.sound_env.sector_env(sector), 0);
    let zid = classes.find_name("EchoSetter").unwrap();
    w.spawn_at(
        zid,
        room_core::math::DVec3::new(100.0, 100.0, 0.0),
        room_core::math::Angle::ZERO,
    )
    .unwrap();
    w.tick();
    assert_eq!(
        w.sound_env.sector_env(sector),
        0x0800,
        "Cave set by ZScript"
    );
}

#[test]
fn a_world_snapshot_carries_the_zscript_vm() {
    // Rollback copies the world, VM and all: a copy run through the same
    // tics must land exactly where the original does, including ZScript
    // fields and the RNG the script uses.
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor Wanderer 9904
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class Wanderer : Actor
{
    int steps;
    override void Tick()
    {
        Super.Tick();
        steps++;
        SetOrigin(pos + (random(-3, 3), random(-3, 3), 0), false);
        angle += steps;
    }
}
"#;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.replace('\\', "/"),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("wander.zs".into(), zs.into()));
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let zid = classes.find_name("Wanderer").unwrap();
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(256.0, 256.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    for _ in 0..10 {
        w.tick();
    }
    let mut copy = w.snapshot().expect("a ZScript world can be copied");
    for _ in 0..40 {
        w.tick();
        copy.tick();
    }
    let (a, b) = (w.actors.get(id).unwrap(), copy.actors.get(id).unwrap());
    assert_ne!(a.pos.x(), 256.0, "the script moved it");
    assert_eq!((a.pos, a.angle), (b.pos, b.angle));
    assert_eq!(w.checksum(), copy.checksum());
    // Same RNG state too: more random steps still agree.
    for _ in 0..5 {
        w.tick();
        copy.tick();
    }
    assert_eq!(
        w.actors.get(id).unwrap().pos,
        copy.actors.get(id).unwrap().pos
    );
}

#[test]
fn zscript_point_in_sector_set_environment_reaches_another_sector() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor FarEcho 9903
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    // A sector other than the caller's, the way a level-wide script names
    // one: `level.PointInSector(pos)`.
    let zs = r#"
class FarEcho : Actor
{
    override void PostBeginPlay()
    {
        Sector s = level.PointInSector((600, 100));
        s.SetEnvironmentID((8 << 8) | 0);
    }
}
"#;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.replace('\\', "/"),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("far.zs".into(), zs.into()));
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let here = room_sim::sector_at_xy(&w, 100.0, 100.0).unwrap();
    let there = room_sim::sector_at_xy(&w, 600.0, 100.0).unwrap();
    assert_ne!(here, there);
    let zid = classes.find_name("FarEcho").unwrap();
    w.spawn_at(
        zid,
        room_core::math::DVec3::new(100.0, 100.0, 0.0),
        room_core::math::Angle::ZERO,
    )
    .unwrap();
    w.tick();
    assert_eq!(
        w.sound_env.sector_env(there),
        0x0800,
        "Cave on the named sector"
    );
    assert_eq!(w.sound_env.sector_env(here), 0, "caller's sector untouched");
}

#[test]
fn uz6_level_sky_fog_natives_reach_the_world() {
    let src = r#"
class FogProbe : EventHandler
{
    override void WorldTick()
    {
        Level.SetSkyFog(96);
        Level.SetThickFog(512.0, 4.0);
        TextureID mist = TexMan.CheckForTexture("MIST01", TexMan.Type_Any);
        Level.ChangeSkyMist(mist, false, 0.5);
    }
}
"#;
    let result = room_zscript::check(&[("fog_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map(map).unwrap();
    let prog = result.program.unwrap();
    w.load_zscript_program(prog);
    w.zscript.set_event_handlers(["FogProbe".into()]);
    assert_eq!(w.thick_fog(), None, "MAPINFO default -1 = off");
    w.tick();
    assert_eq!(w.sky_fog, 96.0);
    assert_eq!(w.thick_fog(), Some((512.0, 4.0)));
    assert_eq!(w.sky_mist.as_deref(), Some("MIST01"));
    assert!(!w.use_sky_mist);
    assert!((w.sky_mist_yscale - 0.5).abs() < 1e-9);
}

#[test]
fn uz3_super_calls_run_the_parent_body() {
    let src = r#"
class BaseProbe : EventHandler
{
    int score;
    virtual int Score() { return 10; }
}
class UzProbe : BaseProbe
{
    override int Score() { return Super.Score() + 1; }
    override void WorldLoaded(WorldEvent e)
    {
        score = Score();
    }
}
"#;
    let result = room_zscript::check(&[("uz_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    rt.set_event_handlers(["UzProbe".into()]);
    rt.activate_map_handlers().expect("activate");
    let h = rt.handler_objs["UzProbe"];
    let score = match rt.vm.as_ref().unwrap().heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i.first().copied().unwrap_or(0),
        _ => 0,
    };
    assert_eq!(score, 11, "Super.Score() runs the parent body, then + 1");
}

/// Compile `zs` with UZDoom's base scripts, load it on `map` with a
/// `PortalProbe` actor, spawn one at `at` and run a tic.
fn run_portal_probe(zs: &str, map: room_map::MapGeometry, at: (f64, f64)) -> room_sim::Actor {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        "actor PortalProbe 9904\n{\n  States\n  {\n  Spawn:\n    POSS A -1\n    Stop\n  }\n}\n"
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.replace('\\', "/"),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("probe.zs".into(), zs.into()));
    let result = room_zscript::check(&sources);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut map = map;
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let zid = classes.find_name("PortalProbe").unwrap();
    let id = w
        .spawn_at(
            zid,
            room_core::math::DVec3::new(at.0, at.1, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    w.tick();
    w.actors.get(id).unwrap().clone()
}

#[test]
fn zscript_vectors_go_through_linked_line_portals() {
    let zs = r#"
class PortalProbe : Actor
{
    override void PostBeginPlay()
    {
        // Room B's x 1040 is 16 past A's portal line at x 256.
        Vector2 d = level.Vec2Diff((128, 128), (1040, 128));
        special1 = int(d.x);
        // 200 east of x 128 crosses the portal at x 256.
        Vector3 p = Vec3Offset(200, 0, 0);
        special2 = int(p.x);
        Vector3 q = Vec3Offset(200, 0, 0, true);
        accuracy = int(q.x);
        Vector2 g = level.GetDisplacement(1, 2);
        stamina = int(g.x);
    }
}
"#;
    let map = room_map::load_udmf_text(
        "MAP01",
        &room_testkit::linked_line_portal_rooms(3).textmap(),
    )
    .unwrap();
    let a = run_portal_probe(zs, map, (128.0, 128.0));
    assert_eq!(a.ext.special1, 144);
    assert_eq!(a.ext.special2, 1096);
    assert_eq!(a.accuracy, 328, "absolute offsets ignore portals");
    assert_eq!(a.stamina.abs(), 768);
}

#[test]
fn zscript_sector_portal_queries() {
    let zs = r#"
class PortalProbe : Actor
{
    override void PostBeginPlay()
    {
        Sector s = level.PointInSector((128, 128));
        Vector2 d = s.GetPortalDisplacement(Sector.ceiling);
        special1 = int(d.x);
        special2 = s.PortalIsLinked(Sector.ceiling) ? 1 : 0;
        accuracy = s.PortalBlocksMovement(Sector.ceiling) ? 1 : 0;
        stamina = int(s.GetPortalPlaneZ(Sector.ceiling));
        threshold = s.PortalIsLinked(Sector.floor) ? 1 : 0;
        // CurSector is the caller's own sector.
        reactiontime = CurSector.GetPortalType(Sector.ceiling);
    }
}
"#;
    let map = room_map::load_udmf_text(
        "MAP01",
        &room_testkit::linked_plane_portal_rooms().textmap(),
    )
    .unwrap();
    let a = run_portal_probe(zs, map, (128.0, 128.0));
    assert_eq!(a.ext.special1, 1024);
    assert_eq!(a.ext.special2, 1);
    assert_eq!(a.accuracy, 0);
    assert_eq!(a.stamina, 128);
    assert_eq!(a.threshold, 0, "A's floor is no portal");
    assert_eq!(a.reaction_time, 3, "PORTS_LINKEDPORTAL");
}

#[test]
fn zscript_map_objects_and_line_portals() {
    let zs = r#"
class PortalProbe : Actor
{
    override void PostBeginPlay()
    {
        // Lines that are portals, and where the first one leads.
        int portals = 0;
        int dest = -1;
        for (int i = 0; i < level.Lines.Size(); i++)
        {
            Line l = level.Lines[i];
            if (l.isLinePortal())
            {
                portals++;
                if (dest < 0) dest = l.getPortalDestination().Index();
            }
        }
        special1 = portals;
        special2 = dest;
        // Sectors, and the caller's own.
        accuracy = level.Sectors.Size() * 100 + CurSector.Index();
        // Its floor through the plane object.
        stamina = int(CurSector.floorplane.ZAtPoint(pos.xy));
        // A sector's lines know their sectors.
        Line first = CurSector.Lines[0];
        threshold = (first.frontsector == CurSector || first.backsector == CurSector) ? 1 : 0;
        CurSector.SetLightLevel(77);
    }
}
"#;
    let map = room_map::load_udmf_text(
        "MAP01",
        &room_testkit::linked_line_portal_rooms(3).textmap(),
    )
    .unwrap();
    let w_light;
    let n_sectors;
    let own;
    let a = {
        let deco = format!(
            "{}\n{}",
            include_str!("../../../testdata/actors/doom_basic.txt"),
            "actor PortalProbe 9904\n{\n  States\n  {\n  Spawn:\n    POSS A -1\n    Stop\n  }\n}\n"
        );
        let ir = parse_decorate(&deco).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let prog = load_uzdoom_stdlib_with(zs);
        let mut map = map;
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        w.load_zscript_program(prog);
        let zid = classes.find_name("PortalProbe").unwrap();
        let id = w
            .spawn_at(
                zid,
                room_core::math::DVec3::new(128.0, 128.0, 0.0),
                room_core::math::Angle::ZERO,
            )
            .unwrap();
        w.tick();
        let sector = room_sim::sector_at_xy(&w, 128.0, 128.0).unwrap();
        w_light = w.sectors[sector].light;
        n_sectors = w.map.sectors.len() as i32;
        own = sector as i32;
        w.actors.get(id).unwrap().clone()
    };
    assert_eq!(a.ext.special1, 2, "the two portal lines");
    assert!(a.ext.special2 >= 0, "a destination line");
    assert_eq!(
        a.accuracy,
        n_sectors * 100 + own,
        "level.Sectors and CurSector"
    );
    assert_eq!(a.stamina, 0, "floor height");
    assert_eq!(a.threshold, 1);
    assert_eq!(w_light, 77, "SetLightLevel reaches the playsim");
}

#[test]
fn zscript_object_arrays() {
    let src = r#"
class Box { int v; }
class Holder { Array<Box> items; }
class T {
    static int Sum(Array<Box> list) {
        int t = 0;
        for (int i = 0; i < list.Size(); i++) t += list[i].v;
        return t;
    }
    static int Run() {
        let h = new("Holder");
        for (int i = 1; i <= 3; i++) { Box b = new("Box"); b.v = i; h.items.Push(b); }
        Box mid = h.items[1];
        int found = h.items.Find(mid);
        Box extra = new("Box"); extra.v = 10;
        h.items.Insert(0, extra);
        h.items.Delete(2);
        Box last = new("Box"); last.v = 100;
        h.items[0] = last;
        int setcount = 0;
        if (h.items[0]) setcount++;
        Array<Box> loc;
        loc.Push(null);
        if (!loc[0]) setcount += 10;
        return Sum(h.items) * 1000 + found * 100 + setcount;
    }
}
"#;
    let result = room_zscript::check(&[("arrays.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    // [1,2,3] → Find(2) = 1; Insert 10 at 0, Delete(2), [0] = 100 → [100,1,3].
    assert_eq!(rt.script_call_with("T.Run", &[], &[]).unwrap(), 104_111);
}

/// Project Brutality idioms: brace-initialized local arrays, class-wide
/// `static const` tables (own, inherited, mixin, qualified) and multi-assign
/// into non-local targets.
#[test]
fn pb_array_initializers_and_static_tables_run() {
    let src = r#"
mixin class PBNames
{
    static const String First[] =
    {
        "John",
        "Jeff",
        "Chris",
    };
    int NameLen(int i) { return First[i].Length(); }
}
class PBBase
{
    mixin PBNames;
    static const int Weights[] = { 5, 7, 11 };
}
class PBProbe : PBBase
{
    static int Sum()
    {
        int s[4] = { 1, 2, 3, 4 };
        int total = 0;
        for (int i = 0; i < s.Size(); i++) total += s[i];
        return total;
    }
    static int Tables()
    {
        int t = 0;
        for (int i = 0; i < Weights.Size(); i++) t += Weights[i];
        return t * 100 + PBBase.Weights.Size();
    }
    static int Inner()
    {
        static const int Local[] = { 10, 20, 30 };
        return Local[2] + Local.Size();
    }
    static int, int Pair(int a) { return a + 1, a * 2; }
    static int Multi()
    {
        Array<int> arr;
        arr.Push(0);
        arr.Push(0);
        [arr[0], arr[1]] = Pair(4);
        return arr[0] * 10 + arr[1];
    }
    static int SelfParam(PBProbe self) { return self ? 8 : 7; }
    static int CallSelf() { return SelfParam(null) * 10 + SelfParam(new("PBProbe")); }
    static int, PBProbe Two() { return 3, new("PBProbe"); }
    static int ObjMulti()
    {
        int n;
        PBProbe p;
        [n, p] = Two();
        return p ? n : -1;
    }
    static int Picks()
    {
        int r = randompick(4, 5, 6);
        double f = frandompick(1.5, 2.5) + cfrandom(-1, 1);
        return (r >= 4 && r <= 6 && f >= 0.5 && f <= 3.5) ? 1 : 0;
    }
    static int Names()
    {
        let p = new("PBProbe");
        return p.NameLen(0) * 10 + p.NameLen(2);
    }
}
"#;
    let result = room_zscript::check(&[("pb_probe.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    assert_eq!(rt.script_call("PBProbe.Sum").expect("Sum"), 10);
    assert_eq!(rt.script_call("PBProbe.Tables").expect("Tables"), 2303);
    assert_eq!(rt.script_call("PBProbe.Inner").expect("Inner"), 33);
    assert_eq!(rt.script_call("PBProbe.Multi").expect("Multi"), 58);
    assert_eq!(rt.script_call("PBProbe.Names").expect("Names"), 45);
    assert_eq!(rt.script_call("PBProbe.Picks").expect("Picks"), 1);
    assert_eq!(rt.script_call("PBProbe.CallSelf").expect("CallSelf"), 78);
    assert_eq!(rt.script_call("PBProbe.ObjMulti").expect("ObjMulti"), 3);
}

/// Handler objects get their `Array<…>` fields, as `new` makes them
/// (Project Brutality's `PB_Materialsys.OnRegister` pushed into a null array).
#[test]
fn handler_array_fields_are_allocated() {
    let src = r#"
class ArrHandler : StaticEventHandler
{
    Map<int, Object> keys;
    Array<int> ranges;
    Array<String> names;
    int got;
    override void OnRegister()
    {
        ranges.Push(names.Size());
        names.Push("a");
        got = ranges.Size() * 10 + names.Size();
    }
    static int Probe()
    {
        let h = ArrHandler(StaticEventHandler.Find("ArrHandler"));
        return h ? h.got : -1;
    }
}
"#;
    let result = room_zscript::check(&[("arr_handler.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    rt.set_event_handlers(["ArrHandler".to_owned()]);
    rt.activate_map_handlers().expect("OnRegister");
}

/// `[RealFloorZ, RealFloor] = F();` with fields of `self` as targets.
#[test]
fn multi_assign_into_self_fields() {
    let src = r#"
class FieldMulti
{
    int n;
    double d;
    FieldMulti other;
    static int, FieldMulti Two() { return 7, new("FieldMulti"); }
    static double, int Three() { return 2.5, 4; }
    int Fill()
    {
        [n, other] = Two();
        [d, n] = Three();
        return (other ? 100 : 0) + n * 10 + int(d * 2);
    }
    static int Run() { let f = new("FieldMulti"); return f.Fill(); }
}
"#;
    let result = room_zscript::check(&[("field_multi.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    assert_eq!(rt.script_call("FieldMulti.Run").expect("Run"), 145);
}

/// `holder.flashlightChargeMax`: a class constant read through an object, in a
/// float expression (Project Brutality's HUD).
#[test]
fn class_constant_through_an_object_in_float_math() {
    let src = r#"
class ConstHolder
{
    const ChargeMax = 100.0;
    double charge;
}
class ConstReader
{
    ConstHolder holder;
    static int Run()
    {
        let r = new("ConstReader");
        r.holder = new("ConstHolder");
        r.holder.charge = 25.0;
        return int(r.Ratio() * 100);
    }
    double Ratio() { return holder.charge / holder.ChargeMax; }
}
"#;
    let result = room_zscript::check(&[("const_holder.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    assert_eq!(rt.script_call("ConstReader.Run").expect("Run"), 25);
}

/// Script functions returning vectors (`vector2 SafeUnit2(…)`, PB's jump
/// code), alone, in `dot` and under unary minus, and before another result.
#[test]
fn script_functions_return_vectors() {
    let src = r#"
class VecRet
{
    vector2 SafeUnit2(vector2 v)
    {
        double l = v.Length();
        if (l == 0) return (0, 0);
        return v / l;
    }
    static vector3 Up(double z) { return (1, 2, z); }
    static vector2, int Pair() { return (3, 4), 7; }
    int Run()
    {
        vector2 a = SafeUnit2((3, 4));
        vector3 b = Up(5);
        double d = (1, 0) dot SafeUnit2((10, 0));
        double neg = - (0, 1) dot SafeUnit2((0, 2)) > 0 ? 1.0 : 2.0;
        vector2 p; int q;
        [p, q] = Pair();
        return int(p.y + q) * 1000000 + int(a.x * 100) * 10000 + int(b.z) * 100 + int(d) * 10 + int(neg);
    }
    static int Go() { let o = new("VecRet"); return o.Run(); }
}
"#;
    let result = room_zscript::check(&[("vec_ret.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    // p.y + q = 11, a.x = 0.6, b.z = 5, d = 1, neg branch = 2.
    assert_eq!(rt.script_call("VecRet.Go").expect("Go"), 11_600_512);
}

/// `a cross b` on vector3 (NashGore's `RotateVector`).
#[test]
fn vector_cross_product() {
    let src = r#"
class CrossT
{
    static int Go()
    {
        vector3 a = (1, 0, 0);
        vector3 b = (0, 1, 0);
        vector3 c = a cross b;
        vector3 u = (a cross b).Unit();
        vector3 m = (a * 2 + (b cross a) * 3);
        return int(c.z) * 100 + int(u.z) * 10 + int(-m.z);
    }
}
"#;
    let result = room_zscript::check(&[("cross.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    assert_eq!(rt.script_call("CrossT.Go").expect("Go"), 113);
}

/// `"" .. d` for a double field, and a `string speed` local shadowing a
/// double field of the same name (PB's footsteps).
#[test]
fn double_field_to_string_and_shadowing_local() {
    let src = r#"
class StrT
{
    double d;
    double speed;
    string Cat() { return ""..d; }
    int Shadow()
    {
        string speed;
        speed = "creep";
        return speed.Length();
    }
    static int Go()
    {
        let o = new("StrT");
        o.d = 2.5;
        o.speed = 9;
        return o.Cat().Length() * 100 + o.Shadow() * 10 + int(o.speed) % 10;
    }
}
"#;
    let result = room_zscript::check(&[("str_t.zs".into(), src.into())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    // "2.500000" (8 chars), "creep" (5), speed 9.
    assert_eq!(rt.script_call("StrT.Go").expect("Go"), 859);
}

/// Run `Class.Fn` of `src` compiled with the UZDoom library.
fn uz_call(src: &str, func: &str) -> i64 {
    let mut rt = ZscriptRuntime::default();
    rt.load_program_stubbed(load_uzdoom_stdlib_with(src));
    rt.script_call(func).expect(func)
}

/// `Map<K, V>` and `MapIterator<K, V>` as UZDoom's native map structs: insert,
/// get, keys, removal, iteration, `foreach`, and a map field of an object
/// (Project Brutality's JSON objects).
#[test]
fn maps_iterators_and_foreach() {
    let src = r#"
class MapHolder { Map<String, Object> data; }
class MapT
{
    static int Ints()
    {
        Map<int, int> m;
        m.Insert(4, 40);
        m.Insert(5, 50);
        m.Insert(4, 41);
        int sum = 0;
        MapIterator<int, int> it;
        it.Init(m);
        while (it.Next()) sum += it.GetKey() * 1000 + it.GetValue();
        int missing = m.GetIfExists(9);
        bool has = m.CheckKey(5);
        m.Remove(5);
        // 9091 = 4041 + 5050.
        return sum * 100 + m.CountUsed() * 10 + has + missing;
    }
    static int Strings()
    {
        let h = new("MapHolder");
        h.data.Insert("a", h);
        h.data.Insert("bb", null);
        int n = 0;
        foreach (k, v : h.data) { n += k.Length() * (v ? 10 : 1); }
        Map<String, double> f;
        f.Insert("x", 2.5);
        return n * 100 + int(f.Get("x") * 10);
    }
    static int Each()
    {
        Map<int, int> m;
        m.Insert(1, 7);
        m.Insert(2, 8);
        int s = 0;
        foreach (v : m) s += v;
        Array<int> a;
        a.Push(3);
        a.Push(4);
        foreach (x : a) s += x * 100;
        return s;
    }
}
"#;
    assert_eq!(uz_call(src, "MapT.Ints"), 909_111);
    // "a" → 1 * 10, "bb" → 2 * 1; 2.5 * 10.
    assert_eq!(uz_call(src, "MapT.Strings"), 1225);
    assert_eq!(uz_call(src, "MapT.Each"), 715);
}

/// `out` parameters come back from functions that also return a value,
/// through nested calls and `return F(i)` (PB's JSON parser index).
#[test]
fn out_parameters_come_back_from_value_functions() {
    let src = r#"
class OutT
{
    static int Set7(out uint i) { i = 7; return 1; }
    static int Pass(out uint i) { return Set7(i); }
    static Object Obj(out int i, out string s, out double d) { i = 9; s = "abc"; d = 1.5; return null; }
    static void Bump(out int i) { i += 3; }
    static void Mid(out int i) { Bump(i); Bump(i); }
    static int Go()
    {
        uint a = 1;
        int r = Pass(a);
        int b = 0; string s; double d = 0;
        Object o = Obj(b, s, d);
        int c = 1;
        Mid(c);
        return a * 100000 + r * 10000 + b * 1000 + s.Length() * 100 + int(d * 10) + c * 0;
    }
    static int Go2() { int c = 1; Mid(c); return c; }
}
"#;
    assert_eq!(uz_call(src, "OutT.Go"), 719_315);
    assert_eq!(uz_call(src, "OutT.Go2"), 7);
}

/// UZDoom's mutating string methods change the variable or field they are
/// called on; `AppendFormat` formats mixed arguments.
#[test]
fn string_methods_mutate_their_receiver() {
    let src = r#"
class StrHolder { string s; }
class StrMutT
{
    string mine;
    static int Go()
    {
        string t = "ab";
        t.AppendFormat("%s%d", "cd", 12);
        string u;
        u.AppendCharacter(65);
        u.AppendCharacter(66);
        let h = new("StrHolder");
        string src = "hello";
        h.s.AppendFormat("%s", src.Mid(1, 3));
        h.s.Truncate(2);
        string v = " x ";
        v.StripRight();
        v.Replace("x", "yz");
        let o = new("StrMutT");
        o.mine = "q";
        o.Add();
        // "abcd12", "AB", "el", " yz", "q!".
        return t.Length() * 10000 + u.Length() * 1000 + h.s.Length() * 100 + v.Length() * 10 + o.mine.Length();
    }
    void Add() { mine.AppendFormat("!"); }
}
"#;
    assert_eq!(uz_call(src, "StrMutT.Go"), 62_232);
}

/// A one-character string next to an int is its character code: in
/// comparisons, `case` labels and `return` (PB's JSON parser).
#[test]
fn char_literals_compare_with_ints() {
    let src = r#"
class CharT
{
    static int Code() { return "\n"; }
    static int Go()
    {
        int c = 123;
        int d = 50;
        int r = 0;
        if (c == "{") r += 1;
        if (c != "}") r += 10;
        if (d >= "0" && d <= "9") r += 100;
        switch (c) { case "{": r += 1000; break; default: break; }
        uint e, n;
        string s = "{x";
        [e, n] = s.GetNextCodePoint(0);
        if (e == "{") r += 10000;
        return r * 100 + Code();
    }
}
"#;
    assert_eq!(uz_call(src, "CharT.Go"), 1_111_110);
}

/// Named arguments go to their parameters, also on a call through another
/// object; script struct locals and fields are values, passed as `out`.
#[test]
fn named_arguments_and_struct_values() {
    let src = r#"
struct TraceData { int Hit; double Dist; }
class NamedT
{
    TraceData field;
    int Fill(out TraceData d) { d.Hit = 3; d.Dist = 2.0; return 1; }
    int Mix(int a, int b = 5, int c = 7) { return a * 100 + b * 10 + c; }
    static int Go()
    {
        let o = new("NamedT");
        TraceData t;
        o.Fill(t);
        o.Fill(o.field);
        let other = new("NamedT");
        return (t.Hit * 10 + o.field.Hit) * 1000 + other.Mix(1, c: 2);
    }
}
"#;
    assert_eq!(uz_call(src, "NamedT.Go"), 33_152);
}

/// Locals of a nested block go out of scope with it, so a long function
/// with many such blocks stays under the 255-register limit.
#[test]
fn nested_block_locals_free_their_registers() {
    let mut body = String::new();
    for k in 0..120 {
        body.push_str(&format!(
            "if (n >= 0) {{ double a{k} = n + {k}; double b{k} = a{k} * 2; s += b{k} - a{k}; }}\n"
        ));
    }
    let src = format!(
        "class RegT {{ static int Go() {{ double s = 0; int n = 1; {body} return int(s); }} }}"
    );
    let result = room_zscript::check(&[("reg_t.zs".into(), src)]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let mut rt = ZscriptRuntime::default();
    rt.load_program(result.program.unwrap());
    // Sum of (1 + k) for k in 0..120.
    assert_eq!(rt.script_call("RegT.Go").expect("Go"), 7260);
}

/// In a lenient compile, a store to an engine field Room does not model is
/// dropped with a warning instead of emptying the whole function (PB's
/// `psp.sprite = …`).
#[test]
fn lenient_unknown_field_store_keeps_the_function() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).expect("zscript.txt");
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let Some(rest) = line.trim().strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = std::fs::read_to_string(root.join(path)).expect(path);
        sources.push((path.replace('\\', "/"), text));
    }
    sources.push((
        "soft_t.zs".to_owned(),
        r#"
class SoftT : Weapon
{
    action void Swap(name imp, int layer = PSP_WEAPON)
    {
        let psp = player.findpsprite(layer);
        if (!psp) return;
        psp.sprite = getspriteindex(imp);
    }
}
"#
        .to_owned(),
    ));
    let result = room_zscript::check_lenient(&sources);
    let mine = |d: &&room_zscript::Diagnostic| d.message.contains("SoftT.");
    let warns: Vec<_> = result.diagnostics.iter().filter(mine).collect();
    assert!(
        warns.iter().any(|d| d
            .message
            .contains("store to an unsupported field is ignored")),
        "{warns:?}"
    );
    assert!(
        !warns
            .iter()
            .any(|d| d.message.contains("runs as an empty function")),
        "{warns:?}"
    );
}

/// With the UZDoom library, `players[n]` is a real `PlayerInfo` whose `mo`
/// is the pawn proxy, and `playeringame` is an array of the seats (PB's
/// `WorldLoaded` reads both).
#[test]
fn players_mo_and_playeringame_with_the_uzdoom_library() {
    let src = r#"
class SeatT
{
    static int Go()
    {
        int r = 0;
        for (int i = 0; i < MAXPLAYERS; i++)
        {
            if (playeringame[i])
            {
                PlayerPawn p = players[i].mo;
                if (p) r += 10 + i;
            }
        }
        return r;
    }
}
"#;
    assert_eq!(uz_call(src, "SeatT.Go"), 10);
}

/// A ZScript weapon's own `action` functions run from its psprite states
/// with the owner as `self` and the weapon as `invoker` (UZDoom
/// `CallAction`): their fields persist, `player.cmd.buttons` reads the input,
/// `ResolveState` jumps the psprite, and the library gun actions they call
/// (`A_WeaponReady`, `A_GiveInventory`) run as the playsim's.
#[test]
fn zscript_weapon_action_functions_run_with_invoker() {
    let deco = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, classes, states).unwrap();
    let prog = load_uzdoom_stdlib_with(
        r#"
class TestGun : Weapon
{
    int readies;
    int shots;
    action state A_TestReady(int flags = WRF_NOBOB)
    {
        invoker.readies++;
        if (player.cmd.buttons & BT_ATTACK) return ResolveState("Fire");
        A_WeaponReady(flags);
        return null;
    }
    action void A_TestShoot(string item = "Clip", int amount = 3)
    {
        invoker.shots++;
        A_GiveInventory(item, amount);
    }
    States
    {
    Ready:
        PISG A 1 A_TestReady;
        Loop;
    Deselect:
        PISG A 1 A_Lower;
        Loop;
    Select:
        PISG A 1 A_Raise;
        Loop;
    Fire:
        PISG B 4 A_TestShoot;
        PISG C 4;
        Goto Ready;
    }
}
"#,
    );
    w.load_zscript_program_stubbed(prog);
    let gun = w.classes.find_name("TestGun").expect("TestGun registered");
    let clip = w.classes.find_name("Clip").unwrap();
    let ready = w.states.find_label(gun, "Ready").unwrap();
    let fire = w.states.find_label(gun, "Fire").unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    {
        let p = w.players[0].as_mut().unwrap();
        p.ready_weapon = Some(gun);
        p.pending_weapon = None;
        p.pspr_y = 32.0; // WEAPON_TOP: raised
        p.pspr_state = Some(ready);
        p.pspr_tics = 1;
    }
    let clips = w.inventory.count(mo, clip);
    // Idle: the Ready function runs every tic and the gun never fires.
    for _ in 0..10 {
        w.set_ticcmds(&[room_sim::TicCmd::default()]);
        w.tick();
        assert_eq!(w.players[0].as_ref().unwrap().pspr_state, Some(ready));
    }
    assert_eq!(w.inventory.count(mo, clip), clips, "no shot while idle");
    // Fire held: `ResolveState("Fire")` jumps, `A_TestShoot` gives 3 clips.
    let mut cmd = room_sim::TicCmd::default();
    cmd.buttons.0 |= room_sim::Buttons::ATTACK;
    w.set_ticcmds(&[cmd]);
    w.tick();
    assert_eq!(w.players[0].as_ref().unwrap().pspr_state, Some(fire));
    assert_eq!(w.inventory.count(mo, clip), clips + 3);
    // The weapon's script object kept its counters between calls.
    let vm = w.zscript.vm.as_ref().unwrap();
    let layout = vm
        .program
        .classes
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("TestGun"))
        .unwrap();
    let (readies, shots) = (
        usize::from(layout.fields["readies"].1),
        usize::from(layout.fields["shots"].1),
    );
    let weapon_obj = w
        .zscript
        .actor_fields
        .iter()
        .filter(|(k, _)| **k >= 0xc000_0000)
        .map(|(_, h)| *h)
        .next()
        .expect("weapon object");
    match vm.heap.get(weapon_obj) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => {
            assert_eq!(
                fields_i[readies], 11,
                "Ready ran 10 idle tics + the firing one"
            );
            assert_eq!(fields_i[shots], 1);
        }
        other => panic!("{other:?}"),
    }
}

/// A ZScript projectile's `SpecialMissileHit` decides the hit (PB's bullets):
/// it passes through its shooter, and hurts the monster it reaches with its
/// own `Default`-block damage via `victim.DamageMobj(self, target, …)`.
#[test]
fn zs_special_missile_hit_passes_shooter_and_damages_victim() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor TestBolt
{
  Radius 2
  Height 2
  Speed 20
  Damage 0
  PROJECTILE
  States
  {
  Spawn:
    MISL A -1
    Stop
  Death:
    MISL B 1
    Stop
  }
}
actor TankZombie : ZombieMan
{
  Health 1000
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let zs = r#"
class TestBolt : Actor
{
    int boltDamage;
    property BoltDamage: boltDamage;
    Default
    {
        TestBolt.BoltDamage 37;
    }
    override int SpecialMissileHit(Actor victim)
    {
        if (victim == target) return 1;
        if (victim.bSHOOTABLE)
        {
            victim.DamageMobj(self, target, boltDamage, 'None');
        }
        return 0;
    }
}
"#;
    // Against the vendored UZDoom library, as a mod's ZScript loads.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.to_owned(),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("bolt.zs".into(), zs.into()));
    let result = room_zscript::check(&sources);
    assert!(
        result.ok(),
        "{:?}",
        result
            .diagnostics
            .iter()
            .filter(|d| d.severity == room_zscript::Severity::Error)
            .take(5)
            .collect::<Vec<_>>()
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(result.program.unwrap());
    let tank = classes.find_name("TankZombie").unwrap();
    let at = |x: f64| room_core::math::DVec3::new(x, 128.0, 0.0);
    let shooter = w
        .spawn_at(tank, at(100.0), room_core::math::Angle::ZERO)
        .unwrap();
    let victim = w
        .spawn_at(tank, at(400.0), room_core::math::Angle::ZERO)
        .unwrap();
    // Start inside the shooter, flying at the victim.
    let bolt_class = classes.find_name("TestBolt").unwrap();
    let bolt = w
        .spawn_at(
            bolt_class,
            room_core::math::DVec3::new(100.0, 128.0, 30.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    {
        let b = w.actors.get_mut(bolt).unwrap();
        b.target = Some(shooter);
        b.vel = room_core::math::DVec3::new(20.0, 0.0, 0.0);
    }
    for _ in 0..40 {
        w.tick();
    }
    assert_eq!(
        w.actors.get(shooter).unwrap().health,
        1000,
        "the bolt passes through its shooter"
    );
    assert_eq!(
        w.actors.get(victim).unwrap().health,
        1000 - 37,
        "the victim takes the script's Default-block damage"
    );
    assert!(
        w.actors.get(bolt).is_none_or(|b| b.vel.x().abs() < 1e-9),
        "the bolt stopped at the victim"
    );
}

/// The vendored UZDoom library plus `extra`, as a mod's ZScript loads.
fn uzdoom_program(extra: &str) -> room_zvm::CompiledProgram {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"');
            sources.push((
                path.to_owned(),
                std::fs::read_to_string(root.join(path)).unwrap(),
            ));
        }
    }
    sources.push(("mod.zs".into(), extra.into()));
    let result = room_zscript::check(&sources);
    assert!(
        result.ok(),
        "{:?}",
        result
            .diagnostics
            .iter()
            .filter(|d| d.severity == room_zscript::Severity::Error)
            .take(5)
            .collect::<Vec<_>>()
    );
    result.program.unwrap()
}

/// `Actor.Spawn` hands the script a live actor at once (UZDoom): fields
/// it writes on it land on the new actor, and a pointer to it (`tracer`)
/// names the new actor. It used to return `null` (PB's casings and muzzle
/// sparks did nothing).
#[test]
fn zs_spawn_returns_a_live_actor() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor SpawnerThing
{
  Radius 8
  Height 16
  +NOGRAVITY
  States
  {
  Spawn:
    TROO A -1
    Stop
  }
}
actor SpawnedThing
{
  Radius 8
  Height 16
  Health 40
  +NOGRAVITY
  States
  {
  Spawn:
    TROO B -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let program = uzdoom_program(
        r#"
class SpawnerThing : Actor
{
    int made;
    override void Tick()
    {
        if (made) return;
        made = 1;
        let mo = Spawn("SpawnedThing", pos + (200, 0, 0));
        if (mo)
        {
            mo.vel = (0, 3, 0);
            mo.angle = 90;
            mo.tracer = self;
            tracer = mo;
            made = 2 + mo.health;
        }
    }
}
"#,
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(program);
    let spawner_class = classes.find_name("SpawnerThing").unwrap();
    let spawned_class = classes.find_name("SpawnedThing").unwrap();
    let spawner = w
        .spawn_at(
            spawner_class,
            room_core::math::DVec3::new(100.0, 128.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    for _ in 0..3 {
        w.tick();
    }
    let made: Vec<_> = w
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| w.actors.get(id).is_some_and(|a| a.class == spawned_class))
        .collect();
    assert_eq!(made.len(), 1, "one actor spawned");
    let child = made[0];
    let c = w.actors.get(child).unwrap();
    assert!(
        (c.pos.x() - 300.0).abs() < 1e-6,
        "at pos + 200: {:?}",
        c.pos
    );
    assert!((c.angle.degrees() - 90.0).abs() < 1e-6);
    assert!(c.pos.y() > 128.0 + 3.0, "moving by its vel: {:?}", c.pos);
    assert_eq!(c.tracer, Some(spawner), "the script set its tracer");
    assert_eq!(w.actors.get(spawner).unwrap().tracer, Some(child));
}

/// `Sector.NextLowestFloorAt` answers `(height, sector)`: PB's
/// `GetFloorInfo` reads `RealFloor.FloorPlane.Normal` from the second value.
#[test]
fn zs_next_lowest_floor_at_returns_the_sector() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        r#"
actor ProbeThing
{
  Radius 8
  Height 16
  States
  {
  Spawn:
    TROO A -1
    Stop
  }
}
"#
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let program = uzdoom_program(
        r#"
class ProbeThing : Actor
{
    override void Tick()
    {
        let s = FloorSector;
        if (!s) { special1 = 1; return; }
        double z; Sector r;
        [z, r] = s.NextLowestFloorAt(pos.x, pos.y, pos.z);
        if (!r) { special1 = 2; return; }
        let n = r.FloorPlane.Normal;
        special1 = 3;
        special2 = int(n.z * 100);
    }
}
"#,
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(program);
    let cid = classes.find_name("ProbeThing").unwrap();
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(100.0, 128.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    for _ in 0..2 {
        w.tick();
    }
    let a = w.actors.get(id).unwrap();
    assert_eq!(
        (a.ext.special1, a.ext.special2),
        (3, 100),
        "a flat floor's normal is up"
    );
}

/// `RotateVector` turns the vector (PB's ground movement builds its
/// acceleration with it); it used to read as a zero vector.
#[test]
fn zs_rotate_vector_turns_the_vector() {
    let deco = format!(
        "{}\n{}",
        include_str!("../../../testdata/actors/doom_basic.txt"),
        "actor ProbeThing\n{\n  Radius 8\n  Height 16\n  States\n  {\n  Spawn:\n    TROO A -1\n    Stop\n  }\n}\n"
    );
    let ir = parse_decorate(&deco).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let program = uzdoom_program(
        r#"
class ProbeThing : Actor
{
    override void Tick()
    {
        int fm = 12800;
        int sm = 0;
        Vector2 acc = RotateVector((fm, -sm), 90);
        special1 = int(acc.x);
        special2 = int(acc.y);
    }
}
"#,
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    w.load_zscript_program(program);
    let cid = classes.find_name("ProbeThing").unwrap();
    let id = w
        .spawn_at(
            cid,
            room_core::math::DVec3::new(100.0, 128.0, 0.0),
            room_core::math::Angle::ZERO,
        )
        .unwrap();
    w.tick();
    let a = w.actors.get(id).unwrap();
    assert_eq!((a.ext.special1, a.ext.special2), (0, 12800));
}

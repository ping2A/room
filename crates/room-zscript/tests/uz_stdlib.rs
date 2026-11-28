//! UZ3: vendored UZDoom `zscript.txt` loads as one translation unit.

// Fixture helpers run only under `cargo test`; a missing vendored file should panic.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

fn uzdoom_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom")
}

fn load_tree() -> Vec<(String, String)> {
    let root = uzdoom_root();
    let entry = fs::read_to_string(root.join("zscript.txt")).expect("zscript.txt");
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = fs::read_to_string(root.join(path))
            .unwrap_or_else(|err| panic!("missing include {path}: {err}"));
        sources.push((path.replace('\\', "/"), text));
    }
    sources
}

#[test]
fn uzdoom_zscript_txt_resolves_every_include() {
    let sources = load_tree();
    // zscript.txt plus every #include.
    assert!(sources.len() > 200, "includes: {}", sources.len());
    let result = room_zscript::check(&sources);
    let errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == room_zscript::Severity::Error)
        .collect();
    let mut buckets: Vec<(String, usize)> = Vec::new();
    for diag in &errors {
        let key = diag
            .message
            .split_once('\'')
            .map(|(head, _)| head.trim().to_owned())
            .unwrap_or_else(|| diag.message.clone());
        if let Some((_, n)) = buckets.iter_mut().find(|(k, _)| k == &key) {
            *n += 1;
        } else {
            buckets.push((key, 1));
        }
    }
    buckets.sort_by_key(|b| std::cmp::Reverse(b.1));
    for (key, n) in buckets.iter().take(6) {
        eprintln!("uz3 {n:5} {key}");
    }
    // Measured 2026-11-03: the vendored stdlib type-checks and lowers with
    // no errors (diagnostics name the stdlib file and line of each).
    assert!(
        errors.is_empty(),
        "{} errors, first: {}",
        errors.len(),
        errors.first().map(|d| d.format_line()).unwrap_or_default()
    );
    let prog = result.program.expect("lowered program");
    let mut table = room_zvm::NativeTable::with_builtins();
    let imports = prog.native_imports.len();
    let stubs = table.bind_imports(&prog.native_imports);
    let bound = imports - stubs;
    eprintln!("uz3 natives {imports} implemented {bound} stubs {stubs}");
    // UZ3 acceptance (docs/36): every import binds to a native — computed,
    // queued for the playsim, engine-managed, or a host native that traps
    // with the host's name — none is a silent return-zero stub.
    assert_eq!(stubs, 0, "return-zero stubs left");
    assert!(
        prog.native_imports
            .iter()
            .all(|name| table.id_ci(name).is_some()),
        "stdlib native imports must bind"
    );
}

/// Weapon `action` functions against the UZDoom library: `invoker` comes
/// from the host (`Object.__invoker`), parameter defaults are recorded for
/// the host, a dropped call of an object-returning library action still calls
/// it, `statelabel` travels as a string, and the library bodies Room routes
/// to the host queue their action with the arguments in order.
#[test]
fn weapon_action_functions_lower_for_the_host() {
    let mut sources = load_tree();
    sources.push((
        "mod.zs".to_owned(),
        r#"
class TW : Weapon
{
    int heat;
    action state A_TestAct(int flags = WRF_ALLOWRELOAD, double sp = 1.5, statelabel next = "Fire")
    {
        invoker.heat += flags;
        if (invoker.heat > 100) return ResolveState(next);
        A_FireProjectile("Rocket", sp, false, 0, 0, 0, 0);
        A_WeaponReady(flags);
        return null;
    }
}
"#
        .to_owned(),
    ));
    let result = room_zscript::check(&sources);
    let mut program = result.program.expect("program");
    assert_eq!(
        program.action_defaults.get("TW.A_TestAct"),
        Some(&vec![
            "16".to_owned(),
            "1.5".to_owned(),
            "\"Fire\"".to_owned()
        ])
    );
    let f = program.func("TW.A_TestAct").expect("function").clone();
    assert_eq!(
        f.params.iter().map(|p| p.bank).collect::<Vec<_>>(),
        [3, 0, 1, 2],
        "self, int, double, statelabel (string)"
    );
    let natives: Vec<&str> = f
        .code
        .windows(2)
        .filter(|w| room_zvm::unpack(w[0]).0 == room_zvm::Op::CallNat as u8)
        .filter_map(|w| program.native_imports.get(w[1] as usize))
        .map(String::as_str)
        .collect();
    assert!(natives.contains(&"Object.__invoker"), "{natives:?}");
    assert!(
        !natives.iter().any(|n| n.ends_with("A_FireProjectile")),
        "the library A_FireProjectile is called, not an untyped native: {natives:?}"
    );

    // Route the library actions as the playsim does and run the function.
    let routed = program.route_to_host_actions(
        &["StateProvider", "Weapon"],
        &["A_FireProjectile", "A_WeaponReady"],
    );
    assert!(routed >= 2, "{routed}");
    let class_id = program.class_by_name["TW"];
    let layout = program.classes[class_id as usize].clone();
    let mut vm = room_zvm::Zvm::new_with_stubs(program);
    let me = vm
        .heap
        .new_object_f(class_id, layout.n_i, layout.n_a, layout.n_s, layout.n_f);
    let weapon = vm
        .heap
        .new_object_f(class_id, layout.n_i, layout.n_a, layout.n_s, layout.n_f);
    vm.play.invoker = weapon;
    vm.call_method_banks("TW.A_TestAct", me, &[16], &[2.5], &["Fire"], &[])
        .expect("call");
    let ops: Vec<_> = std::mem::take(&mut vm.play.actor_ops)
        .into_iter()
        .filter_map(|op| match op {
            room_zvm::ZsActorOp::ActionText { name, args } => Some((name, args)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ops,
        [
            (
                "A_FireProjectile".to_owned(),
                ["\"Rocket\"", "2.5", "0", "0", "0", "0", "0"]
                    .map(str::to_owned)
                    .to_vec()
            ),
            ("A_WeaponReady".to_owned(), vec!["16".to_owned()]),
        ]
    );
    assert!(vm.last_a.is_null(), "returned null");
    // `invoker.heat` landed on the weapon object, not on `self`.
    let slot = usize::from(layout.fields["heat"].1);
    let heat = |h| match vm.heap.get(h) {
        Some(room_zvm::ObjKind::Object { fields_i, .. }) => fields_i[slot],
        _ => -1,
    };
    assert_eq!((heat(me), heat(weapon)), (0, 16));
    // Past 100 it returns `ResolveState(next)`: a token the host resolves.
    if let Some(room_zvm::ObjKind::Object { fields_i, .. }) = vm.heap.get_mut(weapon) {
        fields_i[slot] = 200;
    }
    vm.call_method_banks("TW.A_TestAct", me, &[16], &[2.5], &["AltFire"], &[])
        .expect("call");
    assert_eq!(
        room_zvm::state_token_label(&vm.heap, vm.last_a).as_deref(),
        Some("AltFire")
    );
}

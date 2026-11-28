//! Compile the UZDoom library plus one file and dump the named functions:
//! `zs_dump <uzdoom-root> <file.zs> <name-substring>...`.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::PathBuf;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(&a[1]);
    let entry = std::fs::read_to_string(root.join("zscript.txt")).unwrap();
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let Some(rest) = line.trim().strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = std::fs::read_to_string(root.join(path)).unwrap();
        sources.push((path.replace('\\', "/"), text));
    }
    sources.push(("mod.zs".to_owned(), std::fs::read_to_string(&a[2]).unwrap()));
    let r = room_zscript::check(&sources);
    for d in &r.diagnostics {
        if d.severity == room_zscript::Severity::Error {
            println!("ERR {d:?}");
        }
    }
    let p = r.program.unwrap();
    for f in &p.functions {
        if a[3..].iter().any(|n| {
            f.name
                .to_ascii_lowercase()
                .contains(&n.to_ascii_lowercase())
        }) {
            println!("== {} params={:?}", f.name, f.params);
            for (i, w) in f.code.iter().enumerate() {
                let (op, x, y, z) = room_zvm::unpack(*w);
                let op = room_zvm::Op::from_u8(op);
                println!("  {i:3} {op:?} {x} {y} {z} (raw {w:#x})");
            }
            println!("  const_s={:?} const_i={:?}", f.const_s, f.const_i);
        }
    }
    for (k, v) in &p.action_defaults {
        if a[3..]
            .iter()
            .any(|n| k.to_ascii_lowercase().contains(&n.to_ascii_lowercase()))
        {
            println!("defaults {k} {v:?}");
        }
    }
    for (k, v) in &p.field_defaults {
        if a[3..]
            .iter()
            .any(|n| k.to_ascii_lowercase().contains(&n.to_ascii_lowercase()))
        {
            println!("field defaults {k} {v:?}");
        }
    }
    let mut table = room_zvm::NativeTable::with_builtins();
    let known: Vec<String> = p
        .native_imports
        .iter()
        .filter(|n| table.id_ci(n).is_none())
        .cloned()
        .collect();
    let _ = table.bind_imports(&p.native_imports);
    println!(
        "imports with no native (stubbed): {:?}",
        known
            .iter()
            .filter(|n| !room_zvm::is_host_action_import(n))
            .collect::<Vec<_>>()
    );
    println!(
        "imports containing: {:?}",
        p.native_imports
            .iter()
            .filter(|n| a[3..]
                .iter()
                .any(|x| n.to_ascii_lowercase().contains(&x.to_ascii_lowercase())))
            .collect::<Vec<_>>()
    );
}

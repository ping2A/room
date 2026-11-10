//! Semantic analysis (ZS1).

use crate::ast::*;
use crate::diag::{Diagnostic, Severity};
use crate::span::Span;
use crate::types::ClassTable;
use std::collections::BTreeMap;

/// One method in a class/struct symbol table.
#[derive(Clone, Debug)]
pub struct MethodSym {
    /// Method name.
    pub name: String,
    /// `virtual` / `override` / `abstract`-as-virtual.
    pub virtual_: bool,
    /// `override`.
    pub override_: bool,
    /// `native`.
    pub native: bool,
    /// `static`.
    pub static_: bool,
    /// Declaring class/struct.
    pub owner: String,
}

/// One field symbol.
#[derive(Clone, Debug)]
pub struct FieldSym {
    /// Field name.
    pub name: String,
    /// Declaring type.
    pub owner: String,
    /// `native`.
    pub native: bool,
}

/// Analyzed program summary.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// Class names → parent.
    pub classes: BTreeMap<String, Option<String>>,
    /// Struct names.
    pub structs: Vec<String>,
    /// Enum names.
    pub enums: Vec<String>,
    /// Top-level / nested const names.
    pub consts: Vec<String>,
    /// Methods keyed by `Owner.name` (case-preserving owner).
    pub methods: BTreeMap<String, MethodSym>,
    /// Fields keyed by `Owner.name`.
    pub fields: BTreeMap<String, FieldSym>,
    /// Virtual method slots per class (inheritance order, child overrides replace).
    pub vtables: BTreeMap<String, Vec<String>>,
    /// Rough instance layout: field count per class (own fields only).
    pub layouts: BTreeMap<String, usize>,
    /// Method count (all).
    pub method_count: usize,
    /// Native method names referenced.
    pub native_refs: Vec<String>,
    /// Version.
    pub version: Option<String>,
    /// Non-fatal diagnostics gathered during analysis.
    pub warnings: Vec<Diagnostic>,
    /// Typed classes, fields, and methods (`docs/34` §14.3).
    pub table: ClassTable,
}

/// Type-check / build symbol tables. Uses Room stdlib stubs when present in unit.
pub fn analyze(unit: &TranslationUnit) -> Result<Analysis, Vec<Diagnostic>> {
    let mut diags = Vec::new();
    let mut analysis = Analysis {
        version: unit.version.clone(),
        ..Default::default()
    };
    let mut defined: BTreeMap<String, Span> = BTreeMap::new();

    for decl in &unit.decls {
        match decl {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                let key = c.name.to_ascii_lowercase();
                if let Some(_prev) = defined.insert(key, c.span) {
                    if matches!(decl, Decl::Class(_)) {
                        diags.push(Diagnostic::error(
                            "",
                            c.span,
                            format!("duplicate class '{}'", c.name),
                        ));
                    }
                }
                analysis.classes.insert(c.name.clone(), c.parent.clone());
                ingest_type_members(&c.name, &c.members, &mut analysis, &mut diags);
            }
            Decl::Struct(s) => {
                analysis.structs.push(s.name.clone());
                ingest_type_members(&s.name, &s.members, &mut analysis, &mut diags);
            }
            Decl::Enum(e) => {
                analysis.enums.push(e.name.clone());
                for (v, _) in &e.variants {
                    analysis.consts.push(format!("{}.{}", e.name, v));
                }
            }
            Decl::Const(c) => analysis.consts.push(c.name.clone()),
        }
    }

    // Parent resolution.
    for (name, parent) in &analysis.classes {
        if let Some(p) = parent {
            let known = analysis.classes.keys().any(|k| k.eq_ignore_ascii_case(p))
                || is_stdlib_class(&p.to_ascii_lowercase());
            if !known {
                diags.push(Diagnostic::warn(
                    "",
                    Span::dummy(),
                    format!("class '{name}' parent '{p}' not defined in this unit (stdlib stub?)"),
                ));
            }
        }
    }

    // Override resolution: `override` must match a virtual on an ancestor.
    for (key, meth) in &analysis.methods {
        if !meth.override_ {
            continue;
        }
        let Some(owner) = analysis
            .classes
            .keys()
            .find(|k| k.eq_ignore_ascii_case(&meth.owner))
            .cloned()
        else {
            continue;
        };
        if !ancestor_has_virtual(&analysis, &owner, &meth.name) {
            // Soft: parent may be a Room stdlib stub without the virtual listed yet.
            if analysis
                .classes
                .get(&owner)
                .and_then(|p| p.as_ref())
                .is_some_and(|p| is_stdlib_class(&p.to_ascii_lowercase()))
                || analysis
                    .classes
                    .get(&owner)
                    .and_then(|p| p.as_ref())
                    .is_none()
            {
                diags.push(Diagnostic::warn(
                    "",
                    Span::dummy(),
                    format!(
                        "override '{key}' — no virtual '{}.{}' found on ancestors (stub?)",
                        meth.owner, meth.name
                    ),
                ));
            } else {
                diags.push(Diagnostic::error(
                    "",
                    Span::dummy(),
                    format!(
                        "method '{}' overrides nothing on ancestors of '{}'",
                        meth.name, meth.owner
                    ),
                ));
            }
        }
    }

    build_vtables(&mut analysis);

    let table = ClassTable::from_unit(unit, &mut diags);
    table.check_bodies(unit, &mut diags);
    analysis.table = table;

    if diags.iter().any(|d| d.severity == Severity::Error) {
        Err(diags)
    } else {
        analysis.native_refs.sort();
        analysis.native_refs.dedup();
        analysis.structs.sort();
        analysis.structs.dedup();
        analysis.enums.sort();
        analysis.enums.dedup();
        analysis.consts.sort();
        analysis.consts.dedup();
        analysis.warnings = diags;
        Ok(analysis)
    }
}

fn ingest_type_members(
    owner: &str,
    members: &[Member],
    analysis: &mut Analysis,
    diags: &mut Vec<Diagnostic>,
) {
    let mut field_count = 0usize;
    for m in members {
        match m {
            Member::Method(meth) => {
                analysis.method_count += 1;
                let key = format!("{}.{}", owner, meth.name);
                let sym = MethodSym {
                    name: meth.name.clone(),
                    virtual_: meth.quals.virtual_ || meth.quals.override_,
                    override_: meth.quals.override_,
                    native: meth.quals.native,
                    static_: meth.quals.static_,
                    owner: owner.to_owned(),
                };
                if meth.quals.native {
                    analysis.native_refs.push(key.clone());
                }
                analysis.methods.insert(key, sym);
            }
            Member::Field(f) => {
                for n in &f.names {
                    field_count += 1;
                    let key = format!("{owner}.{n}");
                    analysis.fields.insert(
                        key,
                        FieldSym {
                            name: n.clone(),
                            owner: owner.to_owned(),
                            native: f.quals.native,
                        },
                    );
                }
            }
            Member::NestedEnum(e) => {
                analysis.enums.push(format!("{}.{}", owner, e.name));
                for (v, _) in &e.variants {
                    analysis.consts.push(format!("{}.{}.{}", owner, e.name, v));
                }
            }
            Member::NestedConst(c) => {
                analysis.consts.push(format!("{}.{}", owner, c.name));
            }
            Member::NestedStruct(s) => {
                analysis.structs.push(format!("{}.{}", owner, s.name));
                ingest_type_members(
                    &format!("{}.{}", owner, s.name),
                    &s.members,
                    analysis,
                    diags,
                );
            }
            _ => {}
        }
    }
    analysis.layouts.insert(owner.to_owned(), field_count);
}

fn ancestor_has_virtual(analysis: &Analysis, class: &str, method: &str) -> bool {
    let mut cur = analysis.classes.get(class).and_then(|p| p.clone());
    let mut guard = 0u32;
    while let Some(parent) = cur {
        if guard > 64 {
            break;
        }
        guard += 1;
        let key = format!("{parent}.{method}");
        if analysis
            .methods
            .get(&key)
            .is_some_and(|m| m.virtual_ || m.override_ || m.native)
        {
            return true;
        }
        // Case-insensitive method lookup on parent.
        if analysis.methods.iter().any(|(k, m)| {
            let virt = m.virtual_ || m.override_ || m.native;
            (m.owner.eq_ignore_ascii_case(&parent) && m.name.eq_ignore_ascii_case(method) && virt)
                || (k.eq_ignore_ascii_case(&format!("{parent}.{method}")) && virt)
        }) {
            return true;
        }
        if is_stdlib_class(&parent.to_ascii_lowercase())
            && matches!(
                method.to_ascii_lowercase().as_str(),
                "tick"
                    | "postbeginplay"
                    | "beginplay"
                    | "onregister"
                    | "playerentered"
                    | "worldthingspawned"
                    | "destroy"
                    | "ondestroy"
                    | "getprotrusion"
                    | "getblend"
                    | "draw"
                    | "ticker"
            )
        {
            return true;
        }
        cur = analysis
            .classes
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&parent))
            .and_then(|(_, p)| p.clone());
        if cur.is_none() && is_stdlib_class(&parent.to_ascii_lowercase()) {
            break;
        }
    }
    // Engine virtuals that live on Object / status bars even when a parent link is missing.
    matches!(
        method.to_ascii_lowercase().as_str(),
        "ondestroy"
            | "getprotrusion"
            | "getblend"
            | "draw"
            | "ticker"
            | "specialdropaction"
            | "trypickup"
            | "use"
            | "travelled"
    )
}

fn build_vtables(analysis: &mut Analysis) {
    let classes: Vec<String> = analysis.classes.keys().cloned().collect();
    for class in classes {
        let mut chain = Vec::new();
        let mut cur = Some(class.clone());
        let mut guard = 0u32;
        while let Some(c) = cur {
            if guard > 64 {
                break;
            }
            guard += 1;
            chain.push(c.clone());
            cur = analysis.classes.get(&c).and_then(|p| p.clone());
        }
        chain.reverse();
        let mut slots: Vec<String> = Vec::new();
        for c in &chain {
            let mut owned: Vec<_> = analysis
                .methods
                .values()
                .filter(|m| m.owner.eq_ignore_ascii_case(c) && (m.virtual_ || m.override_))
                .map(|m| m.name.clone())
                .collect();
            owned.sort();
            owned.dedup();
            for name in owned {
                if let Some(pos) = slots.iter().position(|s| s.eq_ignore_ascii_case(&name)) {
                    slots[pos] = name;
                } else {
                    slots.push(name);
                }
            }
        }
        analysis.vtables.insert(class, slots);
    }
}

fn is_stdlib_class(lower: &str) -> bool {
    matches!(
        lower,
        "object"
            | "thinker"
            | "actor"
            | "inventory"
            | "weapon"
            | "playerpawn"
            | "eventhandler"
            | "staticeventhandler"
            | "playerevent"
            | "worldevent"
            | "base"
            | "array"
            | "string"
            | "cvar"
            | "wads"
            | "console"
            | "level"
            | "levellocals"
            | "playerinfo"
            | "statelabel"
            | "state"
            | "textureid"
            | "spriteid"
            | "color"
            | "sound"
            | "name"
            | "vector2"
            | "vector3"
            | "quat"
            | "fquat"
            | "fvector3"
            | "map"
            | "function"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_sources;

    #[test]
    fn bd_like_with_base_builds_symbols_and_vtables() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ws = root.join("../..");
        let base =
            std::fs::read_to_string(ws.join("wadsrc/room/zscript/base.zs")).expect("base.zs");
        let dmg = std::fs::read_to_string(ws.join("testdata/zscript/bd_like/DamageHandler.zc"))
            .expect("DamageHandler");
        let wcs =
            std::fs::read_to_string(ws.join("testdata/zscript/bd_like/BD_WCS.zc")).expect("BD_WCS");
        let sources = vec![
            ("base.zs".into(), base),
            ("DamageHandler.zc".into(), dmg),
            ("BD_WCS.zc".into(), wcs),
        ];
        let parsed = parse_sources(&sources);
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error),
            "{:?}",
            parsed.diagnostics
        );
        let a = analyze(&parsed.unit).expect("analyze");
        assert!(a.classes.contains_key("Script_DamageMult"));
        assert!(a.classes.contains_key("EventHandler"));
        assert!(a.methods.contains_key("Script_DamageMult.PlayerEntered"));
        assert!(
            a.vtables
                .get("Script_DamageMult")
                .is_some_and(|v| v.iter().any(|m| m == "PlayerEntered"))
        );
        assert!(
            a.fields.contains_key("Actor.health")
                || a.fields.keys().any(|k| k.ends_with(".health"))
        );
    }

    #[test]
    fn const_methods_are_read_only_from_4_15_1() {
        let body = r#"
class T
{
    int count;
    Array<int> list;
    Actor other;
    int Peek() const { return count; }
    void Bump() const { count++; }
    void Set() const { self.count = 3; }
    void Add() const { list[0] += 1; }
    void Poke() const { other.health = 1; int count2 = 1; count2++; }
    void Fine() unsafe(const) { count = 4; }
    static void Free() { int count = 1; count++; }
}
"#;
        let errors = |version: &str| -> Vec<String> {
            let src = format!("version \"{version}\"\n{body}");
            crate::check(&[("t.zs".into(), src)])
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| d.message.clone())
                .collect()
        };
        let new = errors("4.15.1");
        assert_eq!(new.len(), 3, "{new:?}");
        assert!(new.iter().all(|m| m.contains("const method")), "{new:?}");
        assert!(new.iter().any(|m| m.contains("'list'")), "{new:?}");
        let old = errors("4.14");
        assert!(
            old.iter().all(|m| !m.contains("const method")),
            "{old:?}"
        );
    }

    #[test]
    fn deprecated_classes_warn_from_their_version() {
        let body = r#"
class OldBox deprecated("4.15.1", "Use Map<String, String> instead")
{
    static OldBox Make() { return new("OldBox"); }
}
class User
{
    void Go() { OldBox b = OldBox.Make(); let c = new("OldBox"); }
}
"#;
        let warns = |version: &str| -> Vec<String> {
            let src = format!("version \"{version}\"\n{body}");
            let r = crate::check(&[("t.zs".into(), src)]);
            assert!(
                r.diagnostics.iter().all(|d| d.severity != Severity::Error),
                "{:?}",
                r.diagnostics
            );
            r.diagnostics
                .iter()
                .filter(|d| d.message.contains("deprecated since"))
                .map(|d| d.message.clone())
                .collect()
        };
        let new = warns("4.15.1");
        assert_eq!(new.len(), 1, "{new:?}");
        assert!(new[0].contains("OldBox") && new[0].contains("Map<String"), "{new:?}");
        assert!(warns("4.14").is_empty());
    }

    #[test]
    fn unknown_name_is_an_error() {
        let src = "class T { void F() { noSuch(); } }\n";
        let result = crate::check(&[("t.zs".into(), src.into())]);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error && d.message.contains("noSuch")),
            "{:?}",
            result.diagnostics
        );
    }

    #[test]
    fn engine_globals_arrays_and_flags_typecheck() {
        let src = r#"
class EventHandler {}
struct _ native
{
    int players[8];
    native readonly GameInfoStruct gameinfo;
}
struct GameInfoStruct native
{
    int defaultdropstyle;
}
class Actor
{
    flagdef NoGravity: flags, 1;
    struct WeaponStates { int melee; }
    void Tick()
    {
        bNoGravity = true;
        bSolid = false;
        int n = random(1, 4);
        int n2 = random[mbf21]();
        int spec = Door_Open;
        bool locks[6];
        if (locks[0] || sv_unlimited_pickup) { spec = n2; }
        vector3 vel;
        double z = vel.z;
        color tint;
        int red = tint.r;
        textureid pic;
        if (pic.IsValid()) { red = 1; }
        Actor other;
        if (other.bIsMonster) { z = red; }
        readonly<Actor> defmobj = other;
        if (defmobj.bSolid) { z = 1; }
        WeaponStates ws;
        int melee = ws.melee;
        if (melee > 0) { z = melee; }
        let info = gameinfo;
        if (info.defaultdropstyle > 0) { z = n; }
        if (gameinfo.defaultdropstyle > 0 && players[0] == n) { }
        if (stateinfo != null && stateinfo.mStateType == 1) { }
        String s = String.Format("%i", n);
    }
}
"#;
        let parsed = crate::parse_sources(&[("t.zs".into(), src.into())]);
        let parse_errs: Vec<_> = parsed
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect();
        assert!(parse_errs.is_empty(), "{parse_errs:?}");
        let mut diags = Vec::new();
        let _ = crate::ClassTable::from_unit(&parsed.unit, &mut diags);
        let errs: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn real_bd_and_probe_typecheck() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ws = root.join("../..");
        let read = |p: &str| std::fs::read_to_string(ws.join(p)).expect(p);
        let sources = vec![
            (
                "zscript.zc".into(),
                read("testdata/zscript/bd_real/zscript.zc"),
            ),
            (
                "zscript/BD_WCS.zc".into(),
                read("testdata/zscript/bd_real/BD_WCS.zc"),
            ),
            (
                "zscript/DamageHandler.zc".into(),
                read("testdata/zscript/bd_real/DamageHandler.zc"),
            ),
            (
                "probe.zs".into(),
                read("testdata/zscript/conformance/probe.zs"),
            ),
        ];
        let result = crate::check(&sources);
        assert!(
            result.ok(),
            "{}",
            result
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

//! ZScript types, class table, and name resolution (`docs/34` §14.3.1–14.3.2).

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::span::Span;
use room_zvm::Bank;
use std::collections::{BTreeMap, BTreeSet};

/// Implicit conversion emitted by the later lowering pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conv {
    /// Same type.
    None,
    /// Int → float.
    I2F,
    /// Float → int.
    F2I,
    /// Number / object / string → bool.
    ToBool,
    /// String → name id.
    StrToName,
    /// Name → string.
    NameToStr,
    /// String literal → `class<T>`.
    StrToClass(String),
}

/// A ZScript type and the register bank it occupies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZType {
    /// No value.
    Void,
    /// `bool`.
    Bool,
    /// `int` / `uint` / sized ints. All live in the `I` bank.
    Int {
        /// Width in bits.
        bits: u8,
        /// Signed integer.
        signed: bool,
    },
    /// `float` / `double`.
    Double,
    /// Interned name.
    Name,
    /// Sound id.
    Sound,
    /// Packed colour.
    Color,
    /// State id.
    State,
    /// Sprite id.
    SpriteId,
    /// Texture id.
    TextureId,
    /// Byte string.
    String,
    /// Two consecutive float registers.
    Vector2,
    /// Three consecutive float registers.
    Vector3,
    /// `class<T>` (class id). Also the type of a class name used as a value.
    Class(String),
    /// Object reference.
    Object(String),
    /// By-reference struct.
    Struct(String),
    /// `Array<T>`.
    DynArray(Box<ZType>),
    /// Inline fixed array.
    FixedArray(Box<ZType>, u32),
    /// `Map<K,V>`.
    Map(Box<ZType>, Box<ZType>),
    /// Literal `null`.
    Null,
    /// Already reported. Suppresses follow-on errors.
    Error,
}

impl ZType {
    /// Register bank.
    #[must_use]
    pub fn bank(&self) -> Bank {
        match self {
            ZType::Double | ZType::Vector2 | ZType::Vector3 => Bank::F,
            ZType::String => Bank::S,
            ZType::Object(_)
            | ZType::Struct(_)
            | ZType::DynArray(_)
            | ZType::Map(..)
            | ZType::Null => Bank::A,
            _ => Bank::I,
        }
    }

    /// How many registers the value occupies.
    #[must_use]
    pub fn width(&self) -> u8 {
        match self {
            ZType::Vector2 => 2,
            ZType::Vector3 => 3,
            _ => 1,
        }
    }

    /// Int / float / bool / name / colour.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            ZType::Int { .. } | ZType::Double | ZType::Bool | ZType::Name | ZType::Color
        )
    }

    fn int() -> Self {
        ZType::Int {
            bits: 32,
            signed: true,
        }
    }
}

/// Conversion allowed for assignment and argument passing.
#[must_use]
pub fn coerce(from: &ZType, to: &ZType, classes: &ClassTable) -> Option<Conv> {
    use ZType::*;
    Some(match (from, to) {
        (a, b) if a == b => Conv::None,
        (Error, _) | (_, Error) => Conv::None,
        (Int { .. } | Bool, Double) => Conv::I2F,
        (Double, Int { .. }) => Conv::F2I,
        (Int { .. }, Bool)
        | (Double, Bool)
        | (Object(_), Bool)
        | (String, Bool)
        | (Struct(_), Bool) => Conv::ToBool,
        (Bool, Int { .. }) | (Int { .. }, Int { .. }) => Conv::None,
        (Null, Object(_) | Class(_) | Struct(_) | DynArray(_)) => Conv::None,
        (Object(a), Object(b)) if classes.is_subclass(a, b) => Conv::None,
        (Class(a), Class(b)) if classes.is_subclass(a, b) => Conv::None,
        (String, Name) => Conv::StrToName,
        (Name, String) => Conv::NameToStr,
        (String, Class(base)) => Conv::StrToClass(base.clone()),
        _ => return None,
    })
}

/// One field, inherited fields first.
#[derive(Clone, Debug)]
pub struct FieldInfo {
    /// Name as written.
    pub name: String,
    /// Type.
    pub ty: ZType,
    /// Bank.
    pub bank: Bank,
    /// Slot within the bank, inherited included.
    pub slot: u16,
    /// `native` binding name, when declared native.
    pub native: Option<String>,
    /// `readonly`.
    pub readonly: bool,
    /// `ui` / `play` / `clearscope` when set.
    pub scope: Option<String>,
    /// Excluded from prediction rollback.
    pub norollback: bool,
}

/// One parameter.
#[derive(Clone, Debug)]
pub struct Param {
    /// Name.
    pub name: String,
    /// Type.
    pub ty: ZType,
    /// `out`.
    pub out: bool,
    /// Default expression, if any.
    pub default: Option<Expr>,
}

/// Function signature.
#[derive(Clone, Debug)]
pub struct FuncSig {
    /// Parameters, `self` is not included (the call site pushes it).
    pub params: Vec<Param>,
    /// Return values. Empty means `void`.
    pub rets: Vec<ZType>,
    /// Trailing `...`.
    pub varargs: bool,
}

/// One method.
#[derive(Clone, Debug)]
pub struct MethodInfo {
    /// Declaring class.
    pub owner: String,
    /// Name as written.
    pub name: String,
    /// Signature.
    pub sig: FuncSig,
    /// `static`.
    pub is_static: bool,
    /// `virtual` or `override`.
    pub is_virtual: bool,
    /// Vtable slot once layouts are assigned.
    pub vslot: Option<u16>,
    /// `native`.
    pub native: bool,
    /// Scope keyword.
    pub scope: Option<String>,
    /// `action`.
    pub action: bool,
    /// Filled by lowering.
    pub func: Option<u32>,
}

/// One class or struct.
#[derive(Clone, Debug)]
pub struct ClassInfo {
    /// Name as written.
    pub name: String,
    /// Parent class. Structs have none.
    pub parent: Option<String>,
    /// Struct rather than class.
    pub is_struct: bool,
    /// `native`.
    pub native: bool,
    /// Inherited fields first, then own fields.
    pub fields: Vec<FieldInfo>,
    /// Register counts per [`Bank`], inherited included.
    pub n_slots: [u16; 4],
    /// Methods keyed by lower-case name. Own methods only; lookup walks parents.
    pub methods: BTreeMap<String, MethodInfo>,
    /// Virtual slots `(owner, method)`.
    pub vtable: Vec<(String, String)>,
    /// Constants and enum variants in this class.
    pub consts: BTreeMap<String, ZType>,
    /// Enum names declared directly on this class.
    pub enums: BTreeMap<String, i64>,
    /// Raw `Default` block.
    pub defaults: Option<DefaultBlock>,
    /// Raw `States` block.
    pub states: Option<StatesBlock>,
    /// Descends from `Actor`.
    pub actor_derived: bool,
}

/// All classes, structs, and top-level constants.
#[derive(Clone, Debug, Default)]
pub struct ClassTable {
    /// Lower-case name → class.
    pub by_lower: BTreeMap<String, ClassInfo>,
    /// Top-level constants (`MAXPLAYERS`, …).
    pub globals: BTreeMap<String, ZType>,
}

impl ClassTable {
    /// Build layouts from a parsed unit. Unknown types are errors.
    pub fn from_unit(unit: &TranslationUnit, diags: &mut Vec<Diagnostic>) -> Self {
        let mut table = ClassTable::default();
        for decl in &unit.decls {
            match decl {
                Decl::Class(c) | Decl::Mixin(c) => table.declare_class(c, false, diags),
                Decl::ExtendClass(c) => table.declare_class(c, true, diags),
                Decl::Struct(s) => table.declare_struct(s, diags),
                Decl::Enum(e) => {
                    for (i, (v, _)) in e.variants.iter().enumerate() {
                        table
                            .globals
                            .entry(v.to_ascii_lowercase())
                            .or_insert(ZType::int());
                        let _ = i;
                    }
                }
                Decl::Const(c) => {
                    table
                        .globals
                        .insert(c.name.to_ascii_lowercase(), fold_type(&c.value));
                }
            }
        }
        for decl in &unit.decls {
            match decl {
                Decl::Enum(e) => table.note_type_name(&e.name),
                Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                    table.note_nested(&c.members, diags);
                }
                Decl::Struct(s) => {
                    table.note_nested(&s.members, diags);
                }
                Decl::Const(_) => {}
            }
        }
        if !table.by_lower.contains_key("stateinfo") {
            table.by_lower.insert(
                "stateinfo".into(),
                ClassInfo {
                    name: "StateInfo".into(),
                    parent: None,
                    is_struct: true,
                    native: true,
                    fields: vec![int_field("mStateType"), int_field("mPSPIndex")],
                    n_slots: [0; 4],
                    methods: BTreeMap::new(),
                    vtable: Vec::new(),
                    consts: BTreeMap::new(),
                    enums: BTreeMap::new(),
                    defaults: None,
                    states: None,
                    actor_derived: false,
                },
            );
        }
        table.layout_all();
        // `_Globals` fields are play-scope names (`globals.zs`).
        let from_globals: Vec<(String, ZType)> = table
            .get("_Globals")
            .map(|g| {
                g.fields
                    .iter()
                    .map(|f| (f.name.to_ascii_lowercase(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (name, ty) in from_globals {
            table.globals.entry(name).or_insert(ty);
        }
        // `struct _` is the engine global object (`gameinfo`, `players`, …).
        let from_engine: Vec<(String, ZType)> = table
            .get("_")
            .map(|g| {
                g.fields
                    .iter()
                    .map(|f| (f.name.to_ascii_lowercase(), f.ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (name, ty) in from_engine {
            table.globals.entry(name).or_insert(ty);
        }
        table
    }

    /// `a` is `b` or a descendant of `b` (case-insensitive).
    #[must_use]
    pub fn is_subclass(&self, a: &str, b: &str) -> bool {
        if a.eq_ignore_ascii_case(b) {
            return true;
        }
        let mut cur = self.get(a).and_then(|c| c.parent.clone());
        for _ in 0..64 {
            let Some(name) = cur else {
                return false;
            };
            if name.eq_ignore_ascii_case(b) {
                return true;
            }
            cur = self.get(&name).and_then(|c| c.parent.clone());
        }
        false
    }

    /// Method on `class` or an ancestor.
    #[must_use]
    pub fn lookup_method(&self, class: &str, name: &str) -> Option<&MethodInfo> {
        let want = name.to_ascii_lowercase();
        let mut cur = Some(class.to_owned());
        for _ in 0..64 {
            let Some(name) = cur else {
                return None;
            };
            let Some(info) = self.get(&name) else {
                return None;
            };
            if let Some(m) = info.methods.get(&want) {
                return Some(m);
            }
            cur = info.parent.clone();
        }
        None
    }

    /// Fewest required arguments on this method or an ancestor.
    ///
    /// Overrides often omit a default that the base declaration still has.
    #[must_use]
    pub fn min_required_args(&self, class: &str, method: &str) -> Option<usize> {
        let want = method.to_ascii_lowercase();
        let mut cur = Some(class.to_owned());
        let mut best = None;
        for _ in 0..64 {
            let Some(name) = cur else {
                break;
            };
            let Some(info) = self.get(&name) else {
                break;
            };
            if let Some(m) = info.methods.get(&want) {
                let n = if m.sig.varargs {
                    0
                } else {
                    m.sig.params.iter().filter(|p| p.default.is_none()).count()
                };
                best = Some(best.map_or(n, |b: usize| b.min(n)));
            }
            cur = info.parent.clone();
        }
        best
    }

    /// Field on `class` or an ancestor.
    #[must_use]
    pub fn lookup_field(&self, class: &str, name: &str) -> Option<&FieldInfo> {
        let want = name.to_ascii_lowercase();
        let mut cur = Some(class.to_owned());
        for _ in 0..64 {
            let Some(name) = cur else {
                return None;
            };
            let Some(info) = self.get(&name) else {
                return None;
            };
            if let Some(f) = info
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&want))
            {
                return Some(f);
            }
            cur = info.parent.clone();
        }
        None
    }

    /// Constant on `class` or an ancestor.
    #[must_use]
    pub fn lookup_const(&self, class: &str, name: &str) -> Option<ZType> {
        let want = name.to_ascii_lowercase();
        let mut cur = Some(class.to_owned());
        for _ in 0..64 {
            let Some(name) = cur else {
                return None;
            };
            let Some(info) = self.get(&name) else {
                return None;
            };
            if let Some(ty) = info.consts.get(&want) {
                return Some(ty.clone());
            }
            cur = info.parent.clone();
        }
        None
    }

    /// Class or struct by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ClassInfo> {
        self.by_lower.get(&name.to_ascii_lowercase())
    }

    fn declare_class(&mut self, c: &ClassDecl, extend: bool, diags: &mut Vec<Diagnostic>) {
        let key = c.name.to_ascii_lowercase();
        if extend {
            if !self.by_lower.contains_key(&key) {
                diags.push(Diagnostic::error(
                    "",
                    c.span,
                    format!("extend class '{}' has no class to extend", c.name),
                ));
                return;
            }
        } else if self.by_lower.contains_key(&key) {
            diags.push(Diagnostic::error(
                "",
                c.span,
                format!("duplicate class '{}'", c.name),
            ));
            return;
        }
        let entry = self.by_lower.entry(key).or_insert_with(|| ClassInfo {
            name: c.name.clone(),
            parent: c.parent.clone(),
            is_struct: false,
            native: c.flags.native,
            fields: Vec::new(),
            n_slots: [0; 4],
            methods: BTreeMap::new(),
            vtable: Vec::new(),
            consts: BTreeMap::new(),
            enums: BTreeMap::new(),
            defaults: None,
            states: None,
            actor_derived: false,
        });
        if c.parent.is_some() {
            entry.parent = c.parent.clone();
        }
        entry.native |= c.flags.native;
        push_members(entry, &c.members, diags);
    }

    fn note_nested(&mut self, members: &[Member], diags: &mut Vec<Diagnostic>) {
        for m in members {
            match m {
                Member::NestedEnum(e) => self.note_type_name(&e.name),
                Member::NestedStruct(s) => self.declare_struct(s, diags),
                _ => {}
            }
        }
    }

    /// An enum name is a type (`EMorphFlags`) even though its variants are ints.
    fn note_type_name(&mut self, name: &str) {
        let key = name.to_ascii_lowercase();
        if key.is_empty() || self.by_lower.contains_key(&key) {
            return;
        }
        self.by_lower.insert(
            key,
            ClassInfo {
                name: name.to_owned(),
                parent: None,
                is_struct: true,
                native: true,
                fields: Vec::new(),
                n_slots: [0; 4],
                methods: BTreeMap::new(),
                vtable: Vec::new(),
                consts: BTreeMap::new(),
                enums: BTreeMap::new(),
                defaults: None,
                states: None,
                actor_derived: false,
            },
        );
    }

    fn declare_struct(&mut self, s: &StructDecl, diags: &mut Vec<Diagnostic>) {
        let key = s.name.to_ascii_lowercase();
        if let Some(entry) = self.by_lower.get_mut(&key) {
            // `extend struct` is parsed as another struct with the same name.
            if entry.is_struct {
                push_members(entry, &s.members, diags);
                return;
            }
            diags.push(Diagnostic::error(
                "",
                s.span,
                format!("duplicate struct '{}'", s.name),
            ));
            return;
        }
        let mut info = ClassInfo {
            name: s.name.clone(),
            parent: None,
            is_struct: true,
            native: false,
            fields: Vec::new(),
            n_slots: [0; 4],
            methods: BTreeMap::new(),
            vtable: Vec::new(),
            consts: BTreeMap::new(),
            enums: BTreeMap::new(),
            defaults: None,
            states: None,
            actor_derived: false,
        };
        push_members(&mut info, &s.members, diags);
        self.by_lower.insert(key, info);
    }

    fn layout_all(&mut self) {
        let names: Vec<String> = self.by_lower.keys().cloned().collect();
        let mut done = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        for n in names {
            self.layout_one(&n, &mut done, &mut visiting);
        }
    }

    fn layout_one(
        &mut self,
        name: &str,
        done: &mut BTreeSet<String>,
        visiting: &mut BTreeSet<String>,
    ) {
        let key = name.to_ascii_lowercase();
        if done.contains(&key) {
            return;
        }
        if !visiting.insert(key.clone()) {
            return;
        }
        let parent = self.get(&key).and_then(|c| c.parent.clone());
        if let Some(p) = &parent {
            if self.get(p).is_some() {
                self.layout_one(p, done, visiting);
            }
        }
        let parent_fields = parent
            .as_ref()
            .and_then(|p| self.get(p).map(|c| c.fields.clone()))
            .unwrap_or_default();
        let parent_slots = parent
            .as_ref()
            .and_then(|p| self.get(p).map(|c| c.n_slots))
            .unwrap_or([0; 4]);
        let mut vtable = parent
            .as_ref()
            .and_then(|p| self.get(p).map(|c| c.vtable.clone()))
            .unwrap_or_default();
        let actor_derived = parent.as_ref().is_some_and(|p| {
            p.eq_ignore_ascii_case("actor") || self.get(p).is_some_and(|c| c.actor_derived)
        }) || name.eq_ignore_ascii_case("actor");
        let Some(class) = self.by_lower.get_mut(&key) else {
            return;
        };
        let own = std::mem::take(&mut class.fields);
        let mut n_slots = parent_slots;
        let mut fields = parent_fields;
        for mut f in own {
            let b = f.ty.bank() as usize;
            f.slot = n_slots[b];
            n_slots[b] = n_slots[b].saturating_add(u16::from(f.ty.width()));
            f.bank = f.ty.bank();
            fields.push(f);
        }
        class.fields = fields;
        class.n_slots = n_slots;
        class.actor_derived = actor_derived;
        let owner = class.name.clone();
        for method in class.methods.values_mut() {
            if method.is_static {
                continue;
            }
            if let Some(idx) = vtable
                .iter()
                .position(|(_, n)| n.eq_ignore_ascii_case(&method.name))
            {
                method.vslot = Some(u16::try_from(idx).unwrap_or(u16::MAX));
                method.is_virtual = true;
                vtable[idx] = (owner.clone(), method.name.clone());
            } else if method.is_virtual {
                method.vslot = u16::try_from(vtable.len()).ok();
                vtable.push((owner.clone(), method.name.clone()));
            }
        }
        class.vtable = vtable;
        visiting.remove(&key);
        done.insert(key);
    }

    /// Resolve every method body. An unknown name is an error.
    pub fn check_bodies(&self, unit: &TranslationUnit, diags: &mut Vec<Diagnostic>) {
        self.check_declared_types(unit, diags);
        for decl in &unit.decls {
            let (name, members) = match decl {
                Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                    (c.name.as_str(), &c.members)
                }
                Decl::Struct(s) => (s.name.as_str(), &s.members),
                _ => continue,
            };
            let Some(info) = self.get(name) else {
                continue;
            };
            check_members(self, info, members, diags);
        }
    }
}

fn push_members(info: &mut ClassInfo, members: &[Member], diags: &mut Vec<Diagnostic>) {
    for m in members {
        match m {
            Member::Field(f) => {
                let ty = type_from_ref(&f.ty, &BTreeMap::new());
                if matches!(ty, ZType::Error) {
                    diags.push(Diagnostic::error(
                        "",
                        f.span,
                        format!("unknown type in field of '{}'", info.name),
                    ));
                }
                for n in &f.names {
                    info.fields.push(FieldInfo {
                        name: n.clone(),
                        ty: ty.clone(),
                        bank: ty.bank(),
                        slot: 0,
                        native: f.quals.native.then(|| n.clone()),
                        readonly: f.quals.readonly,
                        scope: f.quals.scope.clone(),
                        norollback: f.quals.norollback,
                    });
                }
            }
            Member::Method(meth) => {
                let ret = type_from_ref(&meth.ret, &BTreeMap::new());
                let rets = if matches!(ret, ZType::Void) {
                    Vec::new()
                } else {
                    vec![ret]
                };
                let params = meth
                    .params
                    .iter()
                    .map(|p| Param {
                        name: p.name.clone(),
                        ty: type_from_ref(&p.ty, &BTreeMap::new()),
                        out: p.mode.as_deref().is_some_and(|m| m.contains("out")),
                        default: p.default.clone(),
                    })
                    .collect();
                let varargs = meth.quals.scope.as_deref().is_some_and(|s| s == "vararg")
                    || meth.name.eq_ignore_ascii_case("Printf")
                    || meth.name.eq_ignore_ascii_case("Format");
                info.methods.insert(
                    meth.name.to_ascii_lowercase(),
                    MethodInfo {
                        owner: info.name.clone(),
                        name: meth.name.clone(),
                        sig: FuncSig {
                            params,
                            rets,
                            varargs,
                        },
                        is_static: meth.quals.static_,
                        is_virtual: meth.quals.virtual_ || meth.quals.override_,
                        vslot: None,
                        native: meth.quals.native,
                        scope: meth.quals.scope.clone(),
                        action: meth.quals.action,
                        func: None,
                    },
                );
            }
            Member::NestedConst(c) => {
                info.consts
                    .insert(c.name.to_ascii_lowercase(), fold_type(&c.value));
            }
            Member::NestedEnum(e) => {
                for (i, (v, _)) in e.variants.iter().enumerate() {
                    info.consts.insert(v.to_ascii_lowercase(), ZType::int());
                    info.enums
                        .insert(format!("{}.{}", e.name, v), i64::try_from(i).unwrap_or(0));
                }
            }
            Member::Default(d) => info.defaults = Some(d.clone()),
            Member::States(s) => info.states = Some(s.clone()),
            Member::FlagDef { name, .. } => {
                // `flagdef NoAutoFire` is the bool field `bNoAutoFire`.
                info.fields.push(int_field(&format!("b{name}")));
                if let Some(f) = info.fields.last_mut() {
                    f.ty = ZType::Bool;
                    f.bank = ZType::Bool.bank();
                }
            }
            Member::Property { name, fields, .. } => {
                let ty = fields
                    .iter()
                    .find_map(|src| {
                        info.fields
                            .iter()
                            .find(|field| field.name.eq_ignore_ascii_case(src))
                            .map(|field| field.ty.clone())
                    })
                    .unwrap_or_else(ZType::int);
                if info
                    .fields
                    .iter()
                    .any(|field| field.name.eq_ignore_ascii_case(name))
                {
                    continue;
                }
                let mut field = int_field(name);
                field.bank = ty.bank();
                field.ty = ty;
                info.fields.push(field);
            }
            Member::NestedStruct(_) | Member::MixinUse { .. } => {}
        }
    }
}

fn type_from_ref(ty: &TypeRef, _classes: &BTreeMap<String, ()>) -> ZType {
    match ty {
        TypeRef::Void => ZType::Void,
        TypeRef::Named(n) => named_type(n),
        TypeRef::ClassOf(n) => ZType::Class(n.clone()),
        TypeRef::Array(inner) => ZType::DynArray(Box::new(type_from_ref(inner, _classes))),
        TypeRef::Map(k, v) => ZType::Map(
            Box::new(type_from_ref(k, _classes)),
            Box::new(type_from_ref(v, _classes)),
        ),
        TypeRef::Readonly(inner) => type_from_ref(inner, _classes),
    }
}

fn named_type(n: &str) -> ZType {
    match n.to_ascii_lowercase().as_str() {
        "void" => ZType::Void,
        "bool" => ZType::Bool,
        "int" | "int32" | "uint" | "uint32" => ZType::Int {
            bits: 32,
            signed: !n.to_ascii_lowercase().starts_with('u'),
        },
        "int8" | "uint8" | "byte" => ZType::Int {
            bits: 8,
            signed: n.eq_ignore_ascii_case("int8"),
        },
        "int16" | "uint16" | "short" => ZType::Int {
            bits: 16,
            signed: !n.to_ascii_lowercase().starts_with('u'),
        },
        "int64" | "uint64" => ZType::Int {
            bits: 64,
            signed: !n.to_ascii_lowercase().starts_with('u'),
        },
        "double" | "float" => ZType::Double,
        "name" => ZType::Name,
        "string" => ZType::String,
        "sound" => ZType::Sound,
        "color" => ZType::Color,
        "state" | "statelabel" => ZType::State,
        "spriteid" => ZType::SpriteId,
        "textureid" => ZType::TextureId,
        "vector2" | "fvector2" => ZType::Vector2,
        "vector3" | "fvector3" | "vector4" | "quat" | "fquat" => ZType::Vector3,
        "voidptr" | "translationid" => ZType::int(),
        other => ZType::Object(other.to_owned()),
    }
}

/// Play-scope names that exist even when `globals.zs` was not parsed with the unit.
fn builtin_global(name: &str) -> Option<ZType> {
    match name.to_ascii_lowercase().as_str() {
        "gamestate" | "gs_level" | "skill" | "consoleplayer" | "gametic" | "maxplayers"
        | "ticrate" | "deathmatch" | "teamplay" | "timelimit" | "dmflags" | "dmflags2"
        | "compatflags" | "alwaysapplydmflags" => Some(ZType::int()),
        "multiplayer" => Some(ZType::Bool),
        "level" => Some(ZType::Object("LevelLocals".into())),
        "players" => Some(ZType::DynArray(Box::new(ZType::Struct(
            "PlayerInfo".into(),
        )))),
        _ => None,
    }
}

fn int_field(name: &str) -> FieldInfo {
    FieldInfo {
        name: name.to_owned(),
        ty: ZType::int(),
        bank: ZType::int().bank(),
        slot: 0,
        native: None,
        readonly: false,
        scope: None,
        norollback: false,
    }
}

fn fold_type(e: &Expr) -> ZType {
    match e {
        Expr::Float(_, _) => ZType::Double,
        Expr::String(_, _) => ZType::String,
        Expr::Name(_, _) => ZType::Name,
        _ => ZType::int(),
    }
}

fn string_method(name: &str) -> Option<ZType> {
    match name.to_ascii_lowercase().as_str() {
        "length" | "charcodeat" | "byteat" | "charat" | "toint" | "indexof" | "lastindexof"
        | "codepointcount" | "getnextcodepoint" => Some(ZType::int()),
        "mid"
        | "left"
        | "right"
        | "format"
        | "tolower"
        | "toupper"
        | "makelower"
        | "makeupper"
        | "replace"
        | "filter"
        | "appendformat"
        | "deletelastcharacter"
        | "charlower"
        | "charupper"
        | "appendcharacter" => Some(ZType::String),
        "split" => Some(ZType::int()),
        "todouble" | "tofloat" => Some(ZType::Double),
        "isempty" | "isnotempty" | "isdouble" => Some(ZType::Bool),
        "compare" | "comparenocase" => Some(ZType::int()),
        _ => None,
    }
}

fn array_method(name: &str, elem: &ZType) -> Option<ZType> {
    match name.to_ascii_lowercase().as_str() {
        "size" | "max" | "find" | "countused" | "length" | "pushv" => Some(ZType::int()),
        "isempty" => Some(ZType::Bool),
        "pop" => Some(elem.clone()),
        "push" | "insert" | "delete" | "clear" | "resize" | "shrink" | "grow" | "reserve"
        | "copy" | "move" => Some(ZType::Void),
        _ => None,
    }
}

struct Resolver<'a> {
    ct: &'a ClassTable,
    diags: &'a mut Vec<Diagnostic>,
    class: &'a ClassInfo,
    is_static: bool,
    scopes: Vec<BTreeMap<String, ZType>>,
}

impl Resolver<'_> {
    fn push(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: &str, ty: ZType) {
        if name.is_empty() {
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_ascii_lowercase(), ty);
        }
    }

    fn local(&self, name: &str) -> Option<ZType> {
        let key = name.to_ascii_lowercase();
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(&key) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn error(&mut self, span: Span, message: String) {
        self.diags.push(Diagnostic::error("", span, message));
    }

    fn check_members(&mut self, members: &[Member]) {
        for m in members {
            if let Member::Method(meth) = m {
                if let Some(body) = &meth.body {
                    self.is_static = meth.quals.static_;
                    self.scopes.clear();
                    self.push();
                    if !meth.quals.static_ {
                        let ty = if self.class.is_struct {
                            ZType::Struct(self.class.name.clone())
                        } else {
                            ZType::Object(self.class.name.clone())
                        };
                        self.bind("self", ty.clone());
                        self.bind("invoker", ty);
                    }
                    for p in &meth.params {
                        let ty = match type_from_ref(&p.ty, &BTreeMap::new()) {
                            ZType::Object(n) if self.ct.get(&n).is_some_and(|c| c.is_struct) => {
                                ZType::Struct(n)
                            }
                            other => other,
                        };
                        self.bind(&p.name, ty);
                    }
                    self.block(body);
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        self.push();
        for s in &block.stmts {
            self.stmt(s);
        }
        self.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local {
                ty,
                names,
                init,
                extra_inits,
                span,
            } => {
                let resolved = ty
                    .as_ref()
                    .map(|t| self.refine(type_from_ref(t, &BTreeMap::new()), *span));
                for extra in extra_inits {
                    if let Some(e) = extra {
                        let _ = self.expr_ty(e);
                    }
                }
                let init_ty = init.as_ref().map(|e| self.expr_ty(e));
                let fallback = resolved.or(init_ty).unwrap_or(ZType::int());
                for n in names {
                    self.bind(n, fallback.clone());
                }
            }
            Stmt::Expr { expr, .. }
            | Stmt::Case {
                value: Some(expr), ..
            } => {
                let _ = self.expr_ty(expr);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    let _ = self.expr_ty(v);
                }
            }
            Stmt::If {
                cond,
                then_b,
                else_b,
                ..
            } => {
                let _ = self.expr_ty(cond);
                self.block(then_b);
                if let Some(e) = else_b {
                    self.block(e);
                }
            }
            Stmt::While { cond, body, .. } | Stmt::DoWhile { cond, body, .. } => {
                let _ = self.expr_ty(cond);
                self.block(body);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.push();
                if let Some(i) = init {
                    self.stmt(i);
                }
                if let Some(c) = cond {
                    let _ = self.expr_ty(c);
                }
                if let Some(s) = step {
                    let _ = self.expr_ty(s);
                }
                self.block(body);
                self.pop();
            }
            Stmt::Switch { cond, body, .. } => {
                let _ = self.expr_ty(cond);
                self.block(body);
            }
            Stmt::Block(body) => self.block(body),
            Stmt::Case { .. } | Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn refine(&mut self, ty: ZType, span: Span) -> ZType {
        match ty {
            ZType::Object(n) => {
                if let Some(c) = self.ct.get(&n) {
                    if c.is_struct {
                        ZType::Struct(c.name.clone())
                    } else {
                        ZType::Object(c.name.clone())
                    }
                } else if is_builtin_name(&n) {
                    ZType::Object(n)
                } else {
                    self.error(span, format!("unknown type '{n}'"));
                    ZType::Error
                }
            }
            ZType::DynArray(inner) => ZType::DynArray(Box::new(self.refine(*inner, span))),
            other => other,
        }
    }

    fn expr_ty(&mut self, e: &Expr) -> ZType {
        match e {
            Expr::Int(_, _) => ZType::int(),
            Expr::Float(_, _) => ZType::Double,
            Expr::String(_, _) => ZType::String,
            Expr::Name(_, _) => ZType::Name,
            Expr::Ident(name, span) => self.ident(name, *span),
            Expr::Binary { lhs, rhs, .. } => {
                let _ = self.expr_ty(lhs);
                let _ = self.expr_ty(rhs);
                ZType::int()
            }
            Expr::Unary { expr, .. } => self.expr_ty(expr),
            Expr::Call { callee, args, span } => self.call(callee, args, *span),
            Expr::Member { obj, field, span } => self.member(obj, field, *span),
            Expr::Index { arr, index, span } => {
                if let Expr::Ident(name, _) = arr.as_ref() {
                    // `random[name](lo, hi)` — the index is a randomizer, not a value.
                    match name.to_ascii_lowercase().as_str() {
                        "random" | "random2" | "crandom" => return ZType::int(),
                        "frandom" | "frandom2" => return ZType::Double,
                        _ => {}
                    }
                }
                let _ = self.expr_ty(index);
                match self.expr_ty(arr) {
                    ZType::DynArray(inner) | ZType::FixedArray(inner, _) => *inner,
                    ZType::Map(_, val) => *val,
                    ZType::Vector2 | ZType::Vector3 => ZType::Double,
                    ZType::Error => ZType::Error,
                    ZType::String => ZType::int(),
                    other => {
                        self.error(*span, format!("cannot index {}", ty_name(&other)));
                        ZType::Error
                    }
                }
            }
            Expr::Cast { ty, expr, span } => {
                let _ = self.expr_ty(expr);
                if self.ct.get(ty).is_none() && !is_builtin_name(ty) {
                    self.error(*span, format!("unknown type '{ty}'"));
                    return ZType::Error;
                }
                self.refine(named_type(ty), *span)
            }
            Expr::Ternary {
                cond,
                then_e,
                else_e,
                ..
            } => {
                let _ = self.expr_ty(cond);
                let _ = self.expr_ty(else_e);
                self.expr_ty(then_e)
            }
            Expr::New { class, span } => {
                if class.is_empty() || self.ct.get(class).is_none() {
                    self.error(*span, format!("unknown class '{class}'"));
                    ZType::Error
                } else {
                    ZType::Object(class.clone())
                }
            }
            Expr::Tuple { elems, .. } => {
                for e in elems {
                    let _ = self.expr_ty(e);
                }
                ZType::Void
            }
        }
    }

    fn ident(&mut self, name: &str, span: Span) -> ZType {
        if name.is_empty() {
            return ZType::Error;
        }
        match name.to_ascii_lowercase().as_str() {
            "true" | "false" => return ZType::Bool,
            "null" | "nullptr" => return ZType::Null,
            "stateinfo" => return ZType::Struct("StateInfo".into()),
            "self" | "invoker" => {
                if self.is_static {
                    self.error(span, format!("'{name}' in a static method"));
                    return ZType::Error;
                }
                return if self.class.is_struct {
                    ZType::Struct(self.class.name.clone())
                } else {
                    ZType::Object(self.class.name.clone())
                };
            }
            "super" => {
                return self
                    .class
                    .parent
                    .clone()
                    .map_or(ZType::Object("Object".into()), ZType::Object);
            }
            _ => {}
        }
        if let Some(ty) = self.local(name) {
            return ty;
        }
        if !self.is_static {
            if let Some(f) = self.ct.lookup_field(&self.class.name, name) {
                return f.ty.clone();
            }
        }
        if let Some(ty) = self.ct.lookup_const(&self.class.name, name) {
            return ty;
        }
        if let Some(m) = self.ct.lookup_method(&self.class.name, name) {
            return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
        }
        if let Some(ty) = builtin_global(name) {
            return ty;
        }
        if let Some(ty) = self.ct.globals.get(&name.to_ascii_lowercase()) {
            return ty.clone();
        }
        if self.ct.get(name).is_some() {
            return ZType::Class(name.to_owned());
        }
        if let Some(ty) = builtin_value(name) {
            return ty;
        }
        if self.actor_flag(name) {
            return ZType::Bool;
        }
        if name.eq_ignore_ascii_case("default") {
            return if self.class.is_struct {
                ZType::Struct(self.class.name.clone())
            } else {
                ZType::Object(self.class.name.clone())
            };
        }
        if is_action_special(name) || is_cvar_name(name) {
            return ZType::int();
        }
        if let Some(ty) = builtin_call(name) {
            return ty;
        }
        self.error(span, format!("unknown name '{name}'"));
        ZType::Error
    }

    /// Engine-generated `bFlagName` bools on `Actor` (not declared in ZScript).
    fn actor_flag(&self, name: &str) -> bool {
        is_actor_flag_name(name) && self.ct.is_subclass(&self.class.name, "Actor")
    }

    fn member(&mut self, obj: &Expr, field: &str, span: Span) -> ZType {
        if field.is_empty() {
            return ZType::Error;
        }
        let ty = self.expr_ty(obj);
        if matches!(ty, ZType::Error) {
            return ZType::Error;
        }
        self.member_ty(&ty, field, span)
    }

    fn member_ty(&mut self, ty: &ZType, field: &str, span: Span) -> ZType {
        match ty {
            ZType::Error => ZType::Error,
            ZType::String => self
                .ct
                .lookup_method("String", field)
                .map(|m| m.sig.rets.first().cloned().unwrap_or(ZType::Void))
                .or_else(|| string_method(field))
                .unwrap_or_else(|| {
                    self.error(span, format!("unknown string member '{field}'"));
                    ZType::Error
                }),
            ZType::DynArray(inner) => array_method(field, inner).unwrap_or_else(|| {
                self.error(span, format!("unknown array member '{field}'"));
                ZType::Error
            }),
            ZType::Vector2 => vector_swizzle(field, false).unwrap_or_else(|| {
                self.error(span, format!("unknown member '{field}' on vector2"));
                ZType::Error
            }),
            ZType::Vector3 => vector_swizzle(field, true).unwrap_or_else(|| {
                self.error(span, format!("unknown member '{field}' on vector3"));
                ZType::Error
            }),
            ZType::Class(c) => {
                if let Some(m) = self.ct.lookup_method(c, field) {
                    return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                }
                if let Some(t) = self.ct.lookup_const(c, field) {
                    return t;
                }
                if field.eq_ignore_ascii_case("getparentclass")
                    || field.eq_ignore_ascii_case("getclassname")
                    || field.eq_ignore_ascii_case("getreplacement")
                {
                    return ZType::Class(c.clone());
                }
                if field.eq_ignore_ascii_case("isabstract") {
                    return ZType::Bool;
                }
                if field.len() > 3 && field[..3].eq_ignore_ascii_case("cr_") {
                    return ZType::int();
                }
                self.error(span, format!("unknown member '{c}.{field}'"));
                ZType::Error
            }
            ZType::State => match field.to_ascii_lowercase().as_str() {
                "nextstate" => ZType::State,
                "tics" | "frame" | "misc1" | "misc2" | "light" | "distanceto" => ZType::int(),
                "sprite" => ZType::SpriteId,
                "getspritetexture" => ZType::TextureId,
                "validatespriteframe" | "instatesequence" => ZType::Bool,
                _ if is_actor_flag_name(field) => ZType::Bool,
                _ => {
                    self.error(span, format!("unknown member '{field}' on state"));
                    ZType::Error
                }
            },
            ZType::Color => match field.to_ascii_lowercase().as_str() {
                "r" | "g" | "b" | "a" => ZType::int(),
                _ => {
                    self.error(span, format!("unknown member '{field}' on color"));
                    ZType::Error
                }
            },
            ZType::Int { .. } | ZType::Double => match field.to_ascii_lowercase().as_str() {
                "heightsec" => ZType::int(),
                "x" | "y" | "z" | "length" | "equal_epsilon" | "epsilon" | "nan" | "infinity"
                | "min" | "max" => {
                    if matches!(ty, ZType::Double) {
                        ZType::Double
                    } else {
                        ZType::int()
                    }
                }
                _ => {
                    self.error(span, format!("unknown member '{field}' on number"));
                    ZType::Error
                }
            },
            ZType::Map(_, _) => match field.to_ascii_lowercase().as_str() {
                "countused" | "size" | "count" | "get" | "getifexists" | "insert" => ZType::int(),
                "checkkey" => ZType::Bool,
                _ => {
                    self.error(span, format!("unknown member '{field}' on map"));
                    ZType::Error
                }
            },
            ZType::TextureId | ZType::SpriteId => match field.to_ascii_lowercase().as_str() {
                "isvalid" | "isnull" | "exists" | "isvoid" | "setinvalid" | "setnull" => {
                    ZType::Bool
                }
                "x" | "y" | "z" | "length" => ZType::Double,
                "getspritetexture" => ZType::TextureId,
                _ => {
                    self.error(span, format!("unknown member '{field}' on texture"));
                    ZType::Error
                }
            },
            ZType::Object(c) | ZType::Struct(c) => {
                if let Some(f) = self.ct.lookup_field(c, field) {
                    return f.ty.clone();
                }
                if let Some(m) = self.ct.lookup_method(c, field) {
                    return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                }
                if let Some(t) = self.ct.lookup_const(c, field) {
                    return t;
                }
                if is_actor_flag_name(field) && self.ct.is_subclass(c, "Actor") {
                    return ZType::Bool;
                }
                if field.eq_ignore_ascii_case("getclass")
                    || field.eq_ignore_ascii_case("getparentclass")
                    || field.eq_ignore_ascii_case("getclassname")
                {
                    return ZType::Class(c.clone());
                }
                if field.eq_ignore_ascii_case("getparentammo") {
                    return ZType::Class("Ammo".into());
                }
                if self.ct.is_subclass(c, "Actor") {
                    for extra in [
                        "Inventory",
                        "Weapon",
                        "PlayerPawn",
                        "PlayerInfo",
                        "Ammo",
                        "Powerup",
                        "PowerupGiver",
                    ] {
                        if let Some(f) = self.ct.lookup_field(extra, field) {
                            return f.ty.clone();
                        }
                        if let Some(m) = self.ct.lookup_method(extra, field) {
                            return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                        }
                    }
                }
                if field.eq_ignore_ascii_case("getspritetexture") {
                    return ZType::TextureId;
                }
                if field.len() > 3 && field[..3].eq_ignore_ascii_case("cr_") {
                    return ZType::int();
                }
                if c.to_ascii_lowercase().ends_with("iterator") {
                    match field.to_ascii_lowercase().as_str() {
                        "init" | "next" => return ZType::Bool,
                        "getkey" => return ZType::Name,
                        "getvalue" => return ZType::Object("Object".into()),
                        "destroy" => return ZType::Void,
                        _ => {}
                    }
                }
                if c.eq_ignore_ascii_case("function") && field.eq_ignore_ascii_case("call") {
                    return ZType::int();
                }
                if matches!(
                    field.to_ascii_lowercase().as_str(),
                    "destroy" | "ondestroy" | "isabstract"
                ) {
                    return if field.eq_ignore_ascii_case("isabstract") {
                        ZType::Bool
                    } else {
                        ZType::Void
                    };
                }
                if field.eq_ignore_ascii_case("default") {
                    return if self.ct.get(c).is_some_and(|info| info.is_struct) {
                        ZType::Struct(c.clone())
                    } else {
                        ZType::Object(c.clone())
                    };
                }
                self.error(span, format!("unknown member '{c}.{field}'"));
                ZType::Error
            }
            other => {
                // `.X` / `.Y` / `.Z` on a value that should have been a vector.
                if matches!(field.to_ascii_lowercase().as_str(), "x" | "y" | "z") {
                    return ZType::Double;
                }
                if field.eq_ignore_ascii_case("length") {
                    return ZType::int();
                }
                self.error(
                    span,
                    format!("unknown member '{}' on {}", field, ty_name(other)),
                );
                ZType::Error
            }
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], _span: Span) -> ZType {
        for a in args {
            let _ = self.expr_ty(a);
        }
        match callee {
            Expr::Ident(name, span) => {
                // `new("Class")` is a call when `new` is not a keyword.
                if name.eq_ignore_ascii_case("new") {
                    let class = args.iter().find_map(|a| match a {
                        Expr::String(s, _) | Expr::Name(s, _) => Some((s.as_str(), true)),
                        Expr::Ident(s, _) => Some((s.as_str(), false)),
                        _ => None,
                    });
                    return match class {
                        Some((c, _)) if self.ct.get(c).is_some() => ZType::Object(c.to_owned()),
                        // `new(cls)` — the class name is a value, not a literal.
                        Some((_, false)) => ZType::Object("Object".into()),
                        Some((c, true)) => {
                            self.error(*span, format!("unknown class '{c}'"));
                            ZType::Error
                        }
                        None => {
                            self.error(*span, "new() needs a class name".into());
                            ZType::Error
                        }
                    };
                }
                // `PlayerPawn(x)` / `ZC_TMapHack(x)` are casts, parsed as calls.
                if self.ct.get(name).is_some() || is_builtin_name(name) {
                    return self.refine(named_type(name), *span);
                }
                if let Some(ty) = builtin_call(name) {
                    return ty;
                }
                // `Door_Open`, `Floor_LowerToLowest`: action specials are global calls.
                if is_action_special(name) {
                    return ZType::int();
                }
                if let Some(m) = self.ct.lookup_method(&self.class.name, name) {
                    if m.is_static || !self.is_static {
                        self.check_arity(&self.class.name, m, args, *span);
                        return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                    }
                    self.error(
                        *span,
                        format!("instance method '{name}' in a static method"),
                    );
                    return ZType::Error;
                }
                self.error(*span, format!("unknown name '{name}'"));
                ZType::Error
            }
            Expr::Member { obj, field, span } => {
                let ty = self.expr_ty(obj);
                if matches!(ty, ZType::Error) {
                    return ZType::Error;
                }
                if let ZType::Class(c) = &ty {
                    if let Some(m) = self.ct.lookup_method(c, field) {
                        self.check_arity(c, m, args, *span);
                        return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                    }
                }
                if let ZType::Object(c) | ZType::Struct(c) = &ty {
                    if let Some(m) = self.ct.lookup_method(c, field) {
                        self.check_arity(c, m, args, *span);
                        return m.sig.rets.first().cloned().unwrap_or(ZType::Void);
                    }
                }
                self.member_ty(&ty, field, *span)
            }
            other => self.expr_ty(other),
        }
    }

    fn check_arity(&mut self, class: &str, m: &MethodInfo, args: &[Expr], span: Span) {
        if m.sig.varargs {
            return;
        }
        let required = self
            .ct
            .min_required_args(class, &m.name)
            .unwrap_or_else(|| m.sig.params.iter().filter(|p| p.default.is_none()).count());
        if args.len() < required {
            self.error(
                span,
                format!(
                    "call to '{}' needs {required} arguments, got {}",
                    m.name,
                    args.len()
                ),
            );
        }
    }
}

impl ClassTable {
    fn check_declared_types(&self, unit: &TranslationUnit, diags: &mut Vec<Diagnostic>) {
        for decl in &unit.decls {
            let members = match decl {
                Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => &c.members,
                Decl::Struct(s) => &s.members,
                _ => continue,
            };
            for m in members {
                match m {
                    Member::Field(f) => self.require_type(&f.ty, f.span, diags),
                    Member::Method(meth) => {
                        self.require_type(&meth.ret, meth.span, diags);
                        for p in &meth.params {
                            self.require_type(&p.ty, meth.span, diags);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn require_type(&self, ty: &TypeRef, span: Span, diags: &mut Vec<Diagnostic>) {
        match ty {
            TypeRef::Named(n) => {
                if !is_builtin_name(n)
                    && self.get(n).is_none()
                    && !n.to_ascii_lowercase().ends_with("iterator")
                {
                    diags.push(Diagnostic::error("", span, format!("unknown type '{n}'")));
                }
            }
            TypeRef::ClassOf(n) => {
                if !n.is_empty() && !is_builtin_name(n) && self.get(n).is_none() {
                    diags.push(Diagnostic::error("", span, format!("unknown type '{n}'")));
                }
            }
            TypeRef::Array(inner) | TypeRef::Readonly(inner) => {
                self.require_type(inner, span, diags)
            }
            TypeRef::Map(k, v) => {
                self.require_type(k, span, diags);
                self.require_type(v, span, diags);
            }
            TypeRef::Void => {}
        }
    }
}

fn check_members(
    ct: &ClassTable,
    info: &ClassInfo,
    members: &[Member],
    diags: &mut Vec<Diagnostic>,
) {
    let mut r = Resolver {
        ct,
        diags,
        class: info,
        is_static: false,
        scopes: Vec::new(),
    };
    r.check_members(members);
}

fn builtin_value(name: &str) -> Option<ZType> {
    match name.to_ascii_lowercase().as_str() {
        "string" | "vector2" | "vector3" | "fvector2" | "fvector3" | "vector4" | "quat"
        | "fquat" | "name" | "color" | "sound" | "textureid" | "spriteid" | "int" | "uint"
        | "double" | "float" | "bool" | "byte" | "short" => Some(named_type(name)),
        _ => None,
    }
}

fn builtin_call(name: &str) -> Option<ZType> {
    match name.to_ascii_lowercase().as_str() {
        "random" | "random2" | "crandom" | "abs" | "min" | "max" | "clamp" => Some(ZType::int()),
        "getclassname" => Some(ZType::String),
        "dot" => Some(ZType::Double),
        "frandom" | "frandom2" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
        | "sqrt" | "floor" | "ceil" | "round" | "vectorangle" => Some(ZType::Double),
        "getdefaultbytype" => Some(ZType::Object("Actor".into())),
        "getclass" => Some(ZType::Class("Actor".into())),
        _ => None,
    }
}

/// `sv_unlimited_pickup`, `m_tooltip_delay`: CVARINFO names used as globals.
fn is_cvar_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "paletteflash" | "idmypos" | "wipetype" | "dmflags3" | "compatflags2"
    ) || (name.contains('_') && name.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
}

fn is_action_special(name: &str) -> bool {
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "teleport" | "thrustthing" | "noisealert"
    ) {
        return true;
    }
    name.contains('_') && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_actor_flag_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].eq_ignore_ascii_case(&b'b') && bytes[1].is_ascii_uppercase()
}

fn vector_swizzle(field: &str, has_z: bool) -> Option<ZType> {
    match field.to_ascii_lowercase().as_str() {
        "x" | "y" | "length" | "angle" => Some(ZType::Double),
        "z" | "w" if has_z => Some(ZType::Double),
        "fromangles" | "xy" | "plusx" | "plusy" | "plusz" => Some(if has_z {
            ZType::Vector3
        } else {
            ZType::Vector2
        }),
        "xyz" if has_z => Some(ZType::Vector3),
        "unit" => Some(if has_z {
            ZType::Vector3
        } else {
            ZType::Vector2
        }),
        _ => None,
    }
}

fn is_builtin_name(n: &str) -> bool {
    matches!(
        n.to_ascii_lowercase().as_str(),
        "void"
            | "bool"
            | "int"
            | "uint"
            | "int8"
            | "uint8"
            | "byte"
            | "int16"
            | "uint16"
            | "short"
            | "int32"
            | "uint32"
            | "int64"
            | "uint64"
            | "double"
            | "float"
            | "string"
            | "name"
            | "sound"
            | "color"
            | "state"
            | "statelabel"
            | "spriteid"
            | "textureid"
            | "vector2"
            | "fvector2"
            | "vector3"
            | "fvector3"
            | "vector4"
            | "quat"
            | "fquat"
            | "class"
            | "voidptr"
            | "translationid"
            | "function"
            | "object"
    )
}

fn ty_name(ty: &ZType) -> String {
    match ty {
        ZType::Void => "void".into(),
        ZType::Bool => "bool".into(),
        ZType::Int { .. } => "int".into(),
        ZType::Double => "double".into(),
        ZType::String => "string".into(),
        ZType::Name => "name".into(),
        ZType::Object(n) | ZType::Struct(n) | ZType::Class(n) => n.clone(),
        ZType::DynArray(_) => "array".into(),
        ZType::Error => "error".into(),
        _ => "value".into(),
    }
}

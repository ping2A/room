//! ZScript AST.

use crate::span::Span;
use std::collections::BTreeMap;

/// A parsed translation unit.
#[derive(Clone, Debug, Default)]
pub struct TranslationUnit {
    /// Version string if present.
    pub version: Option<String>,
    /// Top-level declarations in order.
    pub decls: Vec<Decl>,
}

/// Top-level declaration.
#[derive(Clone, Debug)]
pub enum Decl {
    /// `class`.
    Class(ClassDecl),
    /// `struct`.
    Struct(StructDecl),
    /// `enum`.
    Enum(EnumDecl),
    /// `const`.
    Const(ConstDecl),
    /// `extend class`.
    ExtendClass(ClassDecl),
    /// `mixin class`.
    Mixin(ClassDecl),
}

/// Class / mixin / extend body.
#[derive(Clone, Debug, Default)]
pub struct ClassDecl {
    /// Name.
    pub name: String,
    /// Parent class.
    pub parent: Option<String>,
    /// `replaces X`.
    pub replaces: Option<String>,
    /// Flags.
    pub flags: ClassFlags,
    /// Mixins used (`mixin Foo;`).
    pub mixins: Vec<String>,
    /// Members.
    pub members: Vec<Member>,
    /// Span.
    pub span: Span,
}

/// Class-level flags / scopes.
#[derive(Clone, Debug, Default)]
pub struct ClassFlags {
    /// `abstract`
    pub abstract_: bool,
    /// `native`
    pub native: bool,
    /// `ui` / `play` / `clearscope`
    pub scope: Option<String>,
    /// `deprecated("version", "message")`.
    pub deprecated: Option<(String, String)>,
}

/// Struct declaration.
#[derive(Clone, Debug, Default)]
pub struct StructDecl {
    /// Name.
    pub name: String,
    /// Members (fields / methods).
    pub members: Vec<Member>,
    /// Span.
    pub span: Span,
}

/// Enum declaration.
#[derive(Clone, Debug, Default)]
pub struct EnumDecl {
    /// Name.
    pub name: String,
    /// Variants `Name [= expr]`.
    pub variants: Vec<(String, Option<Expr>)>,
    /// Span.
    pub span: Span,
}

/// Const declaration.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    /// Name.
    pub name: String,
    /// Value.
    pub value: Expr,
    /// Span.
    pub span: Span,
}

/// Class/struct member.
#[derive(Clone, Debug)]
pub enum Member {
    /// Field.
    Field(FieldDecl),
    /// Method / function.
    Method(MethodDecl),
    /// `Default { … }`
    Default(DefaultBlock),
    /// `States { … }`
    States(StatesBlock),
    /// `Property name: fields…;`
    Property {
        /// Property name.
        name: String,
        /// Field list.
        fields: Vec<String>,
        /// Span.
        span: Span,
    },
    /// `FlagDef name: field, bit;`
    FlagDef {
        /// Flag name.
        name: String,
        /// Field.
        field: String,
        /// Bit index expr.
        bit: Expr,
        /// Span.
        span: Span,
    },
    /// `mixin Name;`
    MixinUse {
        /// Mixin name.
        name: String,
        /// Span.
        span: Span,
    },
    /// Nested `enum` inside a class/struct (GZDoom `Thinker.EStatnums`).
    NestedEnum(EnumDecl),
    /// Nested `const` inside a class/struct (GZDoom `LevelLocals.CLUSTER_HUB`).
    NestedConst(ConstDecl),
    /// Nested `struct` inside a class (GZDoom `ScriptedMarine.WeaponStates`).
    NestedStruct(StructDecl),
}

/// Field declaration.
#[derive(Clone, Debug)]
pub struct FieldDecl {
    /// Type text (as parsed).
    pub ty: TypeRef,
    /// Names (multi-decl).
    pub names: Vec<String>,
    /// Qualifiers.
    pub quals: FieldQuals,
    /// Span.
    pub span: Span,
}

/// Field qualifiers.
#[derive(Clone, Debug, Default)]
pub struct FieldQuals {
    /// `native`
    pub native: bool,
    /// `private` / `protected`
    pub access: Option<String>,
    /// `readonly`
    pub readonly: bool,
    /// `transient`
    pub transient: bool,
    /// `meta`
    pub meta: bool,
    /// `internal` (engine-only field)
    pub internal: bool,
    /// `ui` / `play` / `clearscope` / `unsafe(clearscope)` field scope.
    pub scope: Option<String>,
    /// `norollback` prediction metadata.
    pub norollback: bool,
}

/// Method declaration.
#[derive(Clone, Debug)]
pub struct MethodDecl {
    /// Name.
    pub name: String,
    /// Return type.
    pub ret: TypeRef,
    /// Further return types of a multi-return method (`int, int F()`).
    pub extra_rets: Vec<TypeRef>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Body (`None` = native / abstract semicolon).
    pub body: Option<Block>,
    /// Qualifiers.
    pub quals: MethodQuals,
    /// Span.
    pub span: Span,
}

/// Method qualifiers.
#[derive(Clone, Debug, Default)]
pub struct MethodQuals {
    /// `static`
    pub static_: bool,
    /// `virtual`
    pub virtual_: bool,
    /// `override`
    pub override_: bool,
    /// `final`
    pub final_: bool,
    /// `action`
    pub action: bool,
    /// `native`
    pub native: bool,
    /// `vararg`: takes `...` past its declared parameters.
    pub vararg: bool,
    /// Scope keyword.
    pub scope: Option<String>,
    /// `const` method.
    pub const_: bool,
    /// `unsafe(const)` restores legacy mutable-self behavior for a const method.
    pub unsafe_const: bool,
}

/// Parameter.
#[derive(Clone, Debug)]
pub struct Param {
    /// Type.
    pub ty: TypeRef,
    /// Name.
    pub name: String,
    /// `out` / `ref`
    pub mode: Option<String>,
    /// Default value.
    pub default: Option<Expr>,
}

/// Type reference (kept mostly textual for ZS0/ZS1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    /// Named type / keyword.
    Named(String),
    /// `class<T>`
    ClassOf(String),
    /// `Array<T>`
    Array(Box<TypeRef>),
    /// `Map<K,V>`
    Map(Box<TypeRef>, Box<TypeRef>),
    /// `readonly<T>`
    Readonly(Box<TypeRef>),
    /// `void`
    Void,
}

impl Default for TypeRef {
    fn default() -> Self {
        Self::Void
    }
}

/// Default block statements (properties / flags) as raw text lines for ZS0.
#[derive(Clone, Debug, Default)]
pub struct DefaultBlock {
    /// Raw property/flag lines inside the block.
    pub lines: Vec<String>,
    /// Span.
    pub span: Span,
}

/// States block (DECORATE-like).
#[derive(Clone, Debug, Default)]
pub struct StatesBlock {
    /// Raw state body text (parsed structurally later).
    pub body: String,
    /// Span.
    pub span: Span,
}

/// Statement block.
#[derive(Clone, Debug, Default)]
pub struct Block {
    /// Statements.
    pub stmts: Vec<Stmt>,
    /// Span.
    pub span: Span,
}

/// Statement.
#[derive(Clone, Debug)]
pub enum Stmt {
    /// Local `Type a, b;` / `let a = …;`
    Local {
        /// Optional type.
        ty: Option<TypeRef>,
        /// Names (`int i, j;`).
        names: Vec<String>,
        /// Init for the first name.
        init: Option<Expr>,
        /// Inits for `names[1..]`, aligned with those names.
        extra_inits: Vec<Option<Expr>>,
        /// Span.
        span: Span,
    },
    /// Expression statement.
    Expr {
        /// Expression.
        expr: Expr,
        /// Span.
        span: Span,
    },
    /// `return` [exprs…];
    Return {
        /// Values.
        values: Vec<Expr>,
        /// Span.
        span: Span,
    },
    /// `if (cond) then [else]`
    If {
        /// Condition.
        cond: Expr,
        /// Then.
        then_b: Block,
        /// Else.
        else_b: Option<Block>,
        /// Span.
        span: Span,
    },
    /// `while (cond) body`
    While {
        /// Condition.
        cond: Expr,
        /// Body.
        body: Block,
        /// Span.
        span: Span,
    },
    /// `do body while/until (cond);`
    DoWhile {
        /// Body.
        body: Block,
        /// Condition.
        cond: Expr,
        /// `until` (negate cond) vs `while`.
        until: bool,
        /// Span.
        span: Span,
    },
    /// `for (…; …; …) body` — init may be a local decl.
    For {
        /// Init statement (`Type i = 0` / expr / empty).
        init: Option<Box<Stmt>>,
        /// Cond.
        cond: Option<Expr>,
        /// Step.
        step: Option<Expr>,
        /// Body.
        body: Block,
        /// Span.
        span: Span,
    },
    /// `switch (cond) { … }`
    Switch {
        /// Discriminant.
        cond: Expr,
        /// Body (contains `case` / `default` labels).
        body: Block,
        /// Span.
        span: Span,
    },
    /// `case expr:` / `default:`
    Case {
        /// Value (`None` = default).
        value: Option<Expr>,
        /// Span.
        span: Span,
    },
    /// Nested block.
    Block(Block),
    /// `break`
    Break(Span),
    /// `continue`
    Continue(Span),
}

/// Expression.
#[derive(Clone, Debug)]
pub enum Expr {
    /// Integer.
    Int(i64, Span),
    /// Float.
    Float(f64, Span),
    /// String.
    String(String, Span),
    /// Name literal.
    Name(String, Span),
    /// `true` / `false` / `null` / `self` / `invoker`
    Ident(String, Span),
    /// Binary.
    Binary {
        /// Op text.
        op: String,
        /// LHS.
        lhs: Box<Expr>,
        /// RHS.
        rhs: Box<Expr>,
        /// Span.
        span: Span,
    },
    /// Unary.
    Unary {
        /// Op.
        op: String,
        /// Expr.
        expr: Box<Expr>,
        /// Span.
        span: Span,
    },
    /// Call.
    Call {
        /// Callee.
        callee: Box<Expr>,
        /// Args.
        args: Vec<Expr>,
        /// Span.
        span: Span,
    },
    /// Member `a.b`
    Member {
        /// Object.
        obj: Box<Expr>,
        /// Field.
        field: String,
        /// Span.
        span: Span,
    },
    /// Index `a[b]`
    Index {
        /// Array.
        arr: Box<Expr>,
        /// Index.
        index: Box<Expr>,
        /// Span.
        span: Span,
    },
    /// Cast `Type(expr)` or `(Type)expr` approximated as call-like.
    Cast {
        /// Type name.
        ty: String,
        /// Value.
        expr: Box<Expr>,
        /// Span.
        span: Span,
    },
    /// Ternary.
    Ternary {
        /// Cond.
        cond: Box<Expr>,
        /// Then.
        then_e: Box<Expr>,
        /// Else.
        else_e: Box<Expr>,
        /// Span.
        span: Span,
    },
    /// `new("Class")` / `new Class`
    New {
        /// Class name.
        class: String,
        /// Span.
        span: Span,
    },
    /// Vector / list literal `(x, y)` / `(32, 32)`.
    Tuple {
        /// Elements.
        elems: Vec<Expr>,
        /// Span.
        span: Span,
    },
}

impl Expr {
    /// Span of this expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Int(_, s)
            | Self::Float(_, s)
            | Self::String(_, s)
            | Self::Name(_, s)
            | Self::Ident(_, s)
            | Self::Binary { span: s, .. }
            | Self::Unary { span: s, .. }
            | Self::Call { span: s, .. }
            | Self::Member { span: s, .. }
            | Self::Index { span: s, .. }
            | Self::Cast { span: s, .. }
            | Self::Ternary { span: s, .. }
            | Self::New { span: s, .. }
            | Self::Tuple { span: s, .. } => *s,
        }
    }
}

/// Class name → declaration index helper for sema.
pub type ClassIndex = BTreeMap<String, usize>;

//! ClassDef IR produced by the DECORATE parser (subset for M5 bootstrap).

/// Top-level list of class definitions from one or more DECORATE lumps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassDefIr {
    /// Actor / class definitions in source order.
    pub actors: Vec<ActorDef>,
}

/// One `actor` definition.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorDef {
    /// Class name.
    pub name: String,
    /// Parent class (`Actor` if omitted).
    pub parent: Option<String>,
    /// `replaces` target.
    pub replaces: Option<String>,
    /// Editor / doom ed number.
    pub doomednum: Option<i32>,
    /// Flags set/cleared in the body.
    pub flags: Vec<FlagOp>,
    /// Property statements (name + raw arg tokens for now).
    pub properties: Vec<PropertyStmt>,
    /// Optional states block.
    pub states: Option<StatesBlock>,
}

/// `+FLAG` / `-FLAG`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagOp {
    /// True = set (`+`), false = clear (`-`).
    pub set: bool,
    /// Flag name (may be qualified `INVENTORY.AUTOACTIVATE`).
    pub name: String,
}

/// One property line.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyStmt {
    /// Property name (may be qualified).
    pub name: String,
    /// Raw argument strings (idents / numbers / quoted).
    pub args: Vec<String>,
}

/// `states { … }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatesBlock {
    /// Ordered labels and lines.
    pub items: Vec<StateItem>,
}

/// States block item.
#[derive(Clone, Debug, PartialEq)]
pub enum StateItem {
    /// `Spawn:` label.
    Label(String),
    /// Sprite frame line.
    Line(StateLine),
    /// Flow keyword.
    Flow(StateFlow),
}

/// One state line: `POSS AB 10 A_Look`.
#[derive(Clone, Debug, PartialEq)]
pub struct StateLine {
    /// Sprite name (4 chars / `####` / `----`).
    pub sprite: String,
    /// Frame letters.
    pub frames: String,
    /// Duration in tics (`-1` = forever).
    pub duration: i32,
    /// Bright etc. (raw for now).
    pub modifiers: Vec<String>,
    /// Optional action name.
    pub action: Option<String>,
    /// Action arguments (from `A_Foo(a, b)`).
    pub action_args: Vec<String>,
}

/// Flow control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateFlow {
    /// `Loop`
    Loop,
    /// `Stop`
    Stop,
    /// `Wait`
    Wait,
    /// `Fail`
    Fail,
    /// `Goto Label`
    Goto {
        /// Target label (may be `Super::Death`).
        label: String,
        /// Optional `+ offset`.
        offset: i32,
    },
}

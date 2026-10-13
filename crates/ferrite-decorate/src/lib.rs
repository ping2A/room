//! DECORATE parser producing ClassDef IR.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod error;
mod expr;
mod include;
mod ir;
mod parse;

pub use error::DecorateError;
pub use expr::{BinOp, DecoExpr, is_trivial_arg, parse_deco_expr};
pub use include::expand_includes;
pub use ir::{
    ActorDef, ClassDefIr, FlagOp, PropertyStmt, StateFlow, StateItem, StateLine, StatesBlock,
};
pub use parse::{parse_decorate, parse_decorate_with_includes};

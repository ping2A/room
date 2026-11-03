//! DECORATE parser producing ClassDef IR.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod error;
mod expr;
mod include;
mod ir;
mod parse;
mod sourcemap;

pub use error::DecorateError;
pub use expr::{BinOp, DecoExpr, is_trivial_arg, parse_deco_expr};
pub use include::{expand_includes, expand_includes_mapped};
pub use ir::{
    ActorDef, ClassDefIr, FlagOp, PropertyStmt, StateFlow, StateItem, StateLine, StatesBlock,
};
pub use parse::{
    LenientParse, parse_decorate, parse_decorate_lenient, parse_decorate_lenient_with_map,
    parse_decorate_with_includes, parse_decorate_with_map, split_actors,
};
pub use sourcemap::{ExpandedSource, SourceLine, SourceMap};

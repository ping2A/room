//! VM traps and policy.

use thiserror::Error;

/// Runtime trap (`docs/34` §4.5).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Trap {
    /// Null object dereference.
    #[error("null dereference at {func}:{line}")]
    NullDeref {
        /// Function name.
        func: String,
        /// Source line.
        line: u32,
    },
    /// Array index out of bounds.
    #[error("array out of bounds at {func}:{line}")]
    ArrayOob {
        /// Function name.
        func: String,
        /// Source line.
        line: u32,
    },
    /// Integer division by zero.
    #[error("division by zero at {func}:{line}")]
    DivZero {
        /// Function name.
        func: String,
        /// Source line.
        line: u32,
    },
    /// Instruction fuel exhausted.
    #[error("fuel exhausted in {func}")]
    FuelExhausted {
        /// Function name.
        func: String,
    },
    /// Call stack overflow.
    #[error("stack overflow in {func}")]
    StackOverflow {
        /// Function name.
        func: String,
    },
    /// Unknown / bad opcode.
    #[error("bad opcode {op} in {func}")]
    BadOpcode {
        /// Function name.
        func: String,
        /// Opcode byte.
        op: u8,
    },
    /// Native reported an error.
    #[error("native error: {0}")]
    Native(String),
    /// Explicit `Trap` opcode / assert.
    #[error("{msg} at {func}:{line}")]
    Explicit {
        /// Message.
        msg: String,
        /// Function name.
        func: String,
        /// Source line.
        line: u32,
    },
}

/// How traps are handled (`docs/12` script_trap_policy analogue).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrapPolicy {
    /// Abort the action, print, keep playing (default SP).
    #[default]
    AbortAction,
    /// Fatal like GZDoom.
    Gzdoom,
    /// Deterministic peer abort (netplay).
    NetAbort,
}

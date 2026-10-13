//! Shared error type for core helpers.

use thiserror::Error;

/// Errors that originate in `ferrite-core` (interning, RNG lookup, conversions).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// A named RNG was requested with a name whose CRC-32 is 0 (reserved).
    #[error("RNG name {0:?} hashes to CRC 0, which is reserved for nameless generators")]
    ReservedRngName(String),
    /// Interned name table overflowed `u32::MAX` entries.
    #[error("name table is full")]
    NameTableFull,
    /// Generational arena overflowed `u32::MAX` live slots.
    #[error("arena is full")]
    ArenaFull,
}

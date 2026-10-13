//! Case-insensitive interned names (GZDoom `FName`).

use crate::error::CoreError;
use std::collections::HashMap;

/// Interned, case-insensitive ASCII name. `NONE` is index 0 (empty string).
///
/// Equivalent of GZDoom's `FName`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Name(u32);

impl Name {
    /// The empty name. Equivalent of GZDoom `NAME_None`.
    pub const NONE: Self = Self(0);

    /// Raw intern index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }

    /// Reconstruct from an index previously issued by a [`NameTable`].
    #[must_use]
    pub const fn from_index(index: u32) -> Self {
        Self(index)
    }
}

/// Interning table. First-seen spelling is preserved; lookup is ASCII-case-insensitive.
#[derive(Clone, Debug)]
pub struct NameTable {
    strings: Vec<String>,
    by_folded: HashMap<String, Name>,
}

impl Default for NameTable {
    fn default() -> Self {
        Self::new()
    }
}

impl NameTable {
    /// Empty table containing only [`Name::NONE`].
    #[must_use]
    pub fn new() -> Self {
        let mut by_folded = HashMap::new();
        by_folded.insert(String::new(), Name::NONE);
        Self {
            strings: vec![String::new()],
            by_folded,
        }
    }

    /// Intern `s`, returning the existing name if the folded form is already present.
    pub fn intern(&mut self, s: &str) -> Result<Name, CoreError> {
        let folded = fold_ascii(s);
        if let Some(&name) = self.by_folded.get(&folded) {
            return Ok(name);
        }
        let index = u32::try_from(self.strings.len()).map_err(|_| CoreError::NameTableFull)?;
        if index == u32::MAX {
            return Err(CoreError::NameTableFull);
        }
        let name = Name(index);
        self.strings.push(s.to_owned());
        self.by_folded.insert(folded, name);
        Ok(name)
    }

    /// Look up without inserting.
    #[must_use]
    pub fn get(&self, s: &str) -> Option<Name> {
        self.by_folded.get(&fold_ascii(s)).copied()
    }

    /// Original spelling of the first intern of this name.
    #[must_use]
    pub fn resolve(&self, name: Name) -> &str {
        self.strings.get(name.0 as usize).map_or("", String::as_str)
    }

    /// Number of interned names including `NONE`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// True when only `NONE` is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.len() <= 1
    }
}

fn fold_ascii(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn none_is_zero() {
        assert_eq!(Name::NONE.index(), 0);
        let t = NameTable::new();
        assert_eq!(t.resolve(Name::NONE), "");
        assert_eq!(t.get(""), Some(Name::NONE));
    }

    #[test]
    fn intern_is_ascii_case_insensitive() {
        let mut t = NameTable::new();
        let a = t.intern("DoomImp").unwrap();
        let b = t.intern("doomimp").unwrap();
        let c = t.intern("DOOMIMP").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(t.resolve(a), "DoomImp");
    }

    #[test]
    fn distinct_names_differ() {
        let mut t = NameTable::new();
        let a = t.intern("IMP").unwrap();
        let b = t.intern("DEMON").unwrap();
        assert_ne!(a, b);
        assert_eq!(t.len(), 3);
    }
}

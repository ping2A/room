//! CVar store (M5 subset).

use crate::error::ConsoleError;
use std::collections::BTreeMap;

/// CVar value kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum CvarValue {
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i32),
    /// Float.
    Float(f64),
    /// String.
    String(String),
}

impl CvarValue {
    /// Parse from a console argument token.
    pub fn parse_as(ty: CvarType, s: &str) -> Result<Self, ConsoleError> {
        match ty {
            CvarType::Bool => match s.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(Self::Bool(true)),
                "0" | "false" | "no" | "off" => Ok(Self::Bool(false)),
                _ => Err(ConsoleError::Message(format!("bad bool '{s}'"))),
            },
            CvarType::Int => s
                .parse()
                .map(Self::Int)
                .map_err(|_| ConsoleError::Message(format!("bad int '{s}'"))),
            CvarType::Float => s
                .parse()
                .map(Self::Float)
                .map_err(|_| ConsoleError::Message(format!("bad float '{s}'"))),
            CvarType::String => Ok(Self::String(s.to_owned())),
        }
    }

    /// Display for `get` / echo.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => format!("{f}"),
            Self::String(s) => s.clone(),
        }
    }
}

/// Declared type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CvarType {
    /// Bool.
    Bool,
    /// Int.
    Int,
    /// Float.
    Float,
    /// String.
    String,
}

/// CVar flags (subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CvarFlags(pub u32);

impl CvarFlags {
    /// Persist to config.
    pub const ARCHIVE: Self = Self(1 << 0);
    /// Server-info (synced).
    pub const SERVERINFO: Self = Self(1 << 1);
    /// Cheat-gated.
    pub const CHEAT: Self = Self(1 << 2);
    /// Cannot be set by user.
    pub const NOSET: Self = Self(1 << 3);

    /// True if all bits in `other` are set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Bitwise or.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One registered CVar.
#[derive(Clone, Debug)]
struct CvarEntry {
    ty: CvarType,
    value: CvarValue,
    default: CvarValue,
    flags: CvarFlags,
}

/// Named CVar table (BTreeMap for deterministic iteration).
#[derive(Clone, Debug, Default)]
pub struct CvarStore {
    entries: BTreeMap<String, CvarEntry>,
}

impl CvarStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a CVar. Overwrites if present.
    pub fn register(&mut self, name: &str, ty: CvarType, default: CvarValue, flags: CvarFlags) {
        let key = name.to_ascii_lowercase();
        self.entries.insert(
            key,
            CvarEntry {
                ty,
                value: default.clone(),
                default,
                flags,
            },
        );
    }

    /// Register common engine defaults used in M5.
    pub fn register_engine_defaults(&mut self) {
        self.register(
            "sv_cheats",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::SERVERINFO.union(CvarFlags::ARCHIVE),
        );
        self.register(
            "gravity",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::SERVERINFO,
        );
        self.register(
            "cl_run",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "fullscreen",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "vid_fps",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "invertmouse",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "m_sensitivity",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "mouse_look",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "snd_menuvolume",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "language",
            CvarType::String,
            CvarValue::String("auto".into()),
            CvarFlags::ARCHIVE,
        );
    }

    /// Names with [`CvarFlags::ARCHIVE`] (for config write).
    #[must_use]
    pub fn archive_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.flags.contains(CvarFlags::ARCHIVE))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Get value by name.
    pub fn get(&self, name: &str) -> Option<&CvarValue> {
        self.entries
            .get(&name.to_ascii_lowercase())
            .map(|e| &e.value)
    }

    /// Set value; respects `NOSET`.
    pub fn set(&mut self, name: &str, raw: &str) -> Result<(), ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        if entry.flags.contains(CvarFlags::NOSET) {
            return Err(ConsoleError::Message(format!("{name} is read-only")));
        }
        entry.value = CvarValue::parse_as(entry.ty, raw)?;
        Ok(())
    }

    /// Toggle a bool CVar.
    pub fn toggle(&mut self, name: &str) -> Result<bool, ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        match &mut entry.value {
            CvarValue::Bool(b) => {
                *b = !*b;
                Ok(*b)
            }
            _ => Err(ConsoleError::Message(format!("{name} is not bool"))),
        }
    }

    /// Reset to default.
    pub fn reset(&mut self, name: &str) -> Result<(), ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        entry.value = entry.default.clone();
        Ok(())
    }

    /// Sorted names.
    pub fn list(&self) -> impl Iterator<Item = (&str, &CvarValue)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), &e.value))
    }
}

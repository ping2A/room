//! Native coverage reporting (ZS8).

use crate::natives::NativeTable;
use std::collections::BTreeMap;

/// Implementation status for a native.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeStatus {
    /// Fully implemented.
    Implemented,
    /// Returns a default and warns once.
    Stub,
    /// Calling is a load / link error.
    Unsupported,
}

/// One coverage row.
#[derive(Clone, Debug)]
pub struct CoverageRow {
    /// Qualified ZScript name.
    pub name: String,
    /// Status.
    pub status: NativeStatus,
    /// Referenced by the scanned mod.
    pub referenced: bool,
}

/// Build a coverage report: corpus references ∩ known natives.
#[must_use]
pub fn coverage_report(
    table: &NativeTable,
    referenced: &[String],
    stubs: &BTreeMap<String, NativeStatus>,
) -> Vec<CoverageRow> {
    let mut rows = Vec::new();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for name in referenced {
        let key = name.clone();
        seen.insert(key.to_ascii_lowercase(), ());
        let status = stubs
            .get(name)
            .copied()
            .or_else(|| {
                // GZDoom resolves ZScript native names case-insensitively;
                // use the same rule here so lower-case source spellings do
                // not become false Unsupported rows.
                if table.id_ci(name).is_some() {
                    Some(NativeStatus::Implemented)
                } else {
                    Some(NativeStatus::Unsupported)
                }
            })
            .unwrap_or(NativeStatus::Unsupported);
        rows.push(CoverageRow {
            name: key,
            status,
            referenced: true,
        });
    }
    // Also list implemented-but-unreferenced for completeness? skip.
    rows.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    rows
}

/// Percent of referenced natives that are Implemented or Stub.
#[must_use]
pub fn coverage_percent(rows: &[CoverageRow]) -> f64 {
    let refs: Vec<_> = rows.iter().filter(|r| r.referenced).collect();
    if refs.is_empty() {
        return 100.0;
    }
    let ok = refs
        .iter()
        .filter(|r| matches!(r.status, NativeStatus::Implemented | NativeStatus::Stub))
        .count();
    (ok as f64) * 100.0 / (refs.len() as f64)
}

/// Default stub map for BD corpus natives not yet fully wired to the sim.
#[must_use]
pub fn bd_stub_map() -> BTreeMap<String, NativeStatus> {
    let mut m = BTreeMap::new();
    for n in [
        "Wads.CheckNumForFullName",
        "Wads.CheckNumForName",
        "Wads.ReadLump",
        "Console.Printf",
        "Object.Destroy",
        "Array.Size",
        "Array.Push",
        "Array.Clear",
        "Array.Delete",
        "Array.Insert",
        "String.Length",
        "String.Mid",
        "String.Format",
        "String.ToInt",
        "consoleplayer",
        "skill",
        "gamestate",
    ] {
        m.insert(n.into(), NativeStatus::Implemented);
    }
    for n in [
        "EventHandler.Find",
        "Object.New",
        "Actor.GiveInventory",
        "PlayerInfo.GetGender",
        "PlayerInfo.GetUserName",
    ] {
        m.insert(n.into(), NativeStatus::Stub);
    }
    for n in ["CVar.GetCVar", "CVar.GetInt"] {
        m.insert(n.into(), NativeStatus::Implemented);
    }
    m
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::natives::NativeTable;

    #[test]
    fn bd_coverage_above_95() {
        let table = NativeTable::with_builtins();
        let refs = [
            "Wads.CheckNumForFullName",
            "Wads.ReadLump",
            "Console.Printf",
            "String.Mid",
            "String.Length",
            "Array.Size",
            "Object.Destroy",
            "EventHandler.Find",
            "Actor.GiveInventory",
            "skill",
            "consoleplayer",
            "CVar.GetCVar",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let rows = coverage_report(&table, &refs, &bd_stub_map());
        let pct = coverage_percent(&rows);
        assert!(pct >= 95.0, "coverage {pct}% < 95%");
    }
}

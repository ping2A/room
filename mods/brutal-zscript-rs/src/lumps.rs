//! `ZC_TMapHack` + `ZC_IntStrMap` (`zscript/BD_WCS.zc`): the open-lump
//! table behind `ZandroCompat.Lump*`.
//!
//! Brutal Doom keeps three int→string maps (names, reference counts stored
//! as decimal strings, lump contents) in an `EventHandler`, because it had to
//! run on GZDoom versions without `Map<K, V>`. The port keeps the same three
//! tables as real maps, which is what that code means to be.
//!
//! One difference, on purpose: `ZC_IntStrMap.insert` stops at the first
//! larger key, and new keys go to the front, so with three or more lumps open
//! at once an update can add a second entry for a key; `remove` then deletes
//! only one and a stale reference count survives the last `LumpClose`
//! (re-opening that lump would read nothing). Brutal Doom opens one lump at a
//! time (`WadChecker.acs`), so its behaviour is the same; the port simply
//! does not have the bug.

use std::collections::BTreeMap;

/// The open-lump tables (one per session; saved with the module memory).
#[derive(Debug, Default)]
pub struct LumpTable {
    /// `lumpNames`: lump → the name it was opened with.
    names: BTreeMap<i32, String>,
    /// `lumpRefCount`: lump → open count.
    refs: BTreeMap<i32, i32>,
    /// `lumpBuf`: lump → contents.
    bufs: BTreeMap<i32, Vec<u8>>,
}

impl LumpTable {
    /// `lumpRefCount.checkKey(lump)`.
    #[must_use]
    pub fn is_open(&self, lump: i32) -> bool {
        self.refs.contains_key(&lump)
    }

    /// `lumpRefCount.getInt(lump)` (0 when missing, like `"".ToInt()`).
    #[must_use]
    pub fn ref_count(&self, lump: i32) -> i32 {
        self.refs.get(&lump).copied().unwrap_or(0)
    }

    /// First open: remember the name and contents with a count of 1.
    pub fn insert(&mut self, lump: i32, name: &str, data: Vec<u8>) {
        self.refs.insert(lump, 1);
        self.bufs.insert(lump, data);
        self.names.insert(lump, name.to_owned());
    }

    /// `lumpRefCount.insertInt(lump, n)`.
    pub fn set_ref_count(&mut self, lump: i32, n: i32) {
        self.refs.insert(lump, n);
    }

    /// Forget a lump entirely (`remove` on all three maps).
    pub fn remove(&mut self, lump: i32) {
        self.refs.remove(&lump);
        self.names.remove(&lump);
        self.bufs.remove(&lump);
    }

    /// `lumpBuf.get(lump)` (empty when missing).
    #[must_use]
    pub fn data(&self, lump: i32) -> &[u8] {
        self.bufs.get(&lump).map_or(&[], Vec::as_slice)
    }

    /// `lumpNames.get(lump)` (empty when missing).
    #[must_use]
    pub fn name(&self, lump: i32) -> &str {
        self.names.get(&lump).map_or("", String::as_str)
    }
}

//! Generational object heap and strings.

use std::collections::BTreeMap;

/// Generational object handle (`index` + `generation`). Stale reads → null.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjHandle {
    /// Slot index.
    pub index: u32,
    /// Generation; bumped on Destroy.
    pub generation: u32,
}

impl ObjHandle {
    /// Null handle.
    pub const NULL: Self = Self {
        index: 0,
        generation: 0,
    };

    /// True if this is the null handle.
    #[must_use]
    pub fn is_null(self) -> bool {
        self.index == 0
    }
}

/// String handle into the string table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StrHandle(pub u32);

impl StrHandle {
    /// Empty string.
    pub const EMPTY: Self = Self(0);
}

#[derive(Clone, Debug)]
struct StrEntry {
    value: String,
    refs: u32,
}

/// Object kind stored in a heap slot.
#[derive(Clone, Debug)]
pub enum ObjKind {
    /// User / engine object with script fields.
    Object {
        /// Class layout id in the compiled program.
        class_id: u32,
        /// Integer script fields.
        fields_i: Vec<i64>,
        /// Object-reference script fields.
        fields_a: Vec<ObjHandle>,
        /// String script fields.
        fields_s: Vec<StrHandle>,
        /// Float script fields (`double`, vector components).
        fields_f: Vec<f64>,
    },
    /// `Array<int>` backing store.
    ArrayI(Vec<i64>),
    /// `Array<string>` backing store.
    ArrayS(Vec<StrHandle>),
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    alive: bool,
    marked: bool,
    kind: ObjKind,
}

/// Deterministic heap (no address-dependent behaviour).
#[derive(Clone, Debug)]
pub struct ZvmHeap {
    strings: Vec<StrEntry>,
    slots: Vec<Slot>,
    /// Recycled slot indices (sorted ascending for determinism).
    free: Vec<u32>,
}

impl Default for ZvmHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ZvmHeap {
    /// Fresh heap with empty string at index 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strings: vec![StrEntry {
                value: String::new(),
                refs: 1,
            }],
            // Slot 0 reserved as null sentinel (never allocated).
            slots: vec![Slot {
                generation: 0,
                alive: false,
                marked: false,
                kind: ObjKind::Object {
                    class_id: 0,
                    fields_i: Vec::new(),
                    fields_a: Vec::new(),
                    fields_s: Vec::new(),
                    fields_f: Vec::new(),
                },
            }],
            free: Vec::new(),
        }
    }

    /// Intern or retain a string; returns handle.
    pub fn intern_string(&mut self, s: &str) -> StrHandle {
        if s.is_empty() {
            return StrHandle::EMPTY;
        }
        if let Some(i) = self.strings.iter().position(|e| e.value == s) {
            self.strings[i].refs = self.strings[i].refs.saturating_add(1);
            return StrHandle(i as u32);
        }
        self.strings.push(StrEntry {
            value: s.to_owned(),
            refs: 1,
        });
        StrHandle((self.strings.len() - 1) as u32)
    }

    /// Borrow string bytes.
    #[must_use]
    pub fn str(&self, h: StrHandle) -> &str {
        self.strings
            .get(h.0 as usize)
            .map(|e| e.value.as_str())
            .unwrap_or("")
    }

    /// Concatenate two strings.
    pub fn concat(&mut self, a: StrHandle, b: StrHandle) -> StrHandle {
        let mut out = String::with_capacity(self.str(a).len() + self.str(b).len());
        out.push_str(self.str(a));
        out.push_str(self.str(b));
        self.intern_string(&out)
    }

    fn alloc_slot(&mut self, kind: ObjKind) -> ObjHandle {
        // Lowest free index first (sorted ascending) — address-independent reuse.
        if !self.free.is_empty() {
            let index = self.free.remove(0);
            let slot = &mut self.slots[index as usize];
            let generation = slot.generation;
            *slot = Slot {
                generation,
                alive: true,
                marked: false,
                kind,
            };
            return ObjHandle { index, generation };
        }
        let index = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            alive: true,
            marked: false,
            kind,
        });
        ObjHandle {
            index,
            generation: 1,
        }
    }

    /// Allocate a script object for `class_id` with field counts.
    pub fn new_object(&mut self, class_id: u32, n_i: u16, n_a: u16, n_s: u16) -> ObjHandle {
        self.new_object_f(class_id, n_i, n_a, n_s, 0)
    }

    /// Allocate a script object with a float bank too.
    pub fn new_object_f(
        &mut self,
        class_id: u32,
        n_i: u16,
        n_a: u16,
        n_s: u16,
        n_f: u16,
    ) -> ObjHandle {
        self.alloc_slot(ObjKind::Object {
            class_id,
            fields_i: vec![0; n_i as usize],
            fields_a: vec![ObjHandle::NULL; n_a as usize],
            fields_s: vec![StrHandle::EMPTY; n_s as usize],
            fields_f: vec![0.0; n_f as usize],
        })
    }

    /// Allocate an empty `Array<int>`.
    pub fn new_array_i(&mut self) -> ObjHandle {
        self.alloc_slot(ObjKind::ArrayI(Vec::new()))
    }

    /// Allocate an empty `Array<string>`.
    pub fn new_array_s(&mut self) -> ObjHandle {
        self.alloc_slot(ObjKind::ArrayS(Vec::new()))
    }

    /// Resolve a live object; `None` if destroyed/null.
    #[must_use]
    pub fn get(&self, h: ObjHandle) -> Option<&ObjKind> {
        if h.is_null() {
            return None;
        }
        let slot = self.slots.get(h.index as usize)?;
        if !slot.alive || slot.generation != h.generation {
            return None;
        }
        Some(&slot.kind)
    }

    /// Mutable resolve.
    pub fn get_mut(&mut self, h: ObjHandle) -> Option<&mut ObjKind> {
        if h.is_null() {
            return None;
        }
        let slot = self.slots.get_mut(h.index as usize)?;
        if !slot.alive || slot.generation != h.generation {
            return None;
        }
        Some(&mut slot.kind)
    }

    /// Destroy: bump generation; subsequent reads are null.
    pub fn destroy(&mut self, h: ObjHandle) {
        if h.is_null() {
            return;
        }
        let Some(slot) = self.slots.get_mut(h.index as usize) else {
            return;
        };
        if !slot.alive || slot.generation != h.generation {
            return;
        }
        slot.alive = false;
        slot.generation = slot.generation.wrapping_add(1);
        if slot.generation == 0 {
            slot.generation = 1;
        }
        // Keep free list sorted descending for pop-from-end = lowest index first? Use push + sort.
        self.free.push(h.index);
        self.free.sort_unstable();
        self.free.dedup();
    }

    /// Mark-sweep GC. `roots` are live handles. Returns objects collected.
    pub fn gc_step(&mut self, roots: &[ObjHandle], budget: usize) -> usize {
        for s in &mut self.slots {
            s.marked = false;
        }
        let mut stack: Vec<ObjHandle> = roots.iter().copied().filter(|h| !h.is_null()).collect();
        while let Some(h) = stack.pop() {
            let Some(slot) = self.slots.get_mut(h.index as usize) else {
                continue;
            };
            if !slot.alive || slot.generation != h.generation || slot.marked {
                continue;
            }
            slot.marked = true;
            match &slot.kind {
                ObjKind::Object { fields_a, .. } => {
                    for &child in fields_a {
                        if !child.is_null() {
                            stack.push(child);
                        }
                    }
                }
                ObjKind::ArrayI(_) | ObjKind::ArrayS(_) => {}
            }
        }
        let mut collected = 0usize;
        // Skip index 0.
        for i in 1..self.slots.len() {
            if collected >= budget {
                break;
            }
            let alive = self.slots[i].alive;
            let marked = self.slots[i].marked;
            if alive && !marked {
                let generation = self.slots[i].generation;
                self.destroy(ObjHandle {
                    index: i as u32,
                    generation,
                });
                collected += 1;
            }
        }
        collected
    }

    /// Debug: count live objects.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.slots.iter().skip(1).filter(|s| s.alive).count()
    }

    /// Stable FNV-ish digest of live slots + strings (ZS2 GC/heap determinism).
    ///
    /// Independent of Rust allocator addresses; same ops → same digest cross-run.
    #[must_use]
    pub fn stable_digest(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mix = |h: &mut u64, v: u64| {
            *h ^= v;
            *h = h.wrapping_mul(0x100000001b3);
        };
        for (i, s) in self.strings.iter().enumerate() {
            mix(&mut h, i as u64);
            mix(&mut h, u64::from(s.refs));
            for b in s.value.as_bytes() {
                mix(&mut h, u64::from(*b));
            }
        }
        for (i, slot) in self.slots.iter().enumerate().skip(1) {
            mix(&mut h, i as u64);
            mix(&mut h, u64::from(slot.generation));
            mix(&mut h, u64::from(slot.alive));
            if !slot.alive {
                continue;
            }
            match &slot.kind {
                ObjKind::Object {
                    class_id,
                    fields_i,
                    fields_a,
                    fields_s,
                    fields_f,
                } => {
                    mix(&mut h, 1);
                    for v in fields_f {
                        mix(&mut h, v.to_bits());
                    }
                    mix(&mut h, u64::from(*class_id));
                    for v in fields_i {
                        mix(&mut h, *v as u64);
                    }
                    for a in fields_a {
                        mix(&mut h, u64::from(a.index));
                        mix(&mut h, u64::from(a.generation));
                    }
                    for s in fields_s {
                        mix(&mut h, u64::from(s.0));
                    }
                }
                ObjKind::ArrayI(items) => {
                    mix(&mut h, 2);
                    mix(&mut h, items.len() as u64);
                    for v in items {
                        mix(&mut h, *v as u64);
                    }
                }
                ObjKind::ArrayS(items) => {
                    mix(&mut h, 3);
                    mix(&mut h, items.len() as u64);
                    for s in items {
                        mix(&mut h, u64::from(s.0));
                    }
                }
            }
        }
        for &idx in &self.free {
            mix(&mut h, u64::from(idx) ^ 0xFEED);
        }
        h
    }

    /// Snapshot of string table for determinism tests.
    #[must_use]
    pub fn string_table(&self) -> BTreeMap<u32, String> {
        self.strings
            .iter()
            .enumerate()
            .map(|(i, e)| (i as u32, e.value.clone()))
            .collect()
    }

    /// Snapshot every slot and string.
    #[must_use]
    pub fn image(&self) -> HeapImage {
        HeapImage {
            strings: self.strings.iter().map(|s| s.value.clone()).collect(),
            string_refs: self.strings.iter().map(|s| s.refs).collect(),
            free: self.free.clone(),
            slots: self
                .slots
                .iter()
                .map(|slot| HeapSlotImage {
                    generation: slot.generation,
                    alive: slot.alive,
                    kind: match &slot.kind {
                        ObjKind::Object {
                            class_id,
                            fields_i,
                            fields_a,
                            fields_s,
                            fields_f,
                        } => HeapKindImage::Object {
                            class_id: *class_id,
                            fields_i: fields_i.clone(),
                            fields_a: fields_a.iter().map(|h| (h.index, h.generation)).collect(),
                            fields_s: fields_s.iter().map(|s| s.0).collect(),
                            fields_f: fields_f.iter().map(|v| v.to_bits()).collect(),
                        },
                        ObjKind::ArrayI(items) => HeapKindImage::ArrayI(items.clone()),
                        ObjKind::ArrayS(items) => {
                            HeapKindImage::ArrayS(items.iter().map(|s| s.0).collect())
                        }
                    },
                })
                .collect(),
        }
    }

    /// Replace this heap with a snapshot. Handles keep their index and generation.
    pub fn restore(&mut self, image: &HeapImage) {
        self.strings = image
            .strings
            .iter()
            .enumerate()
            .map(|(i, value)| StrEntry {
                value: value.clone(),
                refs: image.string_refs.get(i).copied().unwrap_or(1),
            })
            .collect();
        if self.strings.is_empty() {
            self.strings.push(StrEntry {
                value: String::new(),
                refs: 1,
            });
        }
        self.free = image.free.clone();
        self.slots = image
            .slots
            .iter()
            .map(|slot| Slot {
                generation: slot.generation,
                alive: slot.alive,
                marked: false,
                kind: match &slot.kind {
                    HeapKindImage::Object {
                        class_id,
                        fields_i,
                        fields_a,
                        fields_s,
                        fields_f,
                    } => ObjKind::Object {
                        class_id: *class_id,
                        fields_i: fields_i.clone(),
                        fields_a: fields_a
                            .iter()
                            .map(|&(index, generation)| ObjHandle { index, generation })
                            .collect(),
                        fields_s: fields_s.iter().map(|&i| StrHandle(i)).collect(),
                        fields_f: fields_f.iter().map(|&b| f64::from_bits(b)).collect(),
                    },
                    HeapKindImage::ArrayI(items) => ObjKind::ArrayI(items.clone()),
                    HeapKindImage::ArrayS(items) => {
                        ObjKind::ArrayS(items.iter().map(|&i| StrHandle(i)).collect())
                    }
                },
            })
            .collect();
        if self.slots.is_empty() {
            self.slots.push(Slot {
                generation: 0,
                alive: false,
                marked: false,
                kind: ObjKind::Object {
                    class_id: 0,
                    fields_i: Vec::new(),
                    fields_a: Vec::new(),
                    fields_s: Vec::new(),
                    fields_f: Vec::new(),
                },
            });
        }
    }
}

/// Whole-heap snapshot (`docs/34` §14.5). Slot 0 is the null sentinel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeapImage {
    /// String table in index order.
    pub strings: Vec<String>,
    /// Intern counts, parallel to [`Self::strings`].
    pub string_refs: Vec<u32>,
    /// Slots in index order.
    pub slots: Vec<HeapSlotImage>,
    /// Recycled indices, ascending.
    pub free: Vec<u32>,
}

/// One heap slot in a [`HeapImage`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeapSlotImage {
    /// Generation.
    pub generation: u32,
    /// Live flag.
    pub alive: bool,
    /// Payload.
    pub kind: HeapKindImage,
}

/// Payload stored in a [`HeapSlotImage`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeapKindImage {
    /// Script object. Object fields are `(index, generation)`. Strings are table indices.
    Object {
        /// Class id.
        class_id: u32,
        /// Integer fields.
        fields_i: Vec<i64>,
        /// Object fields.
        fields_a: Vec<(u32, u32)>,
        /// String fields.
        fields_s: Vec<u32>,
        /// Float fields as IEEE bits.
        fields_f: Vec<u64>,
    },
    /// `Array<int>`.
    ArrayI(Vec<i64>),
    /// `Array<string>` as string-table indices.
    ArrayS(Vec<u32>),
}

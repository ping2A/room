//! Generational arena. Stale handles resolve to `None` (GZDoom `TObjPtr` read barrier).

use crate::error::CoreError;
use core::marker::PhantomData;

/// Handle into an [`Arena<T>`]. Copyable; equality is index+generation.
#[derive(Debug)]
pub struct Handle<T> {
    index: u32,
    generation: u32,
    _ty: PhantomData<fn() -> T>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Eq for Handle<T> {}

impl<T> core::hash::Hash for Handle<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T> Handle<T> {
    /// Reconstruct from index + generation (e.g. ACS opaque activator keys).
    ///
    /// The arena still rejects stale generations on lookup.
    #[must_use]
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            _ty: PhantomData,
        }
    }

    /// Slot index (stable across reuse of the slot).
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Generation that must match the live slot.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Dense generational store. Freed indices are reused with a bumped generation.
#[derive(Clone, Default)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Arena<T> {
    /// Empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    /// Insert `value`, returning a handle that resolves until [`Self::remove`].
    pub fn insert(&mut self, value: T) -> Result<Handle<T>, CoreError> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            self.live += 1;
            Ok(Handle {
                index,
                generation: slot.generation,
                _ty: PhantomData,
            })
        } else {
            let index = u32::try_from(self.slots.len()).map_err(|_| CoreError::ArenaFull)?;
            if index == u32::MAX {
                return Err(CoreError::ArenaFull);
            }
            self.slots.push(Slot {
                generation: 1,
                value: Some(value),
            });
            self.live += 1;
            Ok(Handle {
                index,
                generation: 1,
                _ty: PhantomData,
            })
        }
    }

    /// Borrow the value if `handle` is still live.
    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let slot = self.slots.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.as_ref()
    }

    /// Mutably borrow the value if `handle` is still live.
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Remove and return the value. Subsequent lookups of `handle` yield `None`.
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        if slot.generation == 0 {
            slot.generation = 1;
        }
        self.free.push(handle.index);
        self.live -= 1;
        Some(value)
    }

    /// Live occupancy.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// True when no live values remain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use proptest::prelude::*;

    #[test]
    fn stale_handle_is_none() {
        let mut a = Arena::new();
        let h = a.insert(1u32).unwrap();
        assert_eq!(a.get(h), Some(&1));
        assert_eq!(a.remove(h), Some(1));
        assert_eq!(a.get(h), None);
        let h2 = a.insert(2).unwrap();
        assert_eq!(h2.index(), h.index());
        assert_ne!(h2.generation(), h.generation());
        assert_eq!(a.get(h), None);
        assert_eq!(a.get(h2), Some(&2));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn insert_remove_roundtrip(values in proptest::collection::vec(0u32..10_000, 0..64)) {
            let mut arena = Arena::new();
            let mut live: Vec<(Handle<u32>, u32)> = Vec::new();
            for (i, v) in values.iter().copied().enumerate() {
                if !live.is_empty() && v % 3 == 0 {
                    let (h, expected) = live.remove(v as usize % live.len());
                    prop_assert_eq!(arena.remove(h), Some(expected));
                    prop_assert_eq!(arena.get(h), None);
                } else {
                    let h = arena.insert(v.wrapping_add(i as u32)).unwrap();
                    live.push((h, v.wrapping_add(i as u32)));
                }
            }
            for (h, expected) in &live {
                prop_assert_eq!(arena.get(*h), Some(expected));
            }
            prop_assert_eq!(arena.len(), live.len());
        }
    }
}

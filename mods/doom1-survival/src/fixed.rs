//! Tiny fixed-capacity collections (no allocator).

#![allow(dead_code)]

/// Stack vector with capacity `N`.
#[derive(Clone, Debug)]
pub struct FixedVec<T, const N: usize> {
    data: [T; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> FixedVec<T, N> {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: [T::default(); N],
            len: 0,
        }
    }

    /// Current length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Push; returns false when full.
    pub fn push(&mut self, v: T) -> bool {
        if self.len >= N {
            return false;
        }
        self.data[self.len] = v;
        self.len += 1;
        true
    }

    /// Clear without dropping (Copy).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Slice of live elements.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data[..self.len]
    }

    /// Mutable slice of live elements.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data[..self.len]
    }

    /// Swap-remove at `i`; returns the removed value.
    pub fn swap_remove(&mut self, i: usize) -> Option<T> {
        if i >= self.len {
            return None;
        }
        self.len -= 1;
        let out = self.data[i];
        if i != self.len {
            self.data[i] = self.data[self.len];
        }
        Some(out)
    }
}

impl<T: Copy + Default, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default, const N: usize> core::ops::Index<usize> for FixedVec<T, N> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.as_slice()[i]
    }
}

impl<T: Copy + Default, const N: usize> core::ops::IndexMut<usize> for FixedVec<T, N> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.as_mut_slice()[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_swap_remove() {
        let mut v = FixedVec::<i32, 4>::new();
        assert!(v.push(1));
        assert!(v.push(2));
        assert!(v.push(3));
        assert_eq!(v.swap_remove(1), Some(2));
        assert_eq!(v.as_slice(), &[1, 3]);
        assert!(v.push(4));
        assert!(v.push(5));
        assert!(!v.push(6));
    }
}

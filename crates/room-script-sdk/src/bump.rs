//! Host-callable bump allocator for `room_alloc` (`docs/12` §5.1).
//!
//! Bump-only: `room_free` is a no-op (matches SDK stub). The heap is a static
//! buffer, so it never overlaps the guest's stack or its own allocator.

use core::sync::atomic::{AtomicU32, Ordering};

/// Static guest heap for host-requested allocations (64 KiB).
const HEAP_CAP: u32 = 64 * 1024;

#[cfg(target_arch = "wasm32")]
mod heap {
    use core::cell::UnsafeCell;

    #[repr(C, align(16))]
    struct Heap(UnsafeCell<[u8; super::HEAP_CAP as usize]>);

    // SAFETY: only its address is shared; the host writes through it.
    unsafe impl Sync for Heap {}

    static HEAP: Heap = Heap(UnsafeCell::new([0; super::HEAP_CAP as usize]));

    pub fn base() -> u32 {
        HEAP.0.get() as usize as u32
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod heap {
    /// Native tests: fake addresses (never dereferenced).
    pub fn base() -> u32 {
        1024
    }
}

/// Bytes handed out so far.
static USED: AtomicU32 = AtomicU32::new(0);

/// `room_alloc(size, align) -> ptr` (0 on failure).
#[must_use]
pub fn room_alloc(size: i32, align: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let align = if align <= 0 { 1 } else { align as u32 };
    if !align.is_power_of_two() || align > 16 {
        return 0;
    }
    let base = heap::base();
    let size = size as u32;
    let mut cur = USED.load(Ordering::Relaxed);
    loop {
        let aligned = (cur.wrapping_add(align - 1)) & !(align - 1);
        let end = match aligned.checked_add(size) {
            Some(e) if e <= HEAP_CAP => e,
            _ => return 0,
        };
        match USED.compare_exchange_weak(cur, end, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (base + aligned) as i32,
            Err(actual) => cur = actual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_aligns_and_advances() {
        let a = room_alloc(8, 8);
        assert!(a >= 1024);
        assert_eq!(a % 8, 0);
        let b = room_alloc(1, 1);
        assert!(b > a);
    }

    #[test]
    fn rejects_bad_align_and_oom() {
        assert_eq!(room_alloc(4, 3), 0);
        assert_eq!(room_alloc(i32::MAX, 8), 0);
    }
}

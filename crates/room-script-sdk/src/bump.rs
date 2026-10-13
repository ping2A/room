//! Host-callable bump allocator for `room_alloc` (`docs/12` §5.1).
//!
//! Bump-only: `room_free` is a no-op (matches SDK stub). Guest Rust `alloc`
//! can later plug `talc`/`dlmalloc`; this heap is for host scratch buffers.

use core::sync::atomic::{AtomicU32, Ordering};

/// Static guest heap for host-requested allocations (64 KiB).
const HEAP_CAP: u32 = 64 * 1024;
/// First usable byte (leave low memory for optional data segments in samples).
const HEAP_BASE: u32 = 1024;

static NEXT: AtomicU32 = AtomicU32::new(HEAP_BASE);

/// `room_alloc(size, align) -> ptr` (0 on failure).
#[must_use]
pub fn room_alloc(size: i32, align: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let align = if align <= 0 { 1 } else { align as u32 };
    if !align.is_power_of_two() {
        return 0;
    }
    let size = size as u32;
    let mut cur = NEXT.load(Ordering::Relaxed);
    loop {
        let aligned = (cur.wrapping_add(align - 1)) & !(align - 1);
        let end = match aligned.checked_add(size) {
            Some(e) if e <= HEAP_BASE + HEAP_CAP => e,
            _ => return 0,
        };
        match NEXT.compare_exchange_weak(cur, end, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return aligned as i32,
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
        assert!(a >= HEAP_BASE as i32);
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

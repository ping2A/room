//! Shuriken weapon mod — generated art/sounds/classes (`docs/32`).
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
mod wasm_alloc {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicUsize, Ordering};

    const HEAP: usize = 512 * 1024;
    #[repr(C, align(16))]
    struct Buf(UnsafeCell<[u8; HEAP]>);
    // SAFETY: bump only; single-threaded WASM.
    unsafe impl Sync for Buf {}
    static HEAP_BUF: Buf = Buf(UnsafeCell::new([0; HEAP]));
    static OFF: AtomicUsize = AtomicUsize::new(0);

    struct Bump;
    // SAFETY: bump allocator for the guest heap.
    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let align = layout.align().max(8);
            let size = layout.size();
            let mut off = OFF.load(Ordering::Relaxed);
            loop {
                let aligned = (off + align - 1) & !(align - 1);
                let next = aligned.saturating_add(size);
                if next > HEAP {
                    return core::ptr::null_mut();
                }
                match OFF.compare_exchange_weak(off, next, Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => return unsafe { (HEAP_BUF.0.get() as *mut u8).add(aligned) },
                    Err(v) => off = v,
                }
            }
        }
        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    #[global_allocator]
    static A: Bump = Bump;

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
        core::arch::wasm32::unreachable()
    }
}

mod art;
mod behaviour;
mod classes;
mod fixed;
mod palette;
mod raster;
mod synth;
mod tables;

use room_script_abi::lump_ns;
use room_script_sdk::game::{event_mask, Event, RoomMod};
use room_script_sdk::reg;

use behaviour::Behaviour;

#[derive(Default)]
struct Shuriken {
    behaviour: Behaviour,
}

impl RoomMod for Shuriken {
    const NAME: &'static str = "Shuriken";
    const EVENTS: u64 = event_mask::WORLD_LOADED
        | event_mask::WORLD_TICK
        | event_mask::WORLD_THING_DIED
        | event_mask::WORLD_THING_DESTROYED;

    fn register(&mut self) {
        let pal = load_palette();
        if let Ok(sprites) = art::generate_all(&pal) {
            for s in &sprites {
                let _ = reg::lump(&s.name, lump_ns::SPRITES, &s.data);
            }
        }
        synth::register();
        classes::register();
    }

    fn init(&mut self, session_kind: i32) {
        self.behaviour.init(session_kind);
    }

    fn on_event(&mut self, event: Event) {
        self.behaviour.on_event(event);
    }

    fn on_state_action(&mut self, payload: room_script_abi::ActionPayload) -> i64 {
        self.behaviour.on_state_action(payload)
    }
}

fn load_palette() -> palette::Palette {
    let mut buf = alloc::vec![0u8; 768];
    let idx = reg::lump_find("PLAYPAL", lump_ns::GLOBAL);
    if idx >= 0 {
        let n = reg::lump_read(idx, &mut buf);
        if n >= 768 {
            return palette::Palette::from_playpal(&buf[..768]);
        }
    }
    palette::Palette::fallback()
}

room_script_sdk::export_mod!(Shuriken);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(bytes: &[u8]) -> alloc::string::String {
        bytes.iter().map(|b| alloc::format!("{b:02x}")).collect()
    }

    /// Pinned SHA-256 fingerprint of every SK0 sprite (fallback palette).
    #[test]
    fn sprite_hashes_pinned() {
        let pal = palette::Palette::fallback();
        let sprites = art::generate_all(&pal).unwrap();
        assert_eq!(sprites.len(), tables::SPRITE_LUMPS.len());
        let mut digest_lines = alloc::vec::Vec::new();
        for s in &sprites {
            let h = hex(&Sha256::digest(&s.data));
            digest_lines.push(alloc::format!("{} {h}", s.name));
            assert!(s.data.len() > 16, "{}", s.name);
        }
        let joined = digest_lines.join("\n");
        let fingerprint = hex(&Sha256::digest(joined.as_bytes()));
        const EXPECTED: &[(&str, &str)] = &[
            (
                "SHKMA0",
                "7ef16b2c56022730c6829d030f92452e7aafbc1ae246de97f32f52fe429866a9",
            ),
            (
                "SHKMB0",
                "e71fa9341f46ff17be0cc89fea2f276de5bb0c941c64e7d22c3015a47a8d00cc",
            ),
            (
                "SHKMC0",
                "38a8a90f51a659508e8a8b90d0e1d6f545e4478e9f8f7bf1650934f2a13a09a7",
            ),
            (
                "SHKMD0",
                "04f2eee85f7a6bfd49e2d26aa3144dbd7bb5bc087c4573c8cac77ff55b99fecd",
            ),
            (
                "SHKSA0",
                "39420ecd56bb4d85d8460509a6e73f89b2c445d351ca7a43514f0c6d4e61a054",
            ),
            (
                "SHKPA0",
                "023f5eb441e0aff63dd5ea2df43dbd320081b687fb8d6831e490555d943f666c",
            ),
            (
                "SHKBA0",
                "2b8bf8afe24a517d0ccdee5387aa9cf0647b97d1bea2272148b6d99b228c79f3",
            ),
            (
                "SHKWA0",
                "2afcf0c3e9f2d2873bda144204b58d9f0377e026d86528ea137891afc20b35ea",
            ),
            (
                "SHKGA0",
                "5511c3409eb39d5ccff6d70dfd48a675538e3b17a3ec87169ddb6b9dd5c0eeba",
            ),
            (
                "SHKGB0",
                "1f3af95012797c17e510b2583d67c4fcf494b22bfc9a51cbbe383d6c61166c28",
            ),
            (
                "SHKGC0",
                "fcc44b9c991df64598bc04b4eb8741003ffcb6af2b64b31cde13152011e3ea1d",
            ),
            (
                "SHKGD0",
                "1d2fab4ba9fc9a70ce1d98b62f359bbd08f4f8b18f637d2d9ed6701454f888ad",
            ),
            (
                "SHKGE0",
                "b4b3553f75f1bc14aff1721916885e512d98c64452fe4f4c2c283e827c90b568",
            ),
            (
                "SHKIA0",
                "d2361151efb276d560ab4324226b8ac7792ed0329a063209bdbb47953c873ec6",
            ),
        ];
        for (s, (name, exp)) in sprites.iter().zip(EXPECTED.iter()) {
            assert_eq!(s.name, *name);
            let h = hex(&Sha256::digest(&s.data));
            assert_eq!(h, *exp, "{name}");
        }
        assert_eq!(
            fingerprint,
            "820595e9c5e476f77d2597a502adfa1d8699ee86e58c1387aad06007d8fc9eaa",
            "update pin:\n{joined}\nfingerprint={fingerprint}"
        );
    }

    /// Pinned SHA-256 of every SK1 sound lump.
    #[test]
    fn sound_hashes_pinned() {
        let sounds = synth::generate_all().unwrap();
        assert_eq!(sounds.len(), tables::SOUND_ALIASES.len());
        let mut lines = alloc::vec::Vec::new();
        for s in &sounds {
            let h = hex(&Sha256::digest(&s.data));
            lines.push(alloc::format!("{} {h}", s.name));
            assert!(s.data.len() > 40, "{}", s.name);
        }
        const EXPECTED: &[(&str, &str)] = &[
            (
                "DSSHKTHR",
                "9c97d213daf102c2c8a422aa190896244cdcc995e759f00fbb1c2df44d32a274",
            ),
            (
                "DSSHKCLK",
                "ec8d7bc756a479e929044b403d05481b9bd6928e48ca1e0f223699b6cbea43f3",
            ),
            (
                "DSSHKFLS",
                "d08347e87b6712d14142e107cee14e2c1ae074ac85be04604c27138cfaa18523",
            ),
            (
                "DSSHKPKP",
                "dadfa9579091fdcdd81f2d5c70f78a3fd8b656b9705283fc7ccc91a84b31b870",
            ),
            (
                "DSSHKBOX",
                "28cd11fd8f57d56ddf51d7df68341050037e50b635732cd3e9f57ceb7fee3617",
            ),
            (
                "DSSHKUP",
                "d2f245231644e0d1c5b09bf23035d9444fb7088620db8e5fafbf09b34414fae7",
            ),
        ];
        for (s, (name, exp)) in sounds.iter().zip(EXPECTED.iter()) {
            assert_eq!(s.name, *name);
            let h = hex(&Sha256::digest(&s.data));
            assert_eq!(h, *exp, "{name}");
        }
        let _ = lines;
    }
}

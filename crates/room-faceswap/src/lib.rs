//! Face alignment and sprite-head compositing (`docs/26-face-swap.md`).
//!
//! Pure Rust: no I/O, GPU, or network. Deterministic compositing via `libm`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod align;
pub mod anchor;
pub mod apply_live;
pub mod assign;
pub mod bake;
pub mod color;
pub mod composite;
pub mod detect;
pub mod duke;
pub mod estimate;
pub mod image;
pub mod live;
pub mod parse;
pub mod profile;
pub mod share;

pub use align::{AlignError, ManualPoints, align_manual};
pub use anchor::{HeadAnchor, HeadYaw, Visibility, lump_pixel_hash};
pub use apply_live::{install_bake, override_rgba, sprite_to_rgba};
pub use assign::{
    AssignmentRule, FaceAssignment, FaceCatalog, PickMode, face_for_actor, instance_hash,
    resolve_face_key,
};
pub use bake::{BakeClassResult, BakeError, BakedLump, bake_class, bake_index, resolve_anchor};
pub use composite::{CompositeError, CompositeOptions, PalettedSprite, composite_front};
pub use detect::{FaceDetection, Pt, detect};
pub use duke::{duke_composite_options, duke_skip_indices};
pub use estimate::{ClassHint, estimate_anchor, mirror_anchor, yaw_from_lump};
pub use image::{AlphaMask, RgbaImage};
pub use live::{
    LIVE_LUMPS_PER_FRAME, LiveCacheKey, LiveFaceOverrides, rebuild_class_overrides,
    revise_live_lumps,
};
pub use parse::{AnchorEntry, ClassOverride, HeadAnchorsFile, ParseError, parse_headanchors};
pub use profile::{
    CANON_CHIN_Y, CANON_LEFT_EYE, CANON_RIGHT_EYE, Consent, FaceId, FaceProfile, FaceView,
    ProfileError, Rgb,
};
pub use share::{
    MAX_FACE_BYTES, MAX_FACES_PER_PEER, RoomSharePolicy, SharePrefs, SharedFaceBlob,
    accept_peer_face, decode_share_blob, make_share_blob,
};

//! Multiplayer face-sharing policy (`docs/26` §8). No network I/O here.

use crate::image::RgbaImage;
use crate::profile::{FaceId, FaceView};

/// Caps for peer face payloads (decoded RGBA). Wire uses 64×64 to fit
/// `MAX_CONTROL_BYTES` after base64; local gallery keeps 128×128.
pub const MAX_FACE_BYTES: usize = 64 * 64 * 4;
/// Share raster edge length on the wire.
pub const SHARE_EDGE: u32 = 64;
/// Max faces shared per peer per session.
pub const MAX_FACES_PER_PEER: usize = 4;

/// Whether a room allows face sharing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoomSharePolicy {
    /// Disabled (default for public lobbies).
    #[default]
    Disabled,
    /// Prompt the receiver (private rooms).
    Ask,
    /// Auto-accept (not recommended).
    Allow,
}

/// Outbound share payload: front view only (never sources), wire size [`SHARE_EDGE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFaceBlob {
    /// Sender's face id.
    pub id: FaceId,
    /// User label.
    pub name: String,
    /// Raw RGBA [`SHARE_EDGE`]² (uncompressed; host may zlib).
    pub rgba: Vec<u8>,
}

/// Build a share blob from a front view (downscaled to [`SHARE_EDGE`]).
#[must_use]
pub fn make_share_blob(id: FaceId, name: &str, front: &FaceView) -> Option<SharedFaceBlob> {
    if front.rgba.width < 8 || front.rgba.height < 8 {
        return None;
    }
    let rgba = downsample_rgba(&front.rgba, SHARE_EDGE, SHARE_EDGE);
    if rgba.len() > MAX_FACE_BYTES {
        return None;
    }
    Some(SharedFaceBlob {
        id,
        name: name.to_string(),
        rgba,
    })
}

/// Decode a received blob into an [`RgbaImage`] (session memory only).
#[must_use]
pub fn decode_share_blob(blob: &SharedFaceBlob) -> Option<RgbaImage> {
    if blob.rgba.len() != (SHARE_EDGE * SHARE_EDGE * 4) as usize {
        return None;
    }
    Some(RgbaImage {
        width: SHARE_EDGE,
        height: SHARE_EDGE,
        pixels: blob.rgba.clone(),
    })
}

fn downsample_rgba(src: &RgbaImage, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let sx = ((x as f32 + 0.5) * src.width as f32 / dw as f32) as u32;
            let sy = ((y as f32 + 0.5) * src.height as f32 / dh as f32) as u32;
            let sx = sx.min(src.width.saturating_sub(1));
            let sy = sy.min(src.height.saturating_sub(1));
            let i = ((sy * src.width + sx) * 4) as usize;
            let o = ((y * dw + x) * 4) as usize;
            out[o..o + 4].copy_from_slice(&src.pixels[i..i + 4]);
        }
    }
    out
}

/// Local peer preferences.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SharePrefs {
    /// Offer our faces to the room.
    pub share_my_faces: bool,
    /// Hide all faces received from others.
    pub hide_shared: bool,
    /// Blocked peer ids (host-defined strings).
    pub blocked_peers: Vec<String>,
}

/// Should we display a face from `peer`?
#[must_use]
pub fn accept_peer_face(
    policy: RoomSharePolicy,
    prefs: &SharePrefs,
    peer_id: &str,
    user_accepted: bool,
) -> bool {
    if prefs.hide_shared || prefs.blocked_peers.iter().any(|p| p == peer_id) {
        return false;
    }
    match policy {
        RoomSharePolicy::Disabled => false,
        RoomSharePolicy::Allow => true,
        RoomSharePolicy::Ask => user_accepted,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::image::AlphaMask;
    use crate::profile::FaceId;

    #[test]
    fn public_lobby_default_blocks() {
        let prefs = SharePrefs::default();
        assert!(!accept_peer_face(
            RoomSharePolicy::Disabled,
            &prefs,
            "peer1",
            true
        ));
    }

    #[test]
    fn share_blob_roundtrip_size() {
        let rgba = RgbaImage::new(128, 128);
        let mask = AlphaMask::zeros(128, 128);
        let view = crate::profile::FaceView::new(rgba, mask).unwrap();
        let blob = make_share_blob(FaceId(1), "me", &view).unwrap();
        assert_eq!(blob.rgba.len(), (SHARE_EDGE * SHARE_EDGE * 4) as usize);
        assert!(decode_share_blob(&blob).is_some());
    }
}

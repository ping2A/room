//! Render-side face texture override keys (`docs/26` §7.2).
//!
//! Compositing lives in `room-faceswap`. This module only names the lookup the
//! sprite drawer uses so Live mode never touches the playsim. Hosts upload
//! via [`crate::TextureManager::upsert_rgba`] under [`FaceTextureKey::cache_name`].

use crate::texture::TextureManager;
use room_core::TextureId;

/// Key for a face-composited sprite texture.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceTextureKey {
    /// Source lump / tile name.
    pub lump: String,
    /// Face profile id bits.
    pub face_id: u128,
}

impl FaceTextureKey {
    /// Build a stable GPU/cache name.
    #[must_use]
    pub fn cache_name(&self) -> String {
        format!("face:{}:{:032x}", self.lump, self.face_id)
    }
}

/// Upload one face override into the texture manager (render-only).
pub fn upsert_face_rgba(
    textures: &mut TextureManager,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    key: &FaceTextureKey,
    prev: Option<TextureId>,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> TextureId {
    let _ = key.cache_name();
    textures.upsert_rgba(device, queue, prev, width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_name_stable() {
        let k = FaceTextureKey {
            lump: "TROOA1".into(),
            face_id: 0xFACE,
        };
        assert_eq!(k.cache_name(), "face:TROOA1:0000000000000000000000000000face");
    }
}

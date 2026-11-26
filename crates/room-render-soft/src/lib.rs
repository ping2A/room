//! Classic CPU software renderer for Doom-engine maps (`docs/40`).
//!
//! A port of Doom's own renderer (Chocolate Doom's `r_*.c`, GPL-2.0-or-later)
//! that draws into an 8-bit frame through `COLORMAP`, the way vanilla does.
//! It is separate from the GPU renderer (`room-render`) and only reads
//! `room-render`'s public picture decoding (`docs/40` §6).
//!
//! Status: SR0 / SR1 — walls, floors, ceilings and sky. Things, masked
//! middle textures and the weapon come in SR2.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod data;
mod finetangent;
pub mod fixed;
pub mod level;
pub mod preview;
pub mod render;
pub mod shot;
#[cfg(feature = "sim")]
pub mod sim;

pub use data::{
    SoftAnim, SoftData, SoftPatch, SoftPost, SoftSpriteFrame, SoftTexture, SoftVoxel, boom_tranmap,
};
pub use level::{
    FlatXform, PART_BOTTOM, PART_MID, PART_TOP, PlanePortal, SKY_FLAT, SidePart, SkyTransfer,
    SlabPlanes, SoftFFloor, SoftLevel, SoftLinePortal, SoftSector, SoftSectorPortal, SoftSlope,
};
pub use preview::{doom_spawn_sprite, map_things};
pub use render::{
    FF_FRAMEMASK, FF_FULLBRIGHT, MAX_PORTAL_DEPTH, MF_SHADOW, MF_TRANSLATION, MF_TRANSLUCENT,
    MF_TRANSSHIFT, SoftDecal, SoftFrame, SoftLight, SoftPSprite, SoftParticle, SoftRenderer,
    SoftSpriteKind, SoftStyle, SoftTarget, SoftThing, SoftVoxelPose, sprite_name,
};

/// Everything needed to draw one map: pictures, the walkable level and the
/// renderer tables for one view size.
#[derive(Debug)]
pub struct SoftView {
    /// Pictures.
    pub data: SoftData,
    /// The map.
    pub level: SoftLevel,
    /// Renderer for the view size.
    pub renderer: SoftRenderer,
}

impl SoftView {
    /// Prepare `map` (which needs game nodes) for a `width` × `height` view.
    pub fn new(
        vfs: &room_vfs::Vfs,
        map: &room_map::MapGeometry,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let mut data = SoftData::load(vfs);
        let level = SoftLevel::new(map, &mut data, vfs)?;
        Some(Self {
            data,
            level,
            renderer: SoftRenderer::new(width, height),
        })
    }

    /// Sky texture id by name (`SKY1`, …).
    pub fn sky(&mut self, vfs: &room_vfs::Vfs, name: &str) -> u16 {
        self.data.texture(vfs, name)
    }

    /// Load the sprites `frame` uses (first use only); call before
    /// [`Self::render`].
    pub fn prepare(&mut self, vfs: &room_vfs::Vfs, frame: &SoftFrame) {
        for name in frame.sprite_names() {
            if self.data.sprite_frames(name).is_none() {
                let _ = self.data.sprite(vfs, name);
            }
        }
        for d in &frame.decals {
            if self.data.picture_id(&d.picture).is_none() {
                let _ = self.data.picture(vfs, &d.picture);
            }
        }
    }

    /// Draw a frame.
    pub fn render(&mut self, frame: &SoftFrame) -> SoftTarget {
        let mut target = SoftTarget::new(self.renderer.width(), self.renderer.height());
        self.renderer
            .render(&self.level, &self.data, frame, &mut target);
        target
    }
}

/// The shells' particles as [`SoftParticle`]s (SR8; textured particles
/// are left out).
#[must_use]
pub fn soft_particles(store: &room_render::ParticleStore) -> Vec<SoftParticle> {
    store
        .particles
        .iter()
        .filter(|p| p.texture.is_none() && p.alpha > 0.0)
        .map(|p| {
            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
            SoftParticle {
                x: f64::from(p.pos.x),
                y: f64::from(p.pos.y),
                z: f64::from(p.pos.z),
                size: f64::from(p.size),
                color: (c(p.color[0]) << 16) | (c(p.color[1]) << 8) | c(p.color[2]),
                alpha: (p.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                additive: matches!(p.style, room_render::ParticleStyle::Additive),
                light: (p.light.clamp(0.0, 1.0) * 255.0).round() as i32,
            }
        })
        .collect()
}

/// The shells' dynamic lights as [`SoftLight`]s (SR8; spot lights shine
/// all round).
#[must_use]
pub fn soft_lights(lights: &[room_render::DynLightInstance]) -> Vec<SoftLight> {
    lights
        .iter()
        .map(|l| SoftLight {
            x: f64::from(l.pos.x),
            y: f64::from(l.pos.y),
            z: f64::from(l.pos.z),
            radius: f64::from(l.radius),
            color: l.color,
            subtractive: l.subtractive,
        })
        .collect()
}

//! Renderer-owned particle store (M7 — ACS `SpawnParticle` billboards).

use crate::camera::Camera;
use crate::scene::{MaterialRange, SceneLists};
use crate::sprites::{SpriteInstance, emit_sprite_quad};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Synthetic material for translucent particle quads.
pub const PARTICLE_TEX: &str = "__PARTICLE";
/// Additive particle quads (shell binds Add blend).
pub const PARTICLE_TEX_ADD: &str = "__PARTICLE_ADD";

/// Particle blend / draw style (GZDoom `SPF_*` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParticleStyle {
    /// Alpha blend (default).
    #[default]
    Translucent,
    /// Additive (fullbright sparks / muzzle).
    Additive,
    /// Soft / faded translucent (same blend, lower alpha curve).
    Soft,
}

/// One live particle.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleInstance {
    /// World position.
    pub pos: Vec3,
    /// Velocity map-units per tic (`SPF_RELATIVE*` bootstrap).
    pub vel: Vec3,
    /// Acceleration map-units per tic².
    pub accel: Vec3,
    /// Width/height in map units.
    pub size: f32,
    /// Light 0–1 (from colour luminance / fullbright).
    pub light: f32,
    /// Opacity 0..=1 (fades over lifetime).
    pub alpha: f32,
    /// Remaining tics.
    pub life: u32,
    /// Initial lifetime (for fade).
    pub life_max: u32,
    /// Draw style.
    pub style: ParticleStyle,
}

/// Cap on retained particles.
pub const MAX_PARTICLES: usize = 2048;

/// Persistent particle list owned by the renderer.
#[derive(Clone, Debug, Default)]
pub struct ParticleStore {
    /// Active particles.
    pub particles: Vec<ParticleInstance>,
}

impl ParticleStore {
    /// Ingest ACS events `(color, fullbright, lifetime, size, x, y, z)`.
    pub fn ingest(
        &mut self,
        events: impl IntoIterator<Item = (u32, bool, i32, f64, f64, f64, f64)>,
    ) {
        for (color, fullbright, lifetime, size, x, y, z) in events {
            if lifetime <= 0 {
                continue;
            }
            #[allow(clippy::cast_sign_loss)]
            let life = lifetime.clamp(1, 255) as u32;
            let r = ((color >> 16) & 0xff) as f32 / 255.0;
            let g = ((color >> 8) & 0xff) as f32 / 255.0;
            let b = (color & 0xff) as f32 / 255.0;
            let lum = if fullbright {
                1.0
            } else {
                (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.05, 1.0)
            };
            #[allow(clippy::cast_possible_truncation)]
            let sz = (size.abs() as f32).max(0.5);
            if self.particles.len() >= MAX_PARTICLES {
                self.particles.remove(0);
            }
            #[allow(clippy::cast_possible_truncation)]
            self.particles.push(ParticleInstance {
                pos: Vec3::new(x as f32, y as f32, z as f32),
                vel: Vec3::ZERO,
                accel: Vec3::ZERO,
                size: sz,
                light: lum,
                alpha: 1.0,
                life,
                life_max: life,
                style: if fullbright {
                    ParticleStyle::Additive
                } else {
                    ParticleStyle::Translucent
                },
            });
        }
    }

    /// Spawn with explicit velocity / acceleration (ACS `SpawnParticle` extras).
    pub fn spawn_ex(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        accel: Vec3,
        size: f32,
        light: f32,
        life: u32,
    ) {
        self.spawn_styled(
            pos,
            vel,
            accel,
            size,
            light,
            life,
            ParticleStyle::Translucent,
        );
    }

    /// Spawn with an explicit [`ParticleStyle`].
    pub fn spawn_styled(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        accel: Vec3,
        size: f32,
        light: f32,
        life: u32,
        style: ParticleStyle,
    ) {
        if life == 0 {
            return;
        }
        if self.particles.len() >= MAX_PARTICLES {
            self.particles.remove(0);
        }
        self.particles.push(ParticleInstance {
            pos,
            vel,
            accel,
            size: size.max(0.5),
            light: light.clamp(0.0, 1.0),
            alpha: 1.0,
            life,
            life_max: life,
            style,
        });
    }

    /// Advance one tic; remove expired.
    pub fn tick(&mut self) {
        self.particles.retain_mut(|p| {
            p.life = p.life.saturating_sub(1);
            if p.life == 0 {
                return false;
            }
            p.vel += p.accel;
            p.pos += p.vel;
            #[allow(clippy::cast_precision_loss)]
            let frac = p.life as f32 / p.life_max.max(1) as f32;
            p.alpha = match p.style {
                ParticleStyle::Soft => (frac * frac).clamp(0.0, 1.0),
                ParticleStyle::Additive | ParticleStyle::Translucent => frac.clamp(0.0, 1.0),
            };
            true
        });
    }

    /// Clear (map change).
    pub fn clear(&mut self) {
        self.particles.clear();
    }
}

/// GPU instance record for instanced particle quads (upload bootstrap).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ParticleGpuInstance {
    /// World XYZ + size (map units).
    pub pos_size: [f32; 4],
    /// RGB light + alpha.
    pub color_alpha: [f32; 4],
    /// Style: 0 = translucent, 1 = additive, 2 = soft.
    pub style: u32,
    /// Padding to 16-byte align.
    pub _pad: [u32; 3],
}

/// Pack [`ParticleStore`] into GPU instance buffer contents (instancing path).
#[must_use]
pub fn pack_particle_instances(store: &ParticleStore) -> Vec<ParticleGpuInstance> {
    store
        .particles
        .iter()
        .filter(|p| p.alpha > 0.001)
        .map(|p| {
            let style = match p.style {
                ParticleStyle::Translucent => 0u32,
                ParticleStyle::Additive => 1,
                ParticleStyle::Soft => 2,
            };
            let light = if p.style == ParticleStyle::Additive {
                1.0
            } else {
                p.light
            };
            ParticleGpuInstance {
                pos_size: [p.pos.x, p.pos.y, p.pos.z, p.size],
                color_alpha: [light, light, light, p.alpha],
                style,
                _pad: [0; 3],
            }
        })
        .collect()
}

/// Append particle billboards into the sprite lists.
pub fn append_particles(lists: &mut SceneLists, store: &ParticleStore, camera: &Camera) {
    for p in &store.particles {
        if p.alpha <= 0.001 {
            continue;
        }
        let tex = match p.style {
            ParticleStyle::Additive => PARTICLE_TEX_ADD,
            ParticleStyle::Translucent | ParticleStyle::Soft => PARTICLE_TEX,
        };
        let light = if p.style == ParticleStyle::Additive {
            1.0
        } else {
            p.light
        };
        #[allow(clippy::cast_possible_truncation)]
        let start = lists.sprites.len() as u32;
        let quad = emit_sprite_quad(
            SpriteInstance {
                pos: p.pos,
                width: p.size,
                height: p.size,
                light,
                alpha: p.alpha,
                fuzz: false,
                texture: tex.into(),
                flip_u: false,
                anchor_u: 0.5,
            },
            camera.forward,
        );
        lists.sprites.extend_from_slice(&quad);
        lists.sprite_ranges.push(MaterialRange {
            name: tex.into(),
            start,
            count: 6,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_and_fade() {
        let mut store = ParticleStore::default();
        store.ingest([(0xff_00_00, true, 4, 8.0, 0.0, 0.0, 32.0)]);
        assert_eq!(store.particles.len(), 1);
        assert!((store.particles[0].light - 1.0).abs() < 1e-4);
        store.tick();
        assert_eq!(store.particles[0].life, 3);
        assert!((store.particles[0].alpha - 0.75).abs() < 1e-3);
        store.tick();
        store.tick();
        store.tick();
        assert!(store.particles.is_empty());
    }

    #[test]
    fn spawn_ex_integrates_velocity() {
        let mut store = ParticleStore::default();
        store.spawn_ex(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -0.5),
            4.0,
            1.0,
            10,
        );
        store.tick();
        assert!((store.particles[0].pos.x - 1.0).abs() < 1e-4);
        assert!((store.particles[0].vel.z + 0.5).abs() < 1e-4);
    }

    #[test]
    fn fullbright_ingest_is_additive() {
        let mut store = ParticleStore::default();
        store.ingest([(0xff_ff_ff, true, 8, 4.0, 0.0, 0.0, 0.0)]);
        assert_eq!(store.particles[0].style, ParticleStyle::Additive);
    }

    #[test]
    fn soft_style_fades_quadratically() {
        let mut store = ParticleStore::default();
        store.spawn_styled(
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            4.0,
            1.0,
            4,
            ParticleStyle::Soft,
        );
        store.tick(); // life 3/4 → alpha 0.5625
        assert!((store.particles[0].alpha - 0.5625).abs() < 1e-3);
    }

    #[test]
    fn pack_gpu_instances() {
        let mut store = ParticleStore::default();
        store.spawn_styled(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::ZERO,
            Vec3::ZERO,
            8.0,
            0.5,
            10,
            ParticleStyle::Soft,
        );
        store.ingest([(0xff_00_00, true, 4, 4.0, 0.0, 0.0, 0.0)]);
        let packed = pack_particle_instances(&store);
        assert_eq!(packed.len(), 2);
        assert_eq!(packed[0].style, 2);
        assert!((packed[0].pos_size[0] - 1.0).abs() < 1e-4);
        assert_eq!(packed[1].style, 1);
    }
}

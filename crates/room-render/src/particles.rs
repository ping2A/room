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

const SPF_NEGATIVE_FADESTEP: u32 = 1 << 10;
const SPF_REPLACE: u32 = 1 << 7;
const SPF_NOTIMEFREEZE: u32 = 1 << 5;
const SPF_NODYNAMICLIGHTING: u32 = 1 << 18;
const SPF_FADE_IN_OUT: u32 = 1 << 16;
const SPF_FADE_IN_HOLD_OUT: u32 = 1 << 17;

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
    /// RGB tint (1,1,1 = white; blood / sparks set this).
    pub color: [f32; 3],
    /// Opacity 0..=1 (fades over lifetime).
    pub alpha: f32,
    /// Remaining tics.
    pub life: u32,
    /// Initial lifetime (for fade).
    pub life_max: u32,
    /// Per-tic opacity delta.
    pub fade_step: f32,
    /// UZDoom particle flags used by fade state transitions.
    pub flags: u32,
    /// Opacity step after a fade-in phase.
    pub fade_out_step: f32,
    /// Per-tic size delta.
    pub size_step: f32,
    /// Roll angle in degrees.
    pub roll: f32,
    /// Per-tic roll velocity in degrees.
    pub roll_velocity: f32,
    /// Per-tic roll acceleration in degrees.
    pub roll_acceleration: f32,
    /// Draw style.
    pub style: ParticleStyle,
    /// Optional floor Z (map units). When set, particles clamp and settle on contact.
    pub floor_z: Option<f32>,
    /// Draw as a horizontal floor disc (`Flat`) instead of a vertical billboard.
    pub on_floor: bool,
}

/// Playsim particle request transferred to the render-owned particle store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSpawn {
    /// Packed RGB (`0xRRGGBB`).
    pub color: u32,
    /// Fullbright rendering.
    pub fullbright: bool,
    /// Optional UZDoom legacy render style (`STYLE_Add` = 7, `STYLE_Translucent` = 6).
    pub render_style: Option<i32>,
    /// Lifetime in tics.
    pub lifetime: i32,
    /// Initial size in map units.
    pub size: f64,
    /// Initial position.
    pub pos: [f64; 3],
    /// Velocity in map units per tic.
    pub vel: [f64; 3],
    /// Acceleration in map units per tic².
    pub accel: [f64; 3],
    /// Initial opacity.
    pub alpha: f64,
    /// Per-tic opacity delta.
    pub fade_step: f64,
    /// UZDoom particle flags (`SPF_*`).
    pub flags: u32,
    /// Opacity step after a fade-in phase.
    pub fade_out_step: f64,
    /// Per-tic size delta.
    pub size_step: f64,
    /// Initial roll, velocity, and acceleration in degrees.
    pub roll: [f64; 3],
}

/// Soft-circle RGBA texture name for a quantized tint (`__PARTICLE_T_rrggbb`).
#[must_use]
pub fn particle_tint_tex_name(rgb: [u8; 3]) -> String {
    format!("__PARTICLE_T_{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// 16×16 soft disc RGBA8 for particle billboards.
#[must_use]
pub fn soft_disc_rgba(rgb: [u8; 3]) -> Vec<u8> {
    const N: usize = 16;
    let mut out = vec![0u8; N * N * 4];
    let cx = (N as f32 - 1.0) * 0.5;
    for y in 0..N {
        for x in 0..N {
            let dx = x as f32 - cx;
            let dy = y as f32 - cx;
            let d = (dx * dx + dy * dy).sqrt() / (cx + 0.5);
            let a = ((1.0 - d).clamp(0.0, 1.0).powi(2) * 255.0) as u8;
            let i = (y * N + x) * 4;
            out[i] = rgb[0];
            out[i + 1] = rgb[1];
            out[i + 2] = rgb[2];
            out[i + 3] = a;
        }
    }
    out
}

fn quantize_rgb(c: [f32; 3]) -> [u8; 3] {
    [
        ((c[0].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
        ((c[1].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
        ((c[2].clamp(0.0, 1.0) * 31.0).round() as u8).saturating_mul(8),
    ]
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
                color: [r, g, b],
                alpha: 1.0,
                life,
                life_max: life,
                fade_step: 1.0 / life as f32,
                flags: 0,
                fade_out_step: 0.0,
                size_step: 0.0,
                roll: 0.0,
                roll_velocity: 0.0,
                roll_acceleration: 0.0,
                style: if fullbright {
                    ParticleStyle::Additive
                } else {
                    ParticleStyle::Translucent
                },
                floor_z: None,
                on_floor: false,
            });
        }
    }

    /// Ingest full playsim particle requests while retaining their motion and fade values.
    pub fn ingest_ex(&mut self, events: impl IntoIterator<Item = ParticleSpawn>) {
        for event in events {
            if event.lifetime <= 0 {
                continue;
            }
            let life = event.lifetime.clamp(1, 255) as u32;
            let rgb = [
                ((event.color >> 16) & 0xff) as f32 / 255.0,
                ((event.color >> 8) & 0xff) as f32 / 255.0,
                (event.color & 0xff) as f32 / 255.0,
            ];
            if self.particles.len() >= MAX_PARTICLES {
                if event.flags & SPF_REPLACE == 0 {
                    continue;
                }
                self.particles.remove(0);
            }
            let fade_mode = event.flags & (SPF_FADE_IN_OUT | SPF_FADE_IN_HOLD_OUT);
            let auto_fade = (event.fade_step < 0.0 && event.flags & SPF_NEGATIVE_FADESTEP == 0)
                || event.fade_step <= -1.0;
            let fade_step = if auto_fade {
                let step = 1.0 / life as f32;
                if event.flags & SPF_FADE_IN_OUT != 0 {
                    -2.0 * step
                } else if event.flags & SPF_FADE_IN_HOLD_OUT != 0 {
                    -3.0 * step
                } else {
                    step
                }
            } else {
                event.fade_step as f32
            };
            let fade_out_step = if auto_fade && fade_mode != 0 {
                -fade_step
            } else {
                event.fade_out_step as f32
            };
            self.particles.push(ParticleInstance {
                pos: Vec3::new(
                    event.pos[0] as f32,
                    event.pos[1] as f32,
                    event.pos[2] as f32,
                ),
                vel: Vec3::new(
                    event.vel[0] as f32,
                    event.vel[1] as f32,
                    event.vel[2] as f32,
                ),
                accel: Vec3::new(
                    event.accel[0] as f32,
                    event.accel[1] as f32,
                    event.accel[2] as f32,
                ),
                size: event.size.abs().max(0.5) as f32,
                light: if event.fullbright {
                    1.0
                } else {
                    (0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]).clamp(0.05, 1.0)
                },
                color: rgb,
                alpha: event.alpha.clamp(0.0, 1.0) as f32,
                life,
                life_max: life,
                fade_step,
                flags: event.flags,
                fade_out_step,
                size_step: event.size_step as f32,
                roll: event.roll[0] as f32,
                roll_velocity: event.roll[1] as f32,
                roll_acceleration: event.roll[2] as f32,
                style: match event.render_style {
                    Some(7) => ParticleStyle::Additive,
                    Some(1 | 6) => ParticleStyle::Translucent,
                    Some(10) => ParticleStyle::Soft,
                    _ if event.fullbright => ParticleStyle::Additive,
                    _ => ParticleStyle::Translucent,
                },
                floor_z: None,
                on_floor: false,
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
            color: [1.0, 1.0, 1.0],
            alpha: 1.0,
            life,
            life_max: life,
            fade_step: 1.0 / life as f32,
            flags: 0,
            fade_out_step: 0.0,
            size_step: 0.0,
            roll: 0.0,
            roll_velocity: 0.0,
            roll_acceleration: 0.0,
            style,
            floor_z: None,
            on_floor: false,
        });
    }

    /// Spawn with an explicit RGB tint.
    pub fn spawn_colored(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        accel: Vec3,
        size: f32,
        light: f32,
        life: u32,
        style: ParticleStyle,
        color: [f32; 3],
    ) {
        self.spawn_colored_ex(pos, vel, accel, size, light, life, style, color, None);
    }

    /// Spawn with tint + optional floor clamp.
    pub fn spawn_colored_ex(
        &mut self,
        pos: Vec3,
        vel: Vec3,
        accel: Vec3,
        size: f32,
        light: f32,
        life: u32,
        style: ParticleStyle,
        color: [f32; 3],
        floor_z: Option<f32>,
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
            color: [
                color[0].clamp(0.0, 1.0),
                color[1].clamp(0.0, 1.0),
                color[2].clamp(0.0, 1.0),
            ],
            alpha: 1.0,
            life,
            life_max: life,
            fade_step: 1.0 / life as f32,
            flags: 0,
            fade_out_step: 0.0,
            size_step: 0.0,
            roll: 0.0,
            roll_velocity: 0.0,
            roll_acceleration: 0.0,
            style,
            floor_z,
            on_floor: floor_z.is_some() && vel.z.abs() < 0.05 && accel.z.abs() < 0.01,
        });
    }

    /// Advance one tic; remove expired. Blood particles with [`ParticleInstance::floor_z`] settle.
    pub fn tick(&mut self) {
        self.tick_with_freeze(false);
    }

    /// Advance a tic while honoring UZDoom's level-freeze particle behavior.
    pub fn tick_with_freeze(&mut self, level_frozen: bool) {
        self.particles.retain_mut(|p| {
            if level_frozen && p.flags & SPF_NOTIMEFREEZE == 0 {
                return true;
            }
            let remaining_tics = p.life;
            p.alpha -= p.fade_step;
            let fade_in_out = p.flags & SPF_FADE_IN_OUT != 0;
            let fade_in_hold_out = p.flags & SPF_FADE_IN_HOLD_OUT != 0;
            if (fade_in_out && p.alpha >= 1.0)
                || (fade_in_hold_out
                    && remaining_tics as f32 * p.fade_out_step.abs() <= p.alpha.abs().min(1.0))
            {
                p.alpha = p.alpha.abs().min(1.0);
                p.fade_step = p.fade_out_step;
                p.flags &= !(SPF_FADE_IN_OUT | SPF_FADE_IN_HOLD_OUT);
            }
            if p.alpha > 1.0 {
                p.alpha = 1.0;
            }
            p.size += p.size_step;
            if p.alpha <= 0.0 || p.size <= 0.0 {
                return false;
            }
            p.life = p.life.saturating_sub(1);
            if p.life == 0 {
                return false;
            }
            p.roll += p.roll_velocity;
            p.roll_velocity += p.roll_acceleration;
            p.vel += p.accel;
            p.pos += p.vel;
            if let Some(fz) = p.floor_z {
                let pad = (p.size * 0.08).clamp(0.35, 1.2);
                if p.pos.z <= fz + pad {
                    p.pos.z = fz + pad;
                    p.vel.x *= 0.45;
                    p.vel.y *= 0.45;
                    p.vel.z = 0.0;
                    p.accel = Vec3::ZERO;
                    // Flatten into a horizontal floor smear (not a standing billboard).
                    p.size = (p.size * 1.25).min(12.0);
                    p.style = ParticleStyle::Soft;
                    p.on_floor = true;
                }
            }
            if p.style == ParticleStyle::Soft {
                #[allow(clippy::cast_precision_loss)]
                let frac = p.life as f32 / p.life_max.max(1) as f32;
                p.alpha = (frac * frac).clamp(0.0, 1.0);
            }
            true
        });
    }

    /// Clear (map change).
    pub fn clear(&mut self) {
        self.particles.clear();
    }

    /// Blood spray from a render FX hit (`docs/29` J1).
    ///
    /// `floor_z` clamps droplets onto the sector floor so they don't float mid-air.
    pub fn ingest_blood_spray(
        &mut self,
        pos: Vec3,
        dir: Vec3,
        color_rgb: [u8; 3],
        damage: i32,
        seed: u64,
        floor_z: Option<f32>,
    ) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let dir = {
            let n = dir.length();
            if n < 1e-3 { Vec3::Z } else { dir / n }
        };
        let right = dir.cross(Vec3::Z);
        let right = if right.length_squared() < 1e-4 {
            Vec3::X
        } else {
            right.normalize()
        };
        let up = right.cross(dir).normalize_or_zero();
        let n = (4 + damage.clamp(1, 40) / 2).min(48) as usize;
        let r = f32::from(color_rgb[0]) / 255.0;
        let g = f32::from(color_rgb[1]) / 255.0;
        let b = f32::from(color_rgb[2]) / 255.0;
        let lum = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.35, 1.0);
        // Bias spray downward so more droplets reach the floor.
        let gravity = Vec3::new(0.0, 0.0, -0.32);
        for i in 0..n {
            let spread = next(&mut rng) * 0.55;
            let yaw = (next(&mut rng) - 0.5) * std::f32::consts::TAU;
            let pitch = (next(&mut rng) - 0.45) * 1.1;
            let side = right * yaw.cos() + up * yaw.sin();
            let vel_dir =
                (dir * (1.0 - spread) + side * spread + Vec3::Z * pitch).normalize_or_zero();
            let speed = 1.0 + next(&mut rng) * 3.2;
            let life = 16 + (next(&mut rng) * 28.0) as u32;
            let size = 1.5 + next(&mut rng) * 3.5;
            self.spawn_colored_ex(
                pos + vel_dir * (next(&mut rng) * 2.0),
                vel_dir * speed,
                gravity,
                size,
                lum,
                life.max(10),
                ParticleStyle::Soft,
                [r, g, b],
                floor_z,
            );
            // Every few droplets, seed a puddle already on the floor along the spray.
            if let Some(fz) = floor_z {
                if i % 3 == 0 {
                    let along = (next(&mut rng) - 0.15) * 18.0;
                    let side_off = (next(&mut rng) - 0.5) * 10.0;
                    let puddle = Vec3::new(
                        pos.x + dir.x * along + right.x * side_off,
                        pos.y + dir.y * along + right.y * side_off,
                        fz + 0.6,
                    );
                    // Spawn already marked for floor orient (horizontal disc).
                    if self.particles.len() >= MAX_PARTICLES {
                        self.particles.remove(0);
                    }
                    let life = (40 + (next(&mut rng) * 40.0) as u32).max(16);
                    self.particles.push(ParticleInstance {
                        pos: puddle,
                        vel: Vec3::new(
                            (next(&mut rng) - 0.5) * 0.25,
                            (next(&mut rng) - 0.5) * 0.25,
                            0.0,
                        ),
                        accel: Vec3::ZERO,
                        size: 3.5 + next(&mut rng) * 5.5,
                        light: (lum * 0.85).clamp(0.0, 1.0),
                        color: [r * 0.85, g * 0.85, b * 0.85],
                        alpha: 1.0,
                        life,
                        life_max: life,
                        fade_step: 1.0 / life as f32,
                        flags: 0,
                        fade_out_step: 0.0,
                        size_step: 0.0,
                        roll: 0.0,
                        roll_velocity: 0.0,
                        roll_acceleration: 0.0,
                        style: ParticleStyle::Soft,
                        floor_z: Some(fz),
                        on_floor: true,
                    });
                }
            }
        }
    }

    /// Shell casing eject (`docs/29` I7 MVP) — short arc, then fade.
    pub fn ingest_casing(&mut self, origin: Vec3, forward: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let fwd = {
            let n = forward.length();
            if n < 1e-3 { Vec3::X } else { forward / n }
        };
        let right = fwd.cross(Vec3::Z).normalize_or_zero();
        let side = if right.length_squared() < 1e-4 {
            Vec3::Y
        } else {
            right
        };
        let pos = origin + fwd * 12.0 + Vec3::Z * 14.0 + side * 4.0;
        let vel = side * (2.5 + next(&mut rng) * 2.0)
            + Vec3::Z * (2.0 + next(&mut rng))
            + fwd * (next(&mut rng) - 0.3);
        self.spawn_colored(
            pos,
            vel,
            Vec3::new(0.0, 0.0, -0.35),
            1.2 + next(&mut rng),
            0.9,
            35 + (next(&mut rng) * 25.0) as u32,
            ParticleStyle::Translucent,
            [0.85, 0.7, 0.25],
        );
    }

    /// Barrel smoke wisps (`docs/29` I6 MVP).
    pub fn ingest_weapon_smoke(&mut self, origin: Vec3, forward: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let fwd = {
            let n = forward.length();
            if n < 1e-3 { Vec3::X } else { forward / n }
        };
        let tip = origin + fwd * 16.0 + Vec3::Z * 12.0;
        for _ in 0..3 {
            let j = Vec3::new(
                next(&mut rng) - 0.5,
                next(&mut rng) - 0.5,
                next(&mut rng) * 0.5,
            );
            self.spawn_colored(
                tip + j * 3.0,
                Vec3::Z * (0.4 + next(&mut rng) * 0.6) + j * 0.3,
                Vec3::new(0.0, 0.0, 0.01),
                5.0 + next(&mut rng) * 8.0,
                0.4,
                20 + (next(&mut rng) * 25.0) as u32,
                ParticleStyle::Soft,
                [0.45, 0.45, 0.42],
            );
        }
    }

    /// Muzzle flash burst (`docs/29` I1) — short additive HDR-ish sparks.
    pub fn ingest_muzzle_flash(&mut self, origin: Vec3, forward: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let fwd = {
            let n = forward.length();
            if n < 1e-3 { Vec3::X } else { forward / n }
        };
        let tip = origin + fwd * 18.0 + Vec3::Z * 12.0;
        for _ in 0..6 {
            let jitter = Vec3::new(
                next(&mut rng) - 0.5,
                next(&mut rng) - 0.5,
                next(&mut rng) - 0.5,
            );
            self.spawn_colored(
                tip + jitter * 4.0,
                fwd * (2.0 + next(&mut rng) * 4.0) + jitter * 2.0,
                Vec3::ZERO,
                6.0 + next(&mut rng) * 10.0,
                1.0,
                2 + (next(&mut rng) * 2.0) as u32,
                ParticleStyle::Additive,
                [1.0, 0.85 + next(&mut rng) * 0.15, 0.35],
            );
        }
    }

    /// Hitscan tracer streak (`docs/29` I2).
    pub fn ingest_tracer(&mut self, origin: Vec3, end: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let delta = end - origin;
        let len = delta.length().max(1.0);
        let dir = delta / len;
        let steps = ((len / 48.0) as usize).clamp(2, 12);
        for i in 0..steps {
            let t = (i as f32 + 0.35) / steps as f32;
            let pos = origin + dir * (len * t);
            self.spawn_colored(
                pos,
                dir * (0.5 + next(&mut rng)),
                Vec3::ZERO,
                2.0 + next(&mut rng) * 2.5,
                1.0,
                2 + (next(&mut rng) * 2.0) as u32,
                ParticleStyle::Additive,
                [1.0, 0.92, 0.55],
            );
        }
    }

    /// Projectile smoke / exhaust puff (`docs/29` I9 MVP).
    pub fn ingest_trail_puff(&mut self, pos: Vec3, vel: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let back = {
            let n = vel.length();
            if n < 1e-3 { Vec3::ZERO } else { -vel / n }
        };
        self.spawn_colored(
            pos + back * (4.0 + next(&mut rng) * 6.0),
            back * (0.4 + next(&mut rng) * 0.8) + Vec3::Z * (0.1 + next(&mut rng) * 0.3),
            Vec3::new(0.0, 0.0, 0.02),
            4.0 + next(&mut rng) * 6.0,
            0.55,
            12 + (next(&mut rng) * 16.0) as u32,
            ParticleStyle::Soft,
            [0.55, 0.55, 0.5],
        );
        self.spawn_colored(
            pos,
            back * 0.5,
            Vec3::ZERO,
            3.0 + next(&mut rng) * 4.0,
            1.0,
            3 + (next(&mut rng) * 3.0) as u32,
            ParticleStyle::Additive,
            [1.0, 0.55, 0.12],
        );
    }

    /// Corpse blood pool / gib burst (`docs/29` J5/J6 MVP).
    pub fn ingest_corpse_pool(
        &mut self,
        pos: Vec3,
        color_rgb: [u8; 3],
        mass: i32,
        gibbed: bool,
        seed: u64,
        floor_z: Option<f32>,
    ) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let r = f32::from(color_rgb[0]) / 255.0;
        let g = f32::from(color_rgb[1]) / 255.0;
        let b = f32::from(color_rgb[2]) / 255.0;
        let n = if gibbed {
            (12 + mass / 20).clamp(12, 40) as usize
        } else {
            (4 + mass / 40).clamp(4, 16) as usize
        };
        let fz = floor_z.unwrap_or(pos.z);
        for _ in 0..n {
            let yaw = next(&mut rng) * std::f32::consts::TAU;
            let dir = Vec3::new(yaw.cos(), yaw.sin(), 0.05 + next(&mut rng) * 0.35);
            let speed = if gibbed {
                1.5 + next(&mut rng) * 4.0
            } else {
                0.4 + next(&mut rng) * 1.2
            };
            self.spawn_colored_ex(
                Vec3::new(pos.x, pos.y, fz + 2.0 + next(&mut rng) * 4.0),
                dir * speed,
                Vec3::new(0.0, 0.0, -0.28),
                if gibbed {
                    2.5 + next(&mut rng) * 4.0
                } else {
                    3.0 + next(&mut rng) * 5.0
                },
                0.7,
                if gibbed {
                    18 + (next(&mut rng) * 20.0) as u32
                } else {
                    40 + (next(&mut rng) * 40.0) as u32
                },
                ParticleStyle::Soft,
                [r, g, b],
                Some(fz),
            );
        }
    }

    /// Explosion debris / fireball sparks (`docs/29` I8 MVP).
    pub fn ingest_explosion(&mut self, pos: Vec3, radius: f32, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let n = ((radius / 16.0) as usize).clamp(12, 48);
        for _ in 0..n {
            let yaw = next(&mut rng) * std::f32::consts::TAU;
            let pitch = (next(&mut rng) - 0.15) * 1.2;
            let dir = Vec3::new(
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            );
            let speed = 2.0 + next(&mut rng) * (radius * 0.08).clamp(2.0, 10.0);
            self.spawn_colored(
                pos + dir * (next(&mut rng) * 4.0),
                dir * speed,
                Vec3::new(0.0, 0.0, -0.12),
                3.0 + next(&mut rng) * 8.0,
                1.0,
                10 + (next(&mut rng) * 18.0) as u32,
                ParticleStyle::Additive,
                [1.0, 0.45 + next(&mut rng) * 0.4, 0.08],
            );
        }
    }

    /// Surface impact sparks (`docs/29` I3) — stone/metal MVP.
    pub fn ingest_impact_sparks(&mut self, pos: Vec3, inward: Vec3, seed: u64) {
        let mut rng = seed;
        let next = |r: &mut u64| -> f32 {
            *r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((*r >> 33) as u32 as f32) * (1.0 / 2147483648.0)
        };
        let n = {
            let l = inward.length();
            if l < 1e-3 { Vec3::Z } else { -inward / l }
        };
        for _ in 0..10 {
            let jitter = Vec3::new(next(&mut rng) - 0.5, next(&mut rng) - 0.5, next(&mut rng));
            let vel = (n + jitter * 1.4).normalize_or_zero() * (2.0 + next(&mut rng) * 5.0);
            self.spawn_colored(
                pos + n * 1.5,
                vel,
                Vec3::new(0.0, 0.0, -0.25),
                1.2 + next(&mut rng) * 2.0,
                1.0,
                6 + (next(&mut rng) * 10.0) as u32,
                ParticleStyle::Additive,
                [1.0, 0.55 + next(&mut rng) * 0.35, 0.15],
            );
        }
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
                color_alpha: [
                    p.color[0] * light,
                    p.color[1] * light,
                    p.color[2] * light,
                    p.alpha,
                ],
                style,
                _pad: [0; 3],
            }
        })
        .collect()
}

/// Append particle billboards into the sprite lists.
///
/// When `stretch` is true (`r_particlefx`), elongate along velocity (E6 MVP).
pub fn append_particles(lists: &mut SceneLists, store: &ParticleStore, camera: &Camera) {
    append_particles_ex(lists, store, camera, false);
}

/// Like [`append_particles`] with optional velocity stretch.
pub fn append_particles_ex(
    lists: &mut SceneLists,
    store: &ParticleStore,
    camera: &Camera,
    stretch: bool,
) {
    for p in &store.particles {
        if p.alpha <= 0.001 {
            continue;
        }
        let q = quantize_rgb(p.color);
        let near_white = q[0] > 240 && q[1] > 240 && q[2] > 240;
        let tex = if near_white {
            match p.style {
                ParticleStyle::Additive => PARTICLE_TEX_ADD.to_owned(),
                ParticleStyle::Translucent | ParticleStyle::Soft => PARTICLE_TEX.to_owned(),
            }
        } else {
            particle_tint_tex_name(q)
        };
        let light = if p.style == ParticleStyle::Additive {
            1.0
        } else {
            p.light
        };
        let speed = p.vel.length();
        let (width, height, orient) = if p.on_floor {
            // Horizontal smear on the floor (never a standing billboard).
            (
                p.size,
                p.size * 0.85,
                crate::sprites::SpriteOrient::Flat {
                    yaw: p.pos.x * 0.19 + p.pos.y * 0.11,
                },
            )
        } else if stretch && speed > 0.5 {
            (
                p.size * (1.0 + speed * 0.35).min(3.5),
                p.size * 0.65,
                crate::sprites::SpriteOrient::Billboard,
            )
        } else {
            (p.size, p.size, crate::sprites::SpriteOrient::Billboard)
        };
        #[allow(clippy::cast_possible_truncation)]
        let start = lists.sprites.len() as u32;
        let mut quad = emit_sprite_quad(
            SpriteInstance {
                pos: p.pos,
                width,
                height,
                light,
                alpha: p.alpha,
                fuzz: false,
                texture: tex.clone(),
                flip_u: false,
                anchor_u: 0.5,
                orient,
                additive: false,
            },
            camera.forward,
        );
        if p.roll.abs() > 1.0e-5 && !p.on_floor {
            let axis = camera.forward.normalize_or_zero();
            let angle = p.roll.to_radians();
            let (sin, cos) = angle.sin_cos();
            for vertex in &mut quad {
                let point = Vec3::from_array(vertex.pos) - p.pos;
                let rotated =
                    point * cos + axis.cross(point) * sin + axis * axis.dot(point) * (1.0 - cos);
                vertex.pos = (p.pos + rotated).to_array();
            }
        }
        lists.sprites.extend_from_slice(&quad);
        if p.flags & SPF_NODYNAMICLIGHTING == 0 {
            crate::scene::apply_dyn_lights_to_sprite_range(lists, start..start.saturating_add(6));
        }
        lists.sprite_ranges.push(MaterialRange {
            name: tex,
            start,
            count: 6,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blood_spray_spawns_colored() {
        let mut store = ParticleStore::default();
        store.ingest_blood_spray(Vec3::ZERO, Vec3::X, [180, 16, 16], 20, 42, Some(0.0));
        assert!(store.particles.len() >= 4);
        assert!(store.particles.iter().all(|p| p.color[0] > p.color[1]));
        assert!(store.particles.iter().any(|p| p.floor_z == Some(0.0)));
    }

    #[test]
    fn blood_spray_settles_on_floor() {
        let mut store = ParticleStore::default();
        store.spawn_colored_ex(
            Vec3::new(0.0, 0.0, 8.0),
            Vec3::new(0.0, 0.0, -2.0),
            Vec3::new(0.0, 0.0, -0.5),
            3.0,
            1.0,
            30,
            ParticleStyle::Soft,
            [1.0, 0.0, 0.0],
            Some(0.0),
        );
        assert!(!store.particles[0].on_floor);
        for _ in 0..10 {
            store.tick();
        }
        assert!(!store.particles.is_empty());
        assert!(store.particles[0].pos.z <= 2.0);
        assert!(store.particles[0].vel.z.abs() < 1e-3);
        assert!(store.particles[0].on_floor);
    }

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

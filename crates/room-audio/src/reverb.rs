//! Sound environments (ZDoom `REVERBS`, `SoundEnvironment`) and a software
//! reverb that renders them.
//!
//! The built-in table and UZDoom's stock `reverbs.txt` (vendored in
//! `data/uzdoom_reverbs.txt`, GPL-3.0-or-later like Room) are always
//! present; a mod's `REVERBS` lump adds or replaces entries. An environment
//! is named by two numbers, `id1 id2`, stored as `id1 << 8 | id2`.
//!
//! UZDoom hands these EAX/EFX parameters to OpenAL; Room renders them with a
//! port of OpenAL Soft's reverb ([`crate::efx`]) using UZDoom's parameter
//! mapping. Render-side only: no gameplay state depends on it.

use std::collections::BTreeMap;

/// EAX / EFX reverb parameters (ZDoom `REVERB_PROPERTIES`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbProps {
    /// EAX base environment index (0–25) used to fill unset REVERBS fields.
    pub environment: i32,
    /// Room size in metres.
    pub env_size: f32,
    /// Echo density, 0–1.
    pub env_diffusion: f32,
    /// Master room effect level, mB (−10000 = off).
    pub room: i32,
    /// High-frequency room level, mB.
    pub room_hf: i32,
    /// Low-frequency room level, mB.
    pub room_lf: i32,
    /// Late reverb decay time, seconds.
    pub decay_time: f32,
    /// HF / mid decay ratio.
    pub decay_hf_ratio: f32,
    /// LF / mid decay ratio.
    pub decay_lf_ratio: f32,
    /// Early reflections level, mB.
    pub reflections: i32,
    /// Early reflections delay, seconds.
    pub reflections_delay: f32,
    /// Late reverb level, mB.
    pub reverb: i32,
    /// Late reverb delay after the early reflections, seconds.
    pub reverb_delay: f32,
    /// Echo time, seconds.
    pub echo_time: f32,
    /// Echo depth, 0–1.
    pub echo_depth: f32,
    /// Modulation time, seconds.
    pub modulation_time: f32,
    /// Modulation depth, 0–1.
    pub modulation_depth: f32,
    /// Air absorption per metre at HF, mB.
    pub air_absorption_hf: f32,
    /// HF reference, Hz.
    pub hf_reference: f32,
    /// LF reference, Hz.
    pub lf_reference: f32,
    /// Room rolloff factor.
    pub room_rolloff: f32,
    /// Diffusion, percent.
    pub diffusion: f32,
    /// Density, percent.
    pub density: f32,
    /// `REVERB_FLAGS` (`bDecayTimeScale` = bit 0, …).
    pub flags: u32,
    /// Direction the early reflections come from (`ReflectionsPanX/Y/Z`;
    /// EAX left-handed: x right, y up, z front; length up to 1 = focus).
    pub reflections_pan: [f32; 3],
    /// Direction of the late reverb (`ReverbPanX/Y/Z`).
    pub reverb_pan: [f32; 3],
}

/// One named environment.
#[derive(Clone, Debug, PartialEq)]
pub struct ReverbEnv {
    /// Display name (`"Castle Alcove"`).
    pub name: String,
    /// `id1 << 8 | id2`.
    pub id: u16,
    /// Built into the engine (not from a lump).
    pub builtin: bool,
    /// ZDoom's software under-water filter ("DSP Water") instead of reverb.
    pub software_water: bool,
    /// Parameters.
    pub props: ReverbProps,
}

/// ID of the "Off" environment.
pub const ENV_OFF: u16 = 0x0000;
/// ID of ZDoom's "DSP Water" environment (used while the listener is under water).
pub const ENV_DSP_WATER: u16 = 0xFFFF;
/// ID of the EAX "Underwater" preset.
pub const ENV_UNDERWATER: u16 = 0x1600;

fn builtin(name: &str, id: u16, software_water: bool, props: ReverbProps) -> ReverbEnv {
    ReverbEnv {
        name: name.to_owned(),
        id,
        builtin: true,
        software_water,
        props,
    }
}

/// UZDoom `s_environment.cpp` built-ins, in `DefaultEnvironments` order
/// first (index = EAX environment), then the extras.
#[allow(clippy::too_many_lines, clippy::unreadable_literal)]
fn builtins() -> Vec<ReverbEnv> {
    vec![
        builtin(
            "Off",
            0x0000,
            false,
            ReverbProps {
                environment: 0,
                env_size: 7.5,
                env_diffusion: 1.00,
                room: -10000,
                room_hf: -10000,
                room_lf: 0,
                decay_time: 1.00,
                decay_hf_ratio: 1.00,
                decay_lf_ratio: 1.0,
                reflections: -2602,
                reflections_delay: 0.007,
                reverb: 200,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 0.0,
                density: 0.0,
                flags: 0x33f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Generic",
            0x0001,
            false,
            ReverbProps {
                environment: 0,
                env_size: 7.5,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -100,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.83,
                decay_lf_ratio: 1.0,
                reflections: -2602,
                reflections_delay: 0.007,
                reverb: 200,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Padded Cell",
            0x0100,
            false,
            ReverbProps {
                environment: 1,
                env_size: 1.4,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -6000,
                room_lf: 0,
                decay_time: 0.17,
                decay_hf_ratio: 0.10,
                decay_lf_ratio: 1.0,
                reflections: -1204,
                reflections_delay: 0.001,
                reverb: 207,
                reverb_delay: 0.002,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Room",
            0x0200,
            false,
            ReverbProps {
                environment: 2,
                env_size: 1.9,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -454,
                room_lf: 0,
                decay_time: 0.40,
                decay_hf_ratio: 0.83,
                decay_lf_ratio: 1.0,
                reflections: -1646,
                reflections_delay: 0.002,
                reverb: 53,
                reverb_delay: 0.003,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Bathroom",
            0x0300,
            false,
            ReverbProps {
                environment: 3,
                env_size: 1.4,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -1200,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.54,
                decay_lf_ratio: 1.0,
                reflections: -370,
                reflections_delay: 0.007,
                reverb: 1030,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 60.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Living Room",
            0x0400,
            false,
            ReverbProps {
                environment: 4,
                env_size: 2.5,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -6000,
                room_lf: 0,
                decay_time: 0.50,
                decay_hf_ratio: 0.10,
                decay_lf_ratio: 1.0,
                reflections: -1376,
                reflections_delay: 0.003,
                reverb: -1104,
                reverb_delay: 0.004,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Stone Room",
            0x0500,
            false,
            ReverbProps {
                environment: 5,
                env_size: 11.6,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -300,
                room_lf: 0,
                decay_time: 2.31,
                decay_hf_ratio: 0.64,
                decay_lf_ratio: 1.0,
                reflections: -711,
                reflections_delay: 0.012,
                reverb: 83,
                reverb_delay: 0.017,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Auditorium",
            0x0600,
            false,
            ReverbProps {
                environment: 6,
                env_size: 21.6,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -476,
                room_lf: 0,
                decay_time: 4.32,
                decay_hf_ratio: 0.59,
                decay_lf_ratio: 1.0,
                reflections: -789,
                reflections_delay: 0.020,
                reverb: -289,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Concert Hall",
            0x0700,
            false,
            ReverbProps {
                environment: 7,
                env_size: 19.6,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -500,
                room_lf: 0,
                decay_time: 3.92,
                decay_hf_ratio: 0.70,
                decay_lf_ratio: 1.0,
                reflections: -1230,
                reflections_delay: 0.020,
                reverb: -2,
                reverb_delay: 0.029,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Cave",
            0x0800,
            false,
            ReverbProps {
                environment: 8,
                env_size: 14.6,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: 0,
                room_lf: 0,
                decay_time: 2.91,
                decay_hf_ratio: 1.30,
                decay_lf_ratio: 1.0,
                reflections: -602,
                reflections_delay: 0.015,
                reverb: -302,
                reverb_delay: 0.022,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Arena",
            0x0900,
            false,
            ReverbProps {
                environment: 9,
                env_size: 36.2,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -698,
                room_lf: 0,
                decay_time: 7.24,
                decay_hf_ratio: 0.33,
                decay_lf_ratio: 1.0,
                reflections: -1166,
                reflections_delay: 0.020,
                reverb: 16,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Hangar",
            0x0A00,
            false,
            ReverbProps {
                environment: 10,
                env_size: 50.3,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -1000,
                room_lf: 0,
                decay_time: 10.05,
                decay_hf_ratio: 0.23,
                decay_lf_ratio: 1.0,
                reflections: -602,
                reflections_delay: 0.020,
                reverb: 198,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Carpetted Hallway",
            0x0B00,
            false,
            ReverbProps {
                environment: 11,
                env_size: 1.9,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -4000,
                room_lf: 0,
                decay_time: 0.30,
                decay_hf_ratio: 0.10,
                decay_lf_ratio: 1.0,
                reflections: -1831,
                reflections_delay: 0.002,
                reverb: -1630,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Hallway",
            0x0C00,
            false,
            ReverbProps {
                environment: 12,
                env_size: 1.8,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -300,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.59,
                decay_lf_ratio: 1.0,
                reflections: -1219,
                reflections_delay: 0.007,
                reverb: 441,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Stone Corridor",
            0x0D00,
            false,
            ReverbProps {
                environment: 13,
                env_size: 13.5,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -237,
                room_lf: 0,
                decay_time: 2.70,
                decay_hf_ratio: 0.79,
                decay_lf_ratio: 1.0,
                reflections: -1214,
                reflections_delay: 0.013,
                reverb: 395,
                reverb_delay: 0.020,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Alley",
            0x0E00,
            false,
            ReverbProps {
                environment: 14,
                env_size: 7.5,
                env_diffusion: 0.30,
                room: -1000,
                room_hf: -270,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.86,
                decay_lf_ratio: 1.0,
                reflections: -1204,
                reflections_delay: 0.007,
                reverb: -4,
                reverb_delay: 0.011,
                echo_time: 0.125,
                echo_depth: 0.95,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Forest",
            0x0F00,
            false,
            ReverbProps {
                environment: 15,
                env_size: 38.0,
                env_diffusion: 0.30,
                room: -1000,
                room_hf: -3300,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.54,
                decay_lf_ratio: 1.0,
                reflections: -2560,
                reflections_delay: 0.162,
                reverb: -229,
                reverb_delay: 0.088,
                echo_time: 0.125,
                echo_depth: 1.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 79.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "City",
            0x1000,
            false,
            ReverbProps {
                environment: 16,
                env_size: 7.5,
                env_diffusion: 0.50,
                room: -1000,
                room_hf: -800,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.67,
                decay_lf_ratio: 1.0,
                reflections: -2273,
                reflections_delay: 0.007,
                reverb: -1691,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 50.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Mountains",
            0x1100,
            false,
            ReverbProps {
                environment: 17,
                env_size: 100.0,
                env_diffusion: 0.27,
                room: -1000,
                room_hf: -2500,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.21,
                decay_lf_ratio: 1.0,
                reflections: -2780,
                reflections_delay: 0.300,
                reverb: -1434,
                reverb_delay: 0.100,
                echo_time: 0.250,
                echo_depth: 1.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 27.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Quarry",
            0x1200,
            false,
            ReverbProps {
                environment: 18,
                env_size: 17.5,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -1000,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.83,
                decay_lf_ratio: 1.0,
                reflections: -10000,
                reflections_delay: 0.061,
                reverb: 500,
                reverb_delay: 0.025,
                echo_time: 0.125,
                echo_depth: 0.70,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Plain",
            0x1300,
            false,
            ReverbProps {
                environment: 19,
                env_size: 42.5,
                env_diffusion: 0.21,
                room: -1000,
                room_hf: -2000,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.50,
                decay_lf_ratio: 1.0,
                reflections: -2466,
                reflections_delay: 0.179,
                reverb: -1926,
                reverb_delay: 0.100,
                echo_time: 0.250,
                echo_depth: 1.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 21.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Parking Lot",
            0x1400,
            false,
            ReverbProps {
                environment: 20,
                env_size: 8.3,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: 0,
                room_lf: 0,
                decay_time: 1.65,
                decay_hf_ratio: 1.50,
                decay_lf_ratio: 1.0,
                reflections: -1363,
                reflections_delay: 0.008,
                reverb: -1153,
                reverb_delay: 0.012,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Sewer Pipe",
            0x1500,
            false,
            ReverbProps {
                environment: 21,
                env_size: 1.7,
                env_diffusion: 0.80,
                room: -1000,
                room_hf: -1000,
                room_lf: 0,
                decay_time: 2.81,
                decay_hf_ratio: 0.14,
                decay_lf_ratio: 1.0,
                reflections: 429,
                reflections_delay: 0.014,
                reverb: 1023,
                reverb_delay: 0.021,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 80.0,
                density: 60.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Sewer Pipe 2",
            0x1502,
            false,
            ReverbProps {
                environment: 21,
                env_size: 1.7,
                env_diffusion: 0.80,
                room: -1000,
                room_hf: -1000,
                room_lf: 0,
                decay_time: 1.81,
                decay_hf_ratio: 0.14,
                decay_lf_ratio: 1.0,
                reflections: 229,
                reflections_delay: 0.014,
                reverb: 1023,
                reverb_delay: 0.021,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 80.0,
                density: 60.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Sewer Pipe 3",
            0x1503,
            false,
            ReverbProps {
                environment: 21,
                env_size: 1.7,
                env_diffusion: 0.80,
                room: -1000,
                room_hf: -1000,
                room_lf: 0,
                decay_time: 1.54,
                decay_hf_ratio: 0.14,
                decay_lf_ratio: 1.0,
                reflections: 200,
                reflections_delay: 0.014,
                reverb: 1023,
                reverb_delay: 0.021,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 80.0,
                density: 60.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Underwater",
            0x1600,
            false,
            ReverbProps {
                environment: 22,
                env_size: 1.8,
                env_diffusion: 1.00,
                room: -1000,
                room_hf: -4000,
                room_lf: 0,
                decay_time: 1.49,
                decay_hf_ratio: 0.10,
                decay_lf_ratio: 1.0,
                reflections: -449,
                reflections_delay: 0.007,
                reverb: 1700,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 1.18,
                modulation_depth: 0.348,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x3f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Drugged",
            0x1700,
            false,
            ReverbProps {
                environment: 23,
                env_size: 1.9,
                env_diffusion: 0.50,
                room: -1000,
                room_hf: 0,
                room_lf: 0,
                decay_time: 8.39,
                decay_hf_ratio: 1.39,
                decay_lf_ratio: 1.0,
                reflections: -115,
                reflections_delay: 0.002,
                reverb: 985,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 1.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Dizzy",
            0x1800,
            false,
            ReverbProps {
                environment: 24,
                env_size: 1.8,
                env_diffusion: 0.60,
                room: -1000,
                room_hf: -400,
                room_lf: 0,
                decay_time: 17.23,
                decay_hf_ratio: 0.56,
                decay_lf_ratio: 1.0,
                reflections: -1713,
                reflections_delay: 0.020,
                reverb: -613,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 1.00,
                modulation_time: 0.81,
                modulation_depth: 0.310,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "Psychotic",
            0x1900,
            false,
            ReverbProps {
                environment: 25,
                env_size: 1.0,
                env_diffusion: 0.50,
                room: -1000,
                room_hf: -151,
                room_lf: 0,
                decay_time: 7.56,
                decay_hf_ratio: 0.91,
                decay_lf_ratio: 1.0,
                reflections: -626,
                reflections_delay: 0.020,
                reverb: 774,
                reverb_delay: 0.030,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 4.00,
                modulation_depth: 1.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 100.0,
                density: 100.0,
                flags: 0x1f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
        builtin(
            "DSP Water",
            0xFFFF,
            true,
            ReverbProps {
                environment: 0,
                env_size: 7.5,
                env_diffusion: 1.00,
                room: -10000,
                room_hf: -10000,
                room_lf: 0,
                decay_time: 1.00,
                decay_hf_ratio: 1.00,
                decay_lf_ratio: 1.0,
                reflections: -2602,
                reflections_delay: 0.007,
                reverb: 200,
                reverb_delay: 0.011,
                echo_time: 0.250,
                echo_depth: 0.00,
                modulation_time: 0.25,
                modulation_depth: 0.000,
                air_absorption_hf: -5.0,
                hf_reference: 5000.0,
                lf_reference: 250.0,
                room_rolloff: 0.0,
                diffusion: 0.0,
                density: 0.0,
                flags: 0x33f,
                reflections_pan: [0.0; 3],
                reverb_pan: [0.0; 3],
            },
        ),
    ]
}

/// EAX environment index → built-in ID (`DefaultEnvironments`).
const DEFAULT_ENVIRONMENT_IDS: [u16; 26] = [
    0x0000, 0x0100, 0x0200, 0x0300, 0x0400, 0x0500, 0x0600, 0x0700, 0x0800, 0x0900, 0x0A00, 0x0B00,
    0x0C00, 0x0D00, 0x0E00, 0x0F00, 0x1000, 0x1100, 0x1200, 0x1300, 0x1400, 0x1500, 0x1600, 0x1700,
    0x1800, 0x1900,
];

/// UZDoom's stock `reverbs.txt`.
pub const UZDOOM_REVERBS: &str = include_str!("../data/uzdoom_reverbs.txt");

/// All known environments.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentTable {
    envs: BTreeMap<u16, ReverbEnv>,
}

impl Default for EnvironmentTable {
    fn default() -> Self {
        Self::new()
    }
}

/// REVERBS parse error (line, message).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReverbsError {
    /// 1-based line.
    pub line: usize,
    /// What went wrong.
    pub message: String,
}

impl EnvironmentTable {
    /// Built-ins plus UZDoom's stock `reverbs.txt`.
    #[must_use]
    pub fn new() -> Self {
        let mut t = Self::builtin_only();
        let _ = t.parse_reverbs(UZDOOM_REVERBS);
        t
    }

    /// Only the engine built-ins.
    #[must_use]
    pub fn builtin_only() -> Self {
        Self {
            envs: builtins().into_iter().map(|e| (e.id, e)).collect(),
        }
    }

    /// Number of environments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.envs.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.envs.is_empty()
    }

    /// Every environment, by ID.
    pub fn iter(&self) -> impl Iterator<Item = &ReverbEnv> {
        self.envs.values()
    }

    /// Environment by ID.
    #[must_use]
    pub fn find(&self, id: u16) -> Option<&ReverbEnv> {
        self.envs.get(&id)
    }

    /// Environment by name (case-insensitive), as MAPINFO `DefaultEnvironment "name"`.
    #[must_use]
    pub fn find_name(&self, name: &str) -> Option<&ReverbEnv> {
        self.envs
            .values()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Parse a `REVERBS` lump and add (or replace) its environments. Returns
    /// how many were read; on a syntax error the ones before it are kept.
    pub fn parse_reverbs(&mut self, text: &str) -> Result<usize, ReverbsError> {
        let toks = tokenize(text);
        let mut i = 0usize;
        let mut added = 0usize;
        let err = |line: usize, m: &str| ReverbsError {
            line,
            message: m.to_owned(),
        };
        while i < toks.len() {
            let (name, line) = (&toks[i].0, toks[i].1);
            let id1 = toks.get(i + 1).and_then(|t| t.0.parse::<i32>().ok());
            let id2 = toks.get(i + 2).and_then(|t| t.0.parse::<i32>().ok());
            let (Some(id1), Some(id2)) = (id1, id2) else {
                return Err(err(line, "expected `\"name\" id1 id2 {`"));
            };
            if toks.get(i + 3).map(|t| t.0.as_str()) != Some("{") {
                return Err(err(line, "expected `{`"));
            }
            i += 4;
            let mut set: BTreeMap<String, String> = BTreeMap::new();
            while i < toks.len() && toks[i].0 != "}" {
                let key = toks[i].0.to_ascii_lowercase();
                let Some(val) = toks.get(i + 1) else {
                    return Err(err(toks[i].1, "missing value"));
                };
                set.insert(key, val.0.clone());
                i += 2;
            }
            i += 1; // `}`
            let Some(env_index) = set.get("environment").and_then(|v| v.parse::<usize>().ok())
            else {
                return Err(err(line, "missing `Environment` field"));
            };
            let base = DEFAULT_ENVIRONMENT_IDS
                .get(env_index.min(25))
                .and_then(|id| self.envs.get(id))
                .map_or_else(|| builtins()[0].props, |e| e.props);
            let props = apply_fields(base, &set);
            let id = (u16::try_from(id1.clamp(0, 255)).unwrap_or(0) << 8)
                | u16::try_from(id2.clamp(0, 255)).unwrap_or(0);
            self.envs.insert(
                id,
                ReverbEnv {
                    name: name.clone(),
                    id,
                    builtin: false,
                    software_water: false,
                    props,
                },
            );
            added += 1;
        }
        Ok(added)
    }
}

fn tokenize(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("");
        let mut chars = line.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else if c == '"' {
                chars.next();
                let s: String = chars.by_ref().take_while(|&c| c != '"').collect();
                out.push((s, ln + 1));
            } else if c == '{' || c == '}' {
                chars.next();
                out.push((c.to_string(), ln + 1));
            } else {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                out.push((s, ln + 1));
            }
        }
    }
    out
}

fn apply_fields(mut p: ReverbProps, set: &BTreeMap<String, String>) -> ReverbProps {
    let f = |k: &str| set.get(k).and_then(|v| v.parse::<f32>().ok());
    let i = |k: &str| {
        set.get(k)
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v as i32)
    };
    macro_rules! fset {
        ($key:literal, $field:ident, $lo:expr, $hi:expr) => {
            if let Some(v) = f($key) {
                p.$field = v.clamp($lo, $hi);
            }
        };
    }
    macro_rules! iset {
        ($key:literal, $field:ident, $lo:expr, $hi:expr) => {
            if let Some(v) = i($key) {
                p.$field = v.clamp($lo, $hi);
            }
        };
    }
    iset!("environment", environment, 0, 25);
    fset!("environmentsize", env_size, 1.0, 100.0);
    fset!("environmentdiffusion", env_diffusion, 0.0, 1.0);
    iset!("room", room, -10000, 0);
    iset!("roomhf", room_hf, -10000, 0);
    iset!("roomlf", room_lf, -10000, 0);
    fset!("decaytime", decay_time, 0.1, 20.0);
    fset!("decayhfratio", decay_hf_ratio, 0.1, 2.0);
    fset!("decaylfratio", decay_lf_ratio, 0.1, 2.0);
    iset!("reflections", reflections, -10000, 1000);
    fset!("reflectionsdelay", reflections_delay, 0.0, 0.3);
    iset!("reverb", reverb, -10000, 2000);
    fset!("reverbdelay", reverb_delay, 0.0, 0.1);
    fset!("echotime", echo_time, 0.075, 0.25);
    fset!("echodepth", echo_depth, 0.0, 1.0);
    fset!("modulationtime", modulation_time, 0.04, 4.0);
    fset!("modulationdepth", modulation_depth, 0.0, 1.0);
    fset!("airabsorptionhf", air_absorption_hf, -100.0, 0.0);
    fset!("hfreference", hf_reference, 1000.0, 20000.0);
    fset!("lfreference", lf_reference, 20.0, 1000.0);
    fset!("roomrollofffactor", room_rolloff, 0.0, 10.0);
    fset!("diffusion", diffusion, 0.0, 100.0);
    fset!("density", density, 0.0, 100.0);
    for (i, axis) in ["x", "y", "z"].iter().enumerate() {
        if let Some(v) = f(&format!("reflectionspan{axis}")) {
            p.reflections_pan[i] = v.clamp(-2000.0, 2000.0);
        }
        if let Some(v) = f(&format!("reverbpan{axis}")) {
            p.reverb_pan[i] = v.clamp(-2000.0, 2000.0);
        }
    }
    for (key, bit) in [
        ("bdecaytimescale", 0),
        ("breflectionsscale", 1),
        ("breflectionsdelayscale", 2),
        ("breverbscale", 3),
        ("breverbdelayscale", 4),
        ("bdecayhflimit", 5),
        ("bechotimescale", 6),
        ("bmodulationtimescale", 7),
    ] {
        if let Some(v) = set.get(key) {
            if v.eq_ignore_ascii_case("true") {
                p.flags |= 1 << bit;
            } else {
                p.flags &= !(1 << bit);
            }
        }
    }
    p
}

/// Millibels → linear gain.
#[must_use]
pub fn mb_to_gain(mb: f32) -> f32 {
    10f32.powf(mb / 2000.0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn builtins_and_stock_reverbs_load() {
        let t = EnvironmentTable::new();
        assert_eq!(t.find(ENV_OFF).unwrap().name, "Off");
        assert_eq!(t.find(0x0800).unwrap().name, "Cave");
        assert!(t.find(ENV_DSP_WATER).unwrap().software_water);
        let alcove = t.find(0x1e00).expect("Castle Alcove 30 0");
        assert_eq!(alcove.name, "Castle Alcove");
        // Unset fields come from base environment 20 (Parking Lot).
        assert_eq!(alcove.props.room, t.find(0x1400).unwrap().props.room);
        assert!((alcove.props.decay_time - 1.64).abs() < 1e-4);
        assert!(t.len() > 100, "stock list: {}", t.len());
        assert_eq!(t.find_name("cave").map(|e| e.id), Some(0x0800));
    }

    #[test]
    fn reverbs_lump_overrides_and_rejects_missing_environment() {
        let mut t = EnvironmentTable::builtin_only();
        assert_eq!(
            t.parse_reverbs(
                "\"My Hall\" 200 3 { Environment 7 DecayTime 9.5 bDecayHFLimit false }"
            ),
            Ok(1)
        );
        let e = t.find(0xC803).unwrap();
        assert!((e.props.decay_time - 9.5).abs() < 1e-6);
        assert_eq!(e.props.room, t.find(0x0700).unwrap().props.room);
        assert_eq!(e.props.flags & (1 << 5), 0);
        assert!(t.parse_reverbs("\"Bad\" 1 2 { DecayTime 1 }").is_err());
    }

    fn tail_energy(env: &ReverbEnv) -> f32 {
        let mut r = crate::efx::EfxReverb::new(44_100);
        r.set_params(&crate::efx::EfxParams::from_props(&env.props));
        let mut e = 0.0;
        for i in 0..44_100 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, rr) = r.process_bformat([x, 0.0, 0.0, 0.0]);
            if i > 22_050 {
                e += l * l + rr * rr;
            }
        }
        e
    }

    #[test]
    fn longer_decay_rings_longer() {
        let t = EnvironmentTable::builtin_only();
        let cell = tail_energy(t.find(0x0100).unwrap()); // Padded Cell, 0.17 s
        let hangar = tail_energy(t.find(0x0A00).unwrap()); // Hangar, 10 s
        assert!(hangar > cell * 100.0, "hangar {hangar} vs cell {cell}");
        assert!(t.find(ENV_OFF).unwrap().props.room <= -9000);
    }
}

//! Spectre / `STYLE_Fuzzy` shader variants (`r_drawfuzz` / `gl_fuzztype`).

/// GZDoom-ish fuzz style (`gl_fuzztype`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FuzzType {
    /// No fuzz.
    #[default]
    Off = 0,
    /// Dark shadow noise (classic spectre look).
    Shadow = 1,
    /// Soft translucent noise.
    Smooth = 2,
    /// Animated swirl UV jitter.
    Swirly = 3,
    /// Noise modulates alpha only.
    Translucent = 4,
    /// Column-offset software approximation.
    Software = 5,
    /// Jagged vertical bands.
    Jagged = 6,
}

impl FuzzType {
    /// Parse CVar-style name / digit.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "0" | "off" | "none" => Self::Off,
            "1" | "shadow" => Self::Shadow,
            "2" | "smooth" => Self::Smooth,
            "3" | "swirly" | "swirl" => Self::Swirly,
            "4" | "translucent" | "noise" => Self::Translucent,
            "5" | "software" | "vanilla" => Self::Software,
            "6" | "jagged" => Self::Jagged,
            _ => return None,
        })
    }
}

/// CPU stand-in for fuzz alpha/colour weight (unit tests / soft path).
///
/// Returns `(rgb_scale, alpha_scale)` in 0..=1.
#[must_use]
pub fn fuzz_modulate(kind: FuzzType, u: f32, v: f32, time: f32) -> (f32, f32) {
    if kind == FuzzType::Off {
        return (1.0, 1.0);
    }
    let n = hash21(u * 64.0 + time * 3.0, v * 64.0);
    match kind {
        FuzzType::Off => (1.0, 1.0),
        FuzzType::Shadow => (0.15 + 0.35 * n, 0.55 + 0.35 * n),
        FuzzType::Smooth => (0.4 + 0.4 * n, 0.35 + 0.45 * n),
        FuzzType::Swirly => {
            let s = ((u + time * 0.07).sin() * 0.5 + 0.5) * n;
            (0.25 + 0.5 * s, 0.4 + 0.4 * s)
        }
        FuzzType::Translucent => (1.0, 0.2 + 0.6 * n),
        FuzzType::Software => {
            let col = ((u * 320.0 + time * 8.0).floor() as i32).rem_euclid(3);
            let w = if col == 0 { 0.2 } else { 0.55 + 0.2 * n };
            (w, w)
        }
        FuzzType::Jagged => {
            let band = ((v * 48.0 + time).floor() as i32) & 1;
            let w = if band == 0 { 0.2 } else { 0.7 };
            (w, 0.45 + 0.3 * n)
        }
    }
}

fn hash21(x: f32, y: f32) -> f32 {
    let s = x * 127.1 + y * 311.7;
    (s.sin() * 43_758.547).fract().abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names() {
        assert_eq!(FuzzType::from_name("swirly"), Some(FuzzType::Swirly));
        assert_eq!(FuzzType::from_name("5"), Some(FuzzType::Software));
        assert_eq!(FuzzType::from_name("nope"), None);
    }

    #[test]
    fn off_is_identity() {
        let (c, a) = fuzz_modulate(FuzzType::Off, 0.1, 0.2, 0.0);
        assert!((c - 1.0).abs() < 1e-5 && (a - 1.0).abs() < 1e-5);
    }

    #[test]
    fn shadow_darkens() {
        let (c, _) = fuzz_modulate(FuzzType::Shadow, 0.3, 0.4, 1.0);
        assert!(c < 0.6);
    }
}

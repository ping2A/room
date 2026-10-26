//! Auto head-anchor estimator (`docs/26` §4.3).

use crate::anchor::{HeadAnchor, HeadYaw, Visibility};

/// Hint for yaw / death-frame heuristics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClassHint {
    /// Sprite prefix without frame/rot (e.g. `TROO`).
    pub sprite_prefix: Option<&'static str>,
    /// True when this lump is a death / xdeath frame (index ≥ 2 → prefer Hidden).
    pub death_frame_index: Option<u8>,
}

/// Estimate a [`HeadAnchor`] from a paletted sprite silhouette.
#[must_use]
pub fn estimate_anchor(
    width: u32,
    height: u32,
    alpha: &[u8],
    lump_name: &str,
    hint: ClassHint,
) -> HeadAnchor {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || alpha.len() < w * h {
        return HeadAnchor::front((0.0, 0.0), (1.0, 1.0), 0.0);
    }

    // Row occupied width profile.
    let mut row_span = vec![(w, 0usize); h];
    let mut area = 0u32;
    for y in 0..h {
        let mut min_x = w;
        let mut max_x = 0usize;
        for x in 0..w {
            if alpha[y * w + x] > 0 {
                area += 1;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
        if min_x <= max_x {
            row_span[y] = (min_x, max_x);
        }
    }

    // Topmost solid row.
    let top = row_span
        .iter()
        .position(|&(a, b)| a <= b)
        .unwrap_or(0);
    // Neck = first local minimum of width below the top bump (within 35% height).
    let max_head_h = ((h as f32) * 0.35).ceil() as usize;
    let search_end = (top + max_head_h).min(h.saturating_sub(1));
    let mut neck = search_end;
    let mut prev_w = 0i32;
    let mut decreasing = false;
    for (y, &(a, b)) in row_span.iter().enumerate().take(search_end + 1).skip(top) {
        let ww = if a <= b {
            (b - a + 1) as i32
        } else {
            0
        };
        if prev_w > 0 && ww + 1 < prev_w {
            decreasing = true;
        }
        if decreasing && ww >= prev_w && y > top + 2 {
            neck = y;
            break;
        }
        prev_w = ww;
    }

    let head_rows: Vec<usize> = (top..neck).filter(|&y| row_span[y].0 <= row_span[y].1).collect();
    if head_rows.is_empty() {
        return HeadAnchor {
            center: (w as f32 * 0.5, h as f32 * 0.15),
            radius: (w as f32 * 0.15, h as f32 * 0.12),
            roll: 0.0,
            yaw: yaw_from_lump(lump_name),
            visible: Visibility::Full,
            keep_top: 0.15,
        };
    }
    let mut min_x = w;
    let mut max_x = 0usize;
    for &y in &head_rows {
        min_x = min_x.min(row_span[y].0);
        max_x = max_x.max(row_span[y].1);
    }
    let cx = (min_x + max_x) as f32 * 0.5;
    let cy = (head_rows[0] + *head_rows.last().unwrap_or(&head_rows[0])) as f32 * 0.5;
    let rx = ((max_x - min_x + 1) as f32 * 0.5).max(2.0);
    let ry = ((head_rows.len() as f32) * 0.5).max(2.0);

    let mut visible = Visibility::Full;
    if let Some(idx) = hint.death_frame_index {
        if idx >= 2 {
            visible = Visibility::Hidden;
        }
    }
    // Huge area change vs a thin head → likely gib (caller can refine).
    let head_area = head_rows.len() as u32 * (max_x - min_x + 1) as u32;
    if area > 0 && head_area * 100 / area > 90 && lump_name.ends_with('0') {
        // Rot 0 often death; keep Full unless death hint set.
    }

    HeadAnchor {
        center: (cx, cy),
        radius: (rx, ry),
        roll: 0.0,
        yaw: yaw_from_lump(lump_name),
        visible,
        keep_top: 0.2,
    }
}

/// Doom sprite rotation digit → yaw. Mirrored names (`A2A8`) use the first digit.
#[must_use]
pub fn yaw_from_lump(name: &str) -> HeadYaw {
    let bytes = name.as_bytes();
    // Prefix 4 chars + frame letter + rot digit…
    if bytes.len() < 6 {
        return HeadYaw::Front;
    }
    let rot = bytes[5] as char;
    match rot {
        '1' => HeadYaw::Front,
        '2' => HeadYaw::ThreeQuarterLeft,
        '3' => HeadYaw::ProfileLeft,
        '4' | '5' | '6' => HeadYaw::Back,
        '7' => HeadYaw::ProfileRight,
        '8' => HeadYaw::ThreeQuarterRight,
        '0' => HeadYaw::Front, // often non-rotational / death
        _ => HeadYaw::Front,
    }
}

/// Flip an anchor for a mirrored rotation (x → width-1-x, yaw mirror).
#[must_use]
pub fn mirror_anchor(a: &HeadAnchor, width: u32) -> HeadAnchor {
    let mut out = a.clone();
    out.center.0 = (width as f32 - 1.0) - a.center.0;
    out.yaw = match a.yaw {
        HeadYaw::ThreeQuarterLeft => HeadYaw::ThreeQuarterRight,
        HeadYaw::ThreeQuarterRight => HeadYaw::ThreeQuarterLeft,
        HeadYaw::ProfileLeft => HeadYaw::ProfileRight,
        HeadYaw::ProfileRight => HeadYaw::ProfileLeft,
        other => other,
    };
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn yaw_digits() {
        assert_eq!(yaw_from_lump("TROOA1"), HeadYaw::Front);
        assert_eq!(yaw_from_lump("TROOA2A8"), HeadYaw::ThreeQuarterLeft);
        assert_eq!(yaw_from_lump("TROOA5"), HeadYaw::Back);
    }

    #[test]
    fn estimates_top_blob() {
        let w = 40u32;
        let h = 60u32;
        let mut alpha = vec![0u8; (w * h) as usize];
        // Head blob top-centre.
        for y in 2..14 {
            for x in 14..26 {
                alpha[(y * w + x) as usize] = 255;
            }
        }
        // Body wider below.
        for y in 16..55 {
            for x in 8..32 {
                alpha[(y * w + x) as usize] = 255;
            }
        }
        let a = estimate_anchor(w, h, &alpha, "TROOA1", ClassHint::default());
        assert!(a.center.1 < 20.0, "head centre should be near top, got {:?}", a.center);
        assert!(matches!(a.yaw, HeadYaw::Front));
    }
}

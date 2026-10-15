//! Media bandwidth budgets (docs/22 §8 capacity planning).

/// Opus voice bitrate mid-point (kbit/s).
pub const VOICE_KBPS: u64 = 32;
/// VP8 middle simulcast layer cap (kbit/s).
pub const CAMERA_MIDDLE_KBPS: u64 = 150;
/// Voice + middle camera per remote publisher (kbit/s).
pub const VOICE_CAMERA_MIDDLE_KBPS: u64 = VOICE_KBPS + CAMERA_MIDDLE_KBPS;
/// Spec budget for 8-seat middle-layer egress per subscriber (kbit/s).
pub const EIGHT_SEAT_MIDDLE_BUDGET_KBPS: u64 = 1_300;

/// Egress toward one subscriber when `publishers` seats send voice+middle camera.
#[must_use]
pub fn subscriber_egress_kbps(publishers: u8) -> u64 {
    let remotes = u64::from(publishers.saturating_sub(1));
    remotes.saturating_mul(VOICE_CAMERA_MIDDLE_KBPS)
}

/// Total SFU egress for a room (each of `publishers` listens to the others).
#[must_use]
pub fn room_sfu_egress_kbps(publishers: u8) -> u64 {
    u64::from(publishers).saturating_mul(subscriber_egress_kbps(publishers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_seat_middle_layer_under_budget() {
        // docs/22 §8: 7 × 182 ≈ 1.3 Mbit/s per subscriber.
        assert_eq!(subscriber_egress_kbps(8), 7 * VOICE_CAMERA_MIDDLE_KBPS);
        assert!(subscriber_egress_kbps(8) <= EIGHT_SEAT_MIDDLE_BUDGET_KBPS);
        // ≈ 10 Mbit/s SFU egress for the room.
        assert!(room_sfu_egress_kbps(8) <= 10_400);
    }
}

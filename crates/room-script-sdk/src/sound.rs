//! DMX sound encoder for generated SFX (`docs/31` MA3).

#![cfg(feature = "alloc")]

use alloc::vec::Vec;
use room_script_abi::lump_limits;

/// Encode a Doom DMX lump (`u16` type 3, rate, sample count, 16 pad, PCM, 16 pad).
///
/// # Errors
///
/// Empty PCM or over the 10 s @ 11025 Hz budget.
pub fn encode_dmx(rate: u16, pcm_u8: &[u8]) -> Result<Vec<u8>, &'static str> {
    if pcm_u8.is_empty() {
        return Err("empty pcm");
    }
    if pcm_u8.len() > lump_limits::MAX_SOUND_SAMPLES {
        return Err("sound too long");
    }
    let n = u32::try_from(pcm_u8.len() + 32).map_err(|_| "sound too long")?;
    let mut out = Vec::with_capacity(8 + pcm_u8.len() + 32);
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend(core::iter::repeat_n(128u8, 16));
    out.extend_from_slice(pcm_u8);
    out.extend(core::iter::repeat_n(128u8, 16));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_tone() {
        let dmx = encode_dmx(11_025, &[128, 200, 128, 56]).unwrap();
        assert_eq!(&dmx[0..2], &3u16.to_le_bytes());
        assert_eq!(&dmx[2..4], &11_025u16.to_le_bytes());
        assert_eq!(dmx.len(), 8 + 4 + 32);
    }
}

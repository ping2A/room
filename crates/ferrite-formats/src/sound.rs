//! Decoded PCM sound IR and DMX digital sample parser.

use crate::error::FormatError;

/// Decoded PCM sound (mono or stereo float samples).
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedSound {
    /// Sample rate in Hz.
    pub rate: u32,
    /// Channel count (1 or 2).
    pub channels: u16,
    /// Interleaved `f32` samples in −1.0..=1.0.
    pub samples: Vec<f32>,
}

/// Parse a DMX digital sound lump (`format == 3`).
///
/// Layout: `u16 format(=3), u16 rate, u32 count`, then `count` bytes of
/// unsigned 8-bit PCM. When `count >= 32`, the first and last 16 samples are
/// DMX padding and are stripped (Chocolate Doom / GZDoom behaviour).
pub fn parse_dmx(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if data.len() < 8 {
        return Err(FormatError::Truncated("DMX header"));
    }
    let format = u16::from_le_bytes([data[0], data[1]]);
    if format != 3 {
        return Err(FormatError::Invalid("not DMX digital (format!=3)"));
    }
    let rate = u16::from_le_bytes([data[2], data[3]]) as u32;
    let count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if data.len() < 8 + count {
        return Err(FormatError::Truncated("DMX samples"));
    }
    let raw = &data[8..8 + count];
    let pcm = if count >= 32 {
        &raw[16..count - 16]
    } else {
        raw
    };
    let samples: Vec<f32> = pcm
        .iter()
        .map(|&b| (f32::from(b) - 128.0) / 128.0)
        .collect();
    Ok(DecodedSound {
        rate: if rate == 0 { 11025 } else { rate },
        channels: 1,
        samples,
    })
}

/// True if `data` looks like a DMX digital sample header.
#[must_use]
pub fn is_dmx(data: &[u8]) -> bool {
    data.len() >= 8 && u16::from_le_bytes([data[0], data[1]]) == 3
}

/// Detect and decode a common sound lump (DMX / WAV / Ogg / FLAC).
pub fn decode_sound(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if crate::is_wav(data) {
        return crate::parse_wav(data);
    }
    if crate::is_ogg(data) {
        return crate::parse_ogg(data);
    }
    if crate::is_flac(data) {
        return crate::parse_flac(data);
    }
    if crate::is_mp3(data) {
        return crate::parse_mp3(data);
    }
    if is_dmx(data) {
        return parse_dmx(data);
    }
    Err(FormatError::Invalid("unknown sound format"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn dmx_strips_padding() {
        let mut b = Vec::new();
        b.extend_from_slice(&3u16.to_le_bytes());
        b.extend_from_slice(&11025u16.to_le_bytes());
        let count = 32u32 + 4; // pad + 4 real
        b.extend_from_slice(&count.to_le_bytes());
        b.extend(std::iter::repeat_n(0u8, 16)); // start pad
        b.extend_from_slice(&[128, 255, 0, 128]); // samples
        b.extend(std::iter::repeat_n(0u8, 16)); // end pad
        let s = parse_dmx(&b).unwrap();
        assert_eq!(s.rate, 11025);
        assert_eq!(s.samples.len(), 4);
        assert!((s.samples[0] - 0.0).abs() < 0.01);
        assert!(s.samples[1] > 0.9);
        assert!(s.samples[2] < -0.9);
    }
}

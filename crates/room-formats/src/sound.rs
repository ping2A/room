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

const VOC_MAGIC: &[u8; 20] = b"Creative Voice File\x1a";

/// True if `data` is a Creative Voice File (Duke Nukem 3D / Build sounds).
#[must_use]
pub fn is_voc(data: &[u8]) -> bool {
    data.starts_with(VOC_MAGIC)
}

/// Decode a Creative Voice File: 8-bit unsigned or 16-bit signed PCM blocks
/// (types 1, 2, 3 silence, 8 extended, 9 new format). Repeat markers play once.
pub fn parse_voc(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if !is_voc(data) || data.len() < 26 {
        return Err(FormatError::Invalid("not a VOC file"));
    }
    let mut pos = usize::from(u16::from_le_bytes([data[20], data[21]]));
    let mut rate = 0u32;
    let mut channels = 1u16;
    let mut codec = 0u16;
    let mut ext: Option<(u32, u16)> = None;
    let mut samples = Vec::new();
    while let Some(&kind) = data.get(pos) {
        if kind == 0 {
            break;
        }
        let head = data
            .get(pos + 1..pos + 4)
            .ok_or(FormatError::Truncated("VOC block"))?;
        let size = usize::from(head[0]) | usize::from(head[1]) << 8 | usize::from(head[2]) << 16;
        let body = data
            .get(pos + 4..pos + 4 + size)
            .ok_or(FormatError::Truncated("VOC block body"))?;
        pos += 4 + size;
        let pcm = match kind {
            1 if body.len() >= 2 => {
                if let Some((r, ch)) = ext.take() {
                    rate = r;
                    channels = ch;
                } else {
                    rate = 1_000_000 / (256 - u32::from(body[0]));
                    channels = 1;
                }
                codec = u16::from(body[1]);
                &body[2..]
            }
            2 => body,
            3 if body.len() >= 3 => {
                let len = usize::from(u16::from_le_bytes([body[0], body[1]])) + 1;
                if rate == 0 {
                    rate = 1_000_000 / (256 - u32::from(body[2]));
                }
                samples.extend(std::iter::repeat_n(0.0, len * usize::from(channels)));
                continue;
            }
            8 if body.len() >= 4 => {
                let tc = u32::from(u16::from_le_bytes([body[0], body[1]]));
                let ch = if body[3] == 1 { 2 } else { 1 };
                let r = 256_000_000 / (u32::from(ch) * (65_536 - tc).max(1));
                ext = Some((r, ch));
                continue;
            }
            9 if body.len() >= 12 => {
                rate = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
                channels = u16::from(body[5]).max(1);
                codec = u16::from_le_bytes([body[6], body[7]]);
                &body[12..]
            }
            _ => continue,
        };
        match codec {
            0 => samples.extend(pcm.iter().map(|&b| (f32::from(b) - 128.0) / 128.0)),
            4 => samples.extend(
                pcm.chunks_exact(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0),
            ),
            _ => return Err(FormatError::Invalid("unsupported VOC codec")),
        }
    }
    if samples.is_empty() || rate == 0 {
        return Err(FormatError::Invalid("VOC has no samples"));
    }
    Ok(DecodedSound {
        rate,
        channels,
        samples,
    })
}

/// Detect and decode a common sound lump (DMX / WAV / Ogg / FLAC / MP3 / VOC).
pub fn decode_sound(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if is_voc(data) {
        return parse_voc(data);
    }
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

    fn voc(blocks: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut b = VOC_MAGIC.to_vec();
        b.extend_from_slice(&26u16.to_le_bytes());
        b.extend_from_slice(&0x010Au16.to_le_bytes());
        b.extend_from_slice(&0x1129u16.to_le_bytes());
        for (kind, body) in blocks {
            b.push(*kind);
            b.extend_from_slice(&(body.len() as u32).to_le_bytes()[..3]);
            b.extend_from_slice(body);
        }
        b.push(0);
        b
    }

    #[test]
    fn voc_decodes_pcm_continuation_and_silence() {
        // Divisor 165 → 1_000_000 / 91 ≈ 10989 Hz.
        let data = voc(&[
            (1, vec![165, 0, 128, 255]),
            (2, vec![0]),
            (3, vec![1, 0, 165]),
        ]);
        let s = decode_sound(&data).unwrap();
        assert_eq!((s.rate, s.channels), (10_989, 1));
        assert_eq!(s.samples.len(), 5);
        assert!(s.samples[1] > 0.9 && s.samples[2] < -0.9);
        assert_eq!(&s.samples[3..], [0.0, 0.0]);
    }

    #[test]
    fn voc_new_format_16_bit() {
        let mut body = 22_050u32.to_le_bytes().to_vec();
        body.extend_from_slice(&[16, 1, 4, 0, 0, 0, 0, 0]);
        body.extend_from_slice(&i16::MIN.to_le_bytes());
        let s = parse_voc(&voc(&[(9, body)])).unwrap();
        assert_eq!(s.rate, 22_050);
        assert!((s.samples[0] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn corrupt_voc_is_an_error() {
        let good = voc(&[(1, vec![165, 0, 128, 255])]);
        for len in 0..good.len() - 1 {
            let _ = parse_voc(&good[..len]);
        }
        assert!(parse_voc(&voc(&[(1, vec![165, 2, 1])])).is_err());
        assert!(parse_voc(b"RIFF").is_err());
    }
}

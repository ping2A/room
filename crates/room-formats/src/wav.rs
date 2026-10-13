//! WAV (RIFF/WAVE) PCM decoder subset.

use crate::error::FormatError;
use crate::sound::DecodedSound;

/// Parse a WAV lump into [`DecodedSound`].
///
/// Supports uncompressed PCM (format 1): 8/16-bit, mono/stereo.
pub fn parse_wav(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(FormatError::Invalid("not a WAV"));
    }
    let mut pos = 12usize;
    let mut fmt: Option<FmtChunk> = None;
    let mut pcm: Option<&[u8]> = None;

    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        let start = pos + 8;
        let end = start
            .checked_add(size)
            .ok_or(FormatError::Truncated("WAV chunk"))?;
        if end > data.len() {
            return Err(FormatError::Truncated("WAV chunk data"));
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(FormatError::Truncated("WAV fmt"));
                }
                let audio_format = u16::from_le_bytes([data[start], data[start + 1]]);
                let channels = u16::from_le_bytes([data[start + 2], data[start + 3]]);
                let rate = u32::from_le_bytes([
                    data[start + 4],
                    data[start + 5],
                    data[start + 6],
                    data[start + 7],
                ]);
                let bits = u16::from_le_bytes([data[start + 14], data[start + 15]]);
                fmt = Some(FmtChunk {
                    audio_format,
                    channels,
                    rate,
                    bits,
                });
            }
            b"data" => {
                pcm = Some(&data[start..end]);
            }
            _ => {}
        }
        pos = end + (size % 2); // word align
    }

    let fmt = fmt.ok_or(FormatError::Invalid("WAV missing fmt"))?;
    let pcm = pcm.ok_or(FormatError::Invalid("WAV missing data"))?;
    if fmt.audio_format != 1 {
        return Err(FormatError::Invalid("WAV not PCM"));
    }
    if fmt.channels == 0 || fmt.channels > 2 {
        return Err(FormatError::Invalid("WAV channels"));
    }

    let samples = match fmt.bits {
        8 => pcm
            .iter()
            .map(|&b| (f32::from(b) - 128.0) / 128.0)
            .collect(),
        16 => {
            if pcm.len() % 2 != 0 {
                return Err(FormatError::Truncated("WAV 16-bit"));
            }
            pcm.chunks_exact(2)
                .map(|c| {
                    let s = i16::from_le_bytes([c[0], c[1]]);
                    f32::from(s) / 32768.0
                })
                .collect()
        }
        _ => return Err(FormatError::Invalid("WAV bit depth")),
    };

    Ok(DecodedSound {
        rate: fmt.rate,
        channels: fmt.channels,
        samples,
    })
}

struct FmtChunk {
    audio_format: u16,
    channels: u16,
    rate: u32,
    bits: u16,
}

/// True if `data` looks like a RIFF/WAVE header.
#[must_use]
pub fn is_wav(data: &[u8]) -> bool {
    data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE"
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn pcm8_mono_wav(samples: &[u8], rate: u32) -> Vec<u8> {
        let data_size = samples.len() as u32;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&rate.to_le_bytes());
        let byte_rate = rate; // 1 byte/sample
        v.extend_from_slice(&byte_rate.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // block align
        v.extend_from_slice(&8u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(samples);
        v
    }

    #[test]
    fn decode_pcm8() {
        let wav = pcm8_mono_wav(&[128, 255, 0], 22050);
        let s = parse_wav(&wav).unwrap();
        assert_eq!(s.rate, 22050);
        assert_eq!(s.channels, 1);
        assert_eq!(s.samples.len(), 3);
    }
}

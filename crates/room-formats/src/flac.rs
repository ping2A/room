//! FLAC decode via `claxon`.

use crate::error::FormatError;
use crate::sound::DecodedSound;
use std::io::Cursor;

/// Decode a FLAC bitstream to float PCM.
pub fn parse_flac(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if !is_flac(data) {
        return Err(FormatError::Invalid("not FLAC"));
    }
    let cursor = Cursor::new(data);
    let mut reader = claxon::FlacReader::new(cursor).map_err(|_| FormatError::Invalid("FLAC"))?;
    let info = reader.streaminfo();
    let rate = info.sample_rate;
    let channels = info.channels as u16;
    let bits = info.bits_per_sample;
    if channels == 0 || channels > 2 {
        return Err(FormatError::Invalid("FLAC channels"));
    }
    let scale = (1i64 << (bits.saturating_sub(1)).min(31)) as f32;
    let mut samples = Vec::new();
    {
        let mut blocks = reader.blocks();
        let mut buffer = Vec::new();
        while let Some(block) = blocks
            .read_next_or_eof(buffer)
            .map_err(|_| FormatError::Invalid("FLAC block"))?
        {
            let ch = block.channels() as usize;
            let n = block.len() as usize;
            for i in 0..n {
                for c in 0..ch {
                    let s = block.sample(c as u32, i as u32);
                    samples.push(s as f32 / scale);
                }
            }
            buffer = block.into_buffer();
        }
    }
    Ok(DecodedSound {
        rate,
        channels,
        samples,
    })
}

/// True if `data` starts with a FLAC signature.
#[must_use]
pub fn is_flac(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == b"fLaC"
}

//! MP3 decode via `symphonia` (pure-Rust mp3 feature).

use crate::error::FormatError;
use crate::sound::DecodedSound;
use std::io::Cursor;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;

/// Decode an MP3 bitstream to float PCM.
pub fn parse_mp3(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if !is_mp3(data) {
        return Err(FormatError::Invalid("not MP3"));
    }
    let cursor = Cursor::new(data.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| FormatError::Invalid("MP3 probe"))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or(FormatError::Invalid("MP3 no track"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| FormatError::Invalid("MP3 codec"))?;

    let mut rate = 0u32;
    let mut channels = 0u16;
    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                continue;
            }
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        channels = spec.channels.count() as u16;
        if sample_buf
            .as_ref()
            .is_none_or(|b| b.capacity() < decoded.capacity())
        {
            sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        if let Some(buf) = sample_buf.as_mut() {
            buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(buf.samples());
        }
    }

    if rate == 0 || channels == 0 {
        return Err(FormatError::Invalid("MP3 empty"));
    }
    Ok(DecodedSound {
        rate,
        channels,
        samples,
    })
}

/// Heuristic: ID3 tag or MPEG frame sync.
#[must_use]
pub fn is_mp3(data: &[u8]) -> bool {
    if data.len() >= 3 && &data[0..3] == b"ID3" {
        return true;
    }
    // Frame sync 0xFFEx
    data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0
}

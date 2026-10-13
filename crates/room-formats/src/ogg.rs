//! Ogg Vorbis decode via `lewton`.

use crate::error::FormatError;
use crate::sound::DecodedSound;
use std::io::Cursor;

/// Decode an Ogg Vorbis bitstream to float PCM.
pub fn parse_ogg(data: &[u8]) -> Result<DecodedSound, FormatError> {
    if !is_ogg(data) {
        return Err(FormatError::Invalid("not Ogg"));
    }
    let mut reader = Cursor::new(data);
    let mut pkt_rdr = lewton::inside_ogg::OggStreamReader::new(&mut reader)
        .map_err(|_| FormatError::Invalid("Ogg Vorbis headers"))?;
    let rate = pkt_rdr.ident_hdr.audio_sample_rate;
    let channels = pkt_rdr.ident_hdr.audio_channels as u16;
    if channels == 0 || channels > 2 {
        return Err(FormatError::Invalid("Ogg channels"));
    }
    let mut samples = Vec::new();
    while let Some(pkt) = pkt_rdr
        .read_dec_packet_itl()
        .map_err(|_| FormatError::Invalid("Ogg packet"))?
    {
        for s in pkt {
            samples.push(f32::from(s) / 32768.0);
        }
    }
    Ok(DecodedSound {
        rate,
        channels,
        samples,
    })
}

/// True if `data` starts with an Ogg page.
#[must_use]
pub fn is_ogg(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == b"OggS"
}

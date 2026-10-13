//! Doom DMX `MUS` → standard MIDI (type 0) converter.
//!
//! Behaviour follows Chocolate Doom `mus2mid.c` (Ben Ryves / Simon Howard),
//! which is the reference used by GZDoom's path as well. See `THIRD_PARTY.md`.

use crate::error::FormatError;

const MIDI_PERCUSSION_CHAN: u8 = 9;
const MUS_PERCUSSION_CHAN: u8 = 15;

const CONTROLLER_MAP: [u8; 15] = [
    0x00, 0x20, 0x01, 0x07, 0x0A, 0x0B, 0x5B, 0x5D, 0x40, 0x43, 0x78, 0x7B, 0x7E, 0x7F, 0x79,
];

/// Convert a MUS lump to a type-0 MIDI file.
pub fn mus_to_midi(mus: &[u8]) -> Result<Vec<u8>, FormatError> {
    if mus.len() < 16 || &mus[0..4] != b"MUS\x1A" {
        return Err(FormatError::Invalid("not a MUS file"));
    }
    let score_start = u16::from_le_bytes([mus[6], mus[7]]) as usize;
    if score_start >= mus.len() {
        return Err(FormatError::Truncated("MUS score start"));
    }

    let mut channel_map = [-1i8; 16];
    let mut velocities = [127u8; 16];
    let mut queued_time: u32 = 0;
    let mut track: Vec<u8> = Vec::new();

    let mut pos = score_start;
    let mut score_end = false;

    while !score_end {
        // Block of events until delay bit.
        loop {
            let desc = *mus.get(pos).ok_or(FormatError::Truncated("MUS event"))?;
            pos += 1;
            let mus_chan = desc & 0x0F;
            let event = desc & 0x70;
            let channel =
                get_midi_channel(mus_chan, &mut channel_map, &mut track, &mut queued_time)?;

            match event {
                0x00 => {
                    // release key
                    let key = *mus.get(pos).ok_or(FormatError::Truncated("MUS key"))?;
                    pos += 1;
                    write_time(&mut track, &mut queued_time);
                    track.push(0x80 | channel);
                    track.push(key & 0x7F);
                    track.push(0);
                }
                0x10 => {
                    // press key
                    let key = *mus.get(pos).ok_or(FormatError::Truncated("MUS key"))?;
                    pos += 1;
                    if key & 0x80 != 0 {
                        let vel = *mus.get(pos).ok_or(FormatError::Truncated("MUS vel"))?;
                        pos += 1;
                        velocities[channel as usize] = vel & 0x7F;
                    }
                    write_time(&mut track, &mut queued_time);
                    track.push(0x90 | channel);
                    track.push(key & 0x7F);
                    track.push(velocities[channel as usize]);
                }
                0x20 => {
                    let key = *mus.get(pos).ok_or(FormatError::Truncated("MUS bend"))?;
                    pos += 1;
                    let wheel = u16::from(key) * 64;
                    write_time(&mut track, &mut queued_time);
                    track.push(0xE0 | channel);
                    track.push((wheel & 0x7F) as u8);
                    track.push(((wheel >> 7) & 0x7F) as u8);
                }
                0x30 => {
                    let ctrl = *mus.get(pos).ok_or(FormatError::Truncated("MUS sys"))?;
                    pos += 1;
                    if !(10..=14).contains(&ctrl) {
                        return Err(FormatError::Invalid("MUS system controller"));
                    }
                    write_controller(
                        &mut track,
                        &mut queued_time,
                        channel,
                        CONTROLLER_MAP[ctrl as usize],
                        0,
                    );
                }
                0x40 => {
                    let ctrl = *mus.get(pos).ok_or(FormatError::Truncated("MUS ctrl"))?;
                    pos += 1;
                    let val = *mus.get(pos).ok_or(FormatError::Truncated("MUS ctrl val"))?;
                    pos += 1;
                    if ctrl == 0 {
                        write_time(&mut track, &mut queued_time);
                        track.push(0xC0 | channel);
                        track.push(val & 0x7F);
                    } else {
                        if !(1..=9).contains(&ctrl) {
                            return Err(FormatError::Invalid("MUS controller"));
                        }
                        let mut v = val;
                        if v & 0x80 != 0 {
                            v = 0x7F;
                        }
                        write_controller(
                            &mut track,
                            &mut queued_time,
                            channel,
                            CONTROLLER_MAP[ctrl as usize],
                            v,
                        );
                    }
                }
                0x60 => {
                    score_end = true;
                }
                _ => return Err(FormatError::Invalid("MUS unknown event")),
            }

            if desc & 0x80 != 0 || score_end {
                break;
            }
        }

        if !score_end {
            let mut delay = 0u32;
            loop {
                let b = *mus.get(pos).ok_or(FormatError::Truncated("MUS delay"))?;
                pos += 1;
                delay = delay * 128 + u32::from(b & 0x7F);
                if b & 0x80 == 0 {
                    break;
                }
            }
            queued_time += delay;
        }
    }

    // End of track
    write_time(&mut track, &mut queued_time);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    let track_len = track.len() as u32;
    let mut out = Vec::with_capacity(22 + track.len());
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // type 0
    out.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    out.extend_from_slice(&0x0046u16.to_be_bytes()); // division
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&track_len.to_be_bytes());
    out.extend_from_slice(&track);
    Ok(out)
}

/// True if `data` starts with a MUS signature.
#[must_use]
pub fn is_mus(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == b"MUS\x1A"
}

fn write_time(track: &mut Vec<u8>, queued: &mut u32) {
    let mut time = *queued;
    let mut buffer = time & 0x7F;
    while {
        time >>= 7;
        time != 0
    } {
        buffer <<= 8;
        buffer |= (time & 0x7F) | 0x80;
    }
    loop {
        let writeval = (buffer & 0xFF) as u8;
        track.push(writeval);
        if buffer & 0x80 != 0 {
            buffer >>= 8;
        } else {
            *queued = 0;
            break;
        }
    }
}

fn write_controller(track: &mut Vec<u8>, queued: &mut u32, channel: u8, control: u8, value: u8) {
    write_time(track, queued);
    track.push(0xB0 | channel);
    track.push(control & 0x7F);
    track.push(value);
}

fn get_midi_channel(
    mus_channel: u8,
    channel_map: &mut [i8; 16],
    track: &mut Vec<u8>,
    queued: &mut u32,
) -> Result<u8, FormatError> {
    if mus_channel == MUS_PERCUSSION_CHAN {
        return Ok(MIDI_PERCUSSION_CHAN);
    }
    let idx = mus_channel as usize;
    if channel_map[idx] == -1 {
        let mut max = -1i8;
        for &c in channel_map.iter() {
            if c > max {
                max = c;
            }
        }
        let mut result = max + 1;
        if result == MIDI_PERCUSSION_CHAN as i8 {
            result += 1;
        }
        channel_map[idx] = result;
        // All notes off — fixes "D_DDTBLU disease".
        write_controller(track, queued, result as u8, 0x7B, 0);
    }
    Ok(channel_map[idx] as u8)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_score_to_midi() {
        // Minimal MUS: header + score that is just score-end with last-flag.
        let mut mus = Vec::new();
        mus.extend_from_slice(b"MUS\x1A");
        mus.extend_from_slice(&1u16.to_le_bytes()); // score length
        mus.extend_from_slice(&16u16.to_le_bytes()); // score start
        mus.extend_from_slice(&0u16.to_le_bytes()); // primary
        mus.extend_from_slice(&0u16.to_le_bytes()); // secondary
        mus.extend_from_slice(&0u16.to_le_bytes()); // instruments
        mus.extend_from_slice(&0u16.to_le_bytes()); // reserved
        // score at offset 16: score end | last
        mus.push(0x60 | 0x80);

        let midi = mus_to_midi(&mus).unwrap();
        assert_eq!(&midi[0..4], b"MThd");
        assert_eq!(&midi[14..18], b"MTrk");
        // Ends with end-of-track meta
        assert!(midi.windows(3).any(|w| w == [0xFF, 0x2F, 0x00]));
    }
}

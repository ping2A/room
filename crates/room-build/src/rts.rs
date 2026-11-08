//! Duke Nukem 3D Remote Ridicule Sound archive (`DUKE.RTS`).
//!
//! RTS is an IWAD-style directory with two empty sentinels surrounding ten
//! VOC/WAV sound lumps. This parser keeps the caller-owned archive borrowed;
//! it never copies untrusted lump ranges.

use crate::error::BuildError;

const HEADER_LEN: usize = 12;
const DIRECTORY_ENTRY_LEN: usize = 16;
const SOUND_COUNT: usize = 10;
const LUMP_COUNT: usize = SOUND_COUNT + 2;

/// Borrowed Atomic Duke Remote Ridicule sound archive.
#[derive(Clone, Copy, Debug)]
pub struct DukeRts<'a> {
    data: &'a [u8],
    sounds: [(usize, usize); SOUND_COUNT],
}

impl<'a> DukeRts<'a> {
    /// Parse the strict Atomic Duke RTS directory.
    ///
    /// Expected layout is `IWAD`, `REMOSTRT`, `RTS1`…`RTS10`, `REMOSTOP`.
    /// The two sentinel lumps are empty; each sound lump must contain bytes.
    ///
    /// # Errors
    ///
    /// Returns a bounded [`BuildError`] for an invalid header, directory,
    /// lump range, sentinel, or sound order.
    pub fn parse(data: &'a [u8]) -> Result<Self, BuildError> {
        if data.len() < HEADER_LEN {
            return Err(BuildError::Truncated("RTS header"));
        }
        if data.get(..4) != Some(b"IWAD") {
            return Err(BuildError::Invalid("RTS signature must be IWAD"));
        }
        let lump_count = read_u32(data, 4)?;
        if lump_count != LUMP_COUNT as u32 {
            return Err(BuildError::Invalid("RTS must contain 12 directory entries"));
        }
        let directory_offset = usize::try_from(read_u32(data, 8)?)
            .map_err(|_| BuildError::Invalid("RTS directory offset"))?;
        let directory_len = LUMP_COUNT
            .checked_mul(DIRECTORY_ENTRY_LEN)
            .ok_or(BuildError::Invalid("RTS directory size"))?;
        let directory_end = directory_offset
            .checked_add(directory_len)
            .ok_or(BuildError::Invalid("RTS directory range"))?;
        if directory_offset < HEADER_LEN || directory_end > data.len() {
            return Err(BuildError::Truncated("RTS directory"));
        }

        let mut sounds = [(0_usize, 0_usize); SOUND_COUNT];
        for index in 0..LUMP_COUNT {
            let entry_start = directory_offset + index * DIRECTORY_ENTRY_LEN;
            let entry = data
                .get(entry_start..entry_start + DIRECTORY_ENTRY_LEN)
                .ok_or(BuildError::Truncated("RTS directory entry"))?;
            let offset = usize::try_from(read_u32(entry, 0)?)
                .map_err(|_| BuildError::Invalid("RTS lump offset"))?;
            let size = usize::try_from(read_u32(entry, 4)?)
                .map_err(|_| BuildError::Invalid("RTS lump size"))?;
            let end = offset
                .checked_add(size)
                .ok_or(BuildError::Invalid("RTS lump range"))?;
            if end > data.len() || (size != 0 && offset < HEADER_LEN) {
                return Err(BuildError::Truncated("RTS lump data"));
            }
            let name = entry
                .get(8..16)
                .ok_or(BuildError::Truncated("RTS lump name"))?;
            let valid_name = match index {
                0 => name_eq(name, b"REMOSTRT"),
                1..=SOUND_COUNT => name_eq(name, &sound_name(index)),
                _ => name_eq(name, b"REMOSTOP"),
            };
            if !valid_name {
                return Err(BuildError::Invalid("RTS lump order or name"));
            }
            if (index == 0 || index == LUMP_COUNT - 1) && size != 0 {
                return Err(BuildError::Invalid("RTS sentinel lumps must be empty"));
            }
            if (1..=SOUND_COUNT).contains(&index) {
                if size == 0 {
                    return Err(BuildError::Invalid("RTS sound lump is empty"));
                }
                sounds[index - 1] = (offset, size);
            }
        }
        Ok(Self { data, sounds })
    }

    /// Number of Alt+F1…Alt+F10 sound entries in this archive.
    #[must_use]
    pub const fn len(&self) -> usize {
        SOUND_COUNT
    }

    /// Whether this archive has no remote ridicule entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// RTS sound bytes by zero-based index.
    #[must_use]
    pub fn sound(&self, index: usize) -> Option<&'a [u8]> {
        let (offset, size) = *self.sounds.get(index)?;
        self.data.get(offset..offset.checked_add(size)?)
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, BuildError> {
    let end = offset
        .checked_add(4)
        .ok_or(BuildError::Invalid("RTS integer range"))?;
    let raw = bytes
        .get(offset..end)
        .ok_or(BuildError::Truncated("RTS integer"))?;
    let array: [u8; 4] = raw
        .try_into()
        .map_err(|_| BuildError::Truncated("RTS integer"))?;
    Ok(u32::from_le_bytes(array))
}

fn name_eq(raw: &[u8], expected: &[u8]) -> bool {
    raw.iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(|byte| byte.to_ascii_uppercase())
        .eq(expected.iter().copied())
}

fn sound_name(index: usize) -> [u8; 8] {
    if index < 10 {
        [b'R', b'T', b'S', b'0' + index as u8, 0, 0, 0, 0]
    } else {
        [b'R', b'T', b'S', b'1', b'0', 0, 0, 0]
    }
}

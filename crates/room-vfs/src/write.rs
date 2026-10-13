//! Writers for tools and tests.

use crate::error::VfsError;
use crate::short_name::ShortName;
use std::io::Write;

/// In-memory IWAD/PWAD builder.
#[derive(Default)]
pub struct WadWriter {
    iwad: bool,
    lumps: Vec<(ShortName, Vec<u8>)>,
}

impl WadWriter {
    /// Start a PWAD.
    #[must_use]
    pub fn pwad() -> Self {
        Self {
            iwad: false,
            lumps: Vec::new(),
        }
    }

    /// Start an IWAD.
    #[must_use]
    pub fn iwad() -> Self {
        Self {
            iwad: true,
            lumps: Vec::new(),
        }
    }

    /// Append a lump (name truncated/uppercased to 8 chars).
    pub fn add(&mut self, name: &str, data: impl Into<Vec<u8>>) -> &mut Self {
        self.lumps.push((ShortName::from_stem(name), data.into()));
        self
    }

    /// Append an empty marker lump.
    pub fn marker(&mut self, name: &str) -> &mut Self {
        self.add(name, Vec::new())
    }

    /// Serialise to WAD bytes.
    pub fn finish(&self) -> Result<Vec<u8>, VfsError> {
        let numlumps =
            i32::try_from(self.lumps.len()).map_err(|_| VfsError::parse("too many lumps"))?;
        let mut data_section = Vec::new();
        let mut directory = Vec::with_capacity(self.lumps.len() * 16);
        for (name, payload) in &self.lumps {
            let filepos = i32::try_from(12 + data_section.len())
                .map_err(|_| VfsError::parse("wad too large"))?;
            let size =
                i32::try_from(payload.len()).map_err(|_| VfsError::parse("lump too large"))?;
            data_section.extend_from_slice(payload);
            directory.extend_from_slice(&filepos.to_le_bytes());
            directory.extend_from_slice(&size.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }
        let infotableofs =
            i32::try_from(12 + data_section.len()).map_err(|_| VfsError::parse("wad too large"))?;
        let mut out = Vec::with_capacity(12 + data_section.len() + directory.len());
        out.write_all(if self.iwad { b"IWAD" } else { b"PWAD" })?;
        out.write_all(&numlumps.to_le_bytes())?;
        out.write_all(&infotableofs.to_le_bytes())?;
        out.extend_from_slice(&data_section);
        out.extend_from_slice(&directory);
        Ok(out)
    }
}

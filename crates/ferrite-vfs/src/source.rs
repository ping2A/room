//! Container byte sources.

use crate::error::VfsError;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Random-access byte source for a mounted container.
pub trait ContainerSource: Send + Sync {
    /// Total length in bytes.
    fn len(&self) -> u64;

    /// True when empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read exactly `buf.len()` bytes starting at `offset`.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), VfsError>;

    /// Read a range into a fresh buffer.
    fn read_range(&self, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        let mut buf = vec![0u8; len];
        self.read_at(offset, &mut buf)?;
        Ok(buf)
    }

    /// Full contents (for MD5 / small archives).
    fn read_all(&self) -> Result<Vec<u8>, VfsError> {
        let len =
            usize::try_from(self.len()).map_err(|_| VfsError::parse("container too large"))?;
        self.read_range(0, len)
    }
}

/// In-memory bytes (embedded `ferrite.pk3`, web, tests).
#[derive(Clone, Debug)]
pub struct MemorySource {
    data: bytes::Bytes,
}

impl MemorySource {
    /// Wrap owned or shared bytes.
    #[must_use]
    pub fn new(data: impl Into<bytes::Bytes>) -> Self {
        Self { data: data.into() }
    }

    /// Borrow the underlying bytes.
    #[must_use]
    pub fn bytes(&self) -> &bytes::Bytes {
        &self.data
    }
}

impl ContainerSource for MemorySource {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), VfsError> {
        let start = usize::try_from(offset).map_err(|_| VfsError::parse("offset out of range"))?;
        let end = start
            .checked_add(buf.len())
            .ok_or_else(|| VfsError::parse("read overflow"))?;
        if end > self.data.len() {
            return Err(VfsError::parse("read past end of memory source"));
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }
}

/// Positioned file reads.
#[derive(Debug)]
pub struct FileSource {
    path: PathBuf,
    file: Mutex<File>,
    len: u64,
}

impl FileSource {
    /// Open `path` for random-access reads.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VfsError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let len = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            path,
            file: Mutex::new(file),
            len,
        })
    }

    /// Path used to open this source.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ContainerSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), VfsError> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| VfsError::parse("file source lock poisoned"))?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)?;
        Ok(())
    }
}

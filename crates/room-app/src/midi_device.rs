//! MIDI backend selection + IWAD `GENMIDI` load for the desktop shell.

use room_music::{GenMidiBank, MidiDevice};
use room_vfs::{Namespace, Vfs};
use std::fmt;

/// CLI / CVar-style MIDI device choice (`snd_mididevice` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum MidiBackend {
    /// SoftFont if available, else OPL/`GENMIDI`, else silent.
    #[default]
    Auto,
    /// Force SoftFont (`rustysynth`).
    #[value(name = "sf2")]
    SoftFont,
    /// Force OPL3 + IWAD `GENMIDI`.
    Opl,
}

impl fmt::Display for MidiBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::SoftFont => write!(f, "sf2"),
            Self::Opl => write!(f, "opl"),
        }
    }
}

/// Read and parse the Doom `GENMIDI` lump from a mounted VFS.
pub fn load_genmidi(vfs: &Vfs) -> Option<GenMidiBank> {
    let id = vfs
        .find("GENMIDI", Namespace::Global)
        .or_else(|| vfs.find_any_ns("GENMIDI"))?;
    let bytes = vfs.read(id).ok()?;
    match GenMidiBank::parse(&bytes) {
        Ok(bank) => Some(bank),
        Err(e) => {
            eprintln!("room: GENMIDI: {e}");
            None
        }
    }
}

/// Resolve SoftFont vs OPL for the music player.
#[must_use]
pub fn resolve_midi_device(
    backend: MidiBackend,
    softfont_ok: bool,
    genmidi_ok: bool,
) -> Option<MidiDevice> {
    match backend {
        MidiBackend::SoftFont => softfont_ok.then_some(MidiDevice::SoftFont),
        MidiBackend::Opl => genmidi_ok.then_some(MidiDevice::Opl),
        MidiBackend::Auto => {
            if softfont_ok {
                Some(MidiDevice::SoftFont)
            } else if genmidi_ok {
                Some(MidiDevice::Opl)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::VfsBuilder;
    use std::path::PathBuf;

    #[test]
    fn resolve_auto_prefers_softfont() {
        assert_eq!(
            resolve_midi_device(MidiBackend::Auto, true, true),
            Some(MidiDevice::SoftFont)
        );
        assert_eq!(
            resolve_midi_device(MidiBackend::Auto, false, true),
            Some(MidiDevice::Opl)
        );
    }

    #[test]
    fn freedoom_genmidi_parses() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !path.is_file() {
            return;
        }
        let vfs = VfsBuilder::new()
            .mount_path(&path)
            .unwrap()
            .build()
            .unwrap();
        let bank = load_genmidi(&vfs).unwrap();
        assert_eq!(bank.instruments.len(), 175);
    }
}

//! Desync detection dumps (`docs/17` §1.2).

use crate::{NetError, PeerId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One peer's checksum (and optional world dump) at a tic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerChecksum {
    /// Reporting peer.
    pub peer: PeerId,
    /// `World::checksum()` (or equivalent).
    pub checksum: u64,
    /// Optional opaque world dump bytes (postcard / text).
    pub dump: Vec<u8>,
}

/// Collected reports for one tic; builds a human-readable desync dump.
#[derive(Clone, Debug, Default)]
pub struct DesyncCollector {
    /// `tic → peer → report`.
    reports: BTreeMap<u32, BTreeMap<PeerId, PeerChecksum>>,
}

impl DesyncCollector {
    /// Empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a peer report for `tic`.
    pub fn report(&mut self, tic: u32, peer: PeerId, checksum: u64, dump: impl Into<Vec<u8>>) {
        self.reports.entry(tic).or_default().insert(
            peer,
            PeerChecksum {
                peer,
                checksum,
                dump: dump.into(),
            },
        );
    }

    /// Check agreement across peers for `tic`.
    ///
    /// # Errors
    ///
    /// [`NetError::Desync`] when checksums disagree.
    pub fn check(&self, tic: u32) -> Result<(), NetError> {
        let Some(peers) = self.reports.get(&tic) else {
            return Ok(());
        };
        let mut iter = peers.values().map(|p| p.checksum);
        let Some(first) = iter.next() else {
            return Ok(());
        };
        for c in iter {
            if c != first {
                return Err(NetError::Desync { tic });
            }
        }
        Ok(())
    }

    /// Format a multi-peer dump for `tic` (empty if no reports).
    #[must_use]
    pub fn format_dump(&self, tic: u32) -> String {
        let Some(peers) = self.reports.get(&tic) else {
            return String::new();
        };
        let mut out = String::new();
        let _ = writeln!(out, "=== room desync dump tic {tic} ===");
        for (peer, rep) in peers {
            let _ = writeln!(out, "peer {} checksum {:016x}", peer.0, rep.checksum);
            if !rep.dump.is_empty() {
                if let Ok(text) = std::str::from_utf8(&rep.dump) {
                    let _ = writeln!(out, "--- dump peer {} ---", peer.0);
                    out.push_str(text);
                    if !text.ends_with('\n') {
                        out.push('\n');
                    }
                } else {
                    let _ = writeln!(
                        out,
                        "--- dump peer {} ({} binary bytes) ---",
                        peer.0,
                        rep.dump.len()
                    );
                }
            }
        }
        out
    }

    /// Check and, on desync, return the formatted dump.
    pub fn check_with_dump(&self, tic: u32) -> Result<(), (NetError, String)> {
        match self.check(tic) {
            Ok(()) => Ok(()),
            Err(e) => Err((e, self.format_dump(tic))),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dump_on_mismatch() {
        let mut c = DesyncCollector::new();
        c.report(10, PeerId(0), 1, b"world_a=1");
        c.report(10, PeerId(1), 2, b"world_b=2");
        let (err, dump) = c.check_with_dump(10).unwrap_err();
        assert_eq!(err, NetError::Desync { tic: 10 });
        assert!(dump.contains("tic 10"));
        assert!(dump.contains("world_a=1"));
        assert!(dump.contains("world_b=2"));
    }
}

//! Legacy FTRL helpers (seat stamping).

use room_net::{PeerId, decode_timed_cmd, encode_timed_cmd};

/// If `payload` is a TimedCmd, rewrite `peer` to `seat`; otherwise return as-is.
#[must_use]
pub fn stamp_timed_cmd_peer(payload: &[u8], seat: u8) -> Vec<u8> {
    match decode_timed_cmd(payload) {
        Ok(mut t) => {
            t.peer = PeerId(seat);
            encode_timed_cmd(&t)
        }
        Err(_) => payload.to_vec(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use room_net::{NetTicCmd, TimedCmd};

    #[test]
    fn stamps_peer_byte() {
        let t = TimedCmd {
            tic: 3,
            peer: PeerId(7),
            cmd: NetTicCmd {
                forward_move: 10,
                ..NetTicCmd::default()
            },
        };
        let stamped = stamp_timed_cmd_peer(&encode_timed_cmd(&t), 1);
        let d = decode_timed_cmd(&stamped).unwrap();
        assert_eq!(d.peer, PeerId(1));
        assert_eq!(d.tic, 3);
        assert_eq!(d.cmd.forward_move, 10);
    }
}

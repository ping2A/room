//! `FTR2` binary framing.

use crate::ProtoError;
use crate::duke::{
    DukeSyncCmdMsg, DukeSyncFrameMsg, decode_duke_sync_cmd, decode_duke_sync_frame,
    encode_duke_sync_cmd, encode_duke_sync_frame,
};
use crate::tic::{
    TicChecksum, TicCmdMsg, TicFrameMsg, encode_tic_checksum, encode_tic_cmd, encode_tic_frame,
    parse_tic_checksum, parse_tic_cmd, parse_tic_frame, parse_tic_frame_wide, tic_frame_is_wide,
    tic_frame_len,
};

/// Protocol v2 magic.
pub const FTR2_MAGIC: &[u8; 4] = b"FTR2";
/// Max client→server binary payload (excluding magic+type), except voice legacy.
pub const MAX_BINARY_PAYLOAD: usize = 512;

/// Client → server tic command.
pub const MSG_TIC_CMD: u8 = 0x10;
/// Server → client combined frame.
pub const MSG_TIC_FRAME: u8 = 0x11;
/// Client → server world checksum.
pub const MSG_CHECKSUM: u8 = 0x12;
/// Ping.
pub const MSG_PING: u8 = 0x13;
/// Pong.
pub const MSG_PONG: u8 = 0x14;
/// Server → spectator: consecutive TIC_FRAMEs (catch-up from tic 0).
pub const MSG_TIC_BATCH: u8 = 0x15;
/// Protocol v3: TIC_FRAME with a 64-bit seat mask (rooms past 8 seats).
pub const MSG_TIC_FRAME_WIDE: u8 = 0x16;
/// Protocol v3: TIC_BATCH of 64-bit-mask frames.
pub const MSG_TIC_BATCH_WIDE: u8 = 0x17;
/// Max v3 TIC_FRAME payload (server → client; 64 seats need 1100 bytes).
pub const MAX_WIDE_FRAME_PAYLOAD: usize = 12 + 64 * crate::tic::NET_TIC_CMD_LEN;
/// Wire protocol version: 3 adds 64-seat frames; games of up to 8 seats send
/// exactly the v2 bytes.
pub const PROTOCOL_VERSION: u32 = 3;
/// Legacy PCM voice (off by default).
pub const MSG_VOICE_LEGACY: u8 = 0x1F;
/// Client → server native Duke input command.
pub const MSG_DUKE_TIC_CMD: u8 = 0x20;
/// Server → client native Duke lockstep frame.
pub const MSG_DUKE_TIC_FRAME: u8 = 0x21;
/// Max TIC_BATCH payload (server → client only).
pub const MAX_TIC_BATCH_PAYLOAD: usize = 65_536;
/// Max frames per TIC_BATCH.
pub const MAX_TIC_BATCH_FRAMES: usize = 256;

/// Decoded binary message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinaryMsg {
    /// TIC_CMD.
    TicCmd(TicCmdMsg),
    /// TIC_FRAME.
    TicFrame(TicFrameMsg),
    /// TIC_BATCH: consecutive frames, oldest first.
    TicBatch(Vec<TicFrameMsg>),
    /// CHECKSUM.
    Checksum(TicChecksum),
    /// Duke TIC_CMD.
    DukeTicCmd(DukeSyncCmdMsg),
    /// Duke TIC_FRAME.
    DukeTicFrame(DukeSyncFrameMsg),
    /// PING / PONG body: nonce + server_time_us.
    Ping {
        /// Client or server nonce.
        nonce: u64,
        /// Server time microseconds (0 on client ping).
        server_time_us: u64,
    },
    /// PONG.
    Pong {
        /// Echoed nonce.
        nonce: u64,
        /// Server time microseconds.
        server_time_us: u64,
    },
    /// VOICE_LEGACY raw payload.
    VoiceLegacy(Vec<u8>),
}

/// Encode a binary WebSocket payload (`FTR2` + type + body).
#[must_use]
pub fn encode_binary(msg: &BinaryMsg) -> Vec<u8> {
    let (ty, body) = match msg {
        BinaryMsg::TicCmd(m) => (MSG_TIC_CMD, encode_tic_cmd(m)),
        BinaryMsg::TicFrame(m) => (
            if tic_frame_is_wide(m) {
                MSG_TIC_FRAME_WIDE
            } else {
                MSG_TIC_FRAME
            },
            encode_tic_frame(m),
        ),
        BinaryMsg::TicBatch(frames) => {
            let mut body = Vec::with_capacity(2 + frames.len() * 40);
            let n = frames.len().min(MAX_TIC_BATCH_FRAMES);
            // One wide frame makes the whole batch v3.
            let wide = frames[..n].iter().any(tic_frame_is_wide);
            body.extend_from_slice(&(n as u16).to_le_bytes());
            for f in &frames[..n] {
                if wide && !tic_frame_is_wide(f) {
                    let mut e = encode_tic_frame(f);
                    let mask = u64::from(e[4]);
                    e.splice(4..5, mask.to_le_bytes());
                    body.extend_from_slice(&e);
                } else {
                    body.extend_from_slice(&encode_tic_frame(f));
                }
            }
            (if wide { MSG_TIC_BATCH_WIDE } else { MSG_TIC_BATCH }, body)
        }
        BinaryMsg::Checksum(c) => (MSG_CHECKSUM, encode_tic_checksum(c)),
        BinaryMsg::DukeTicCmd(m) => (MSG_DUKE_TIC_CMD, encode_duke_sync_cmd(m)),
        BinaryMsg::DukeTicFrame(m) => (MSG_DUKE_TIC_FRAME, encode_duke_sync_frame(m)),
        BinaryMsg::Ping {
            nonce,
            server_time_us,
        }
        | BinaryMsg::Pong {
            nonce,
            server_time_us,
        } => {
            let ty = match msg {
                BinaryMsg::Ping { .. } => MSG_PING,
                _ => MSG_PONG,
            };
            let mut body = Vec::with_capacity(16);
            body.extend_from_slice(&nonce.to_le_bytes());
            body.extend_from_slice(&server_time_us.to_le_bytes());
            return {
                let mut v = Vec::with_capacity(5 + body.len());
                v.extend_from_slice(FTR2_MAGIC);
                v.push(ty);
                v.extend_from_slice(&body);
                v
            };
        }
        BinaryMsg::VoiceLegacy(p) => (MSG_VOICE_LEGACY, p.clone()),
    };
    let mut v = Vec::with_capacity(5 + body.len());
    v.extend_from_slice(FTR2_MAGIC);
    v.push(ty);
    v.extend_from_slice(&body);
    v
}

/// Parse a binary WebSocket payload.
///
/// # Errors
///
/// Bad magic, unknown type, truncated / oversized payload.
pub fn parse_binary(bytes: &[u8]) -> Result<BinaryMsg, ProtoError> {
    if bytes.len() < 5 || &bytes[0..4] != FTR2_MAGIC {
        return Err(ProtoError::BadPacket);
    }
    let ty = bytes[4];
    let rest = &bytes[5..];
    if ty == MSG_TIC_BATCH || ty == MSG_TIC_BATCH_WIDE {
        return parse_tic_batch(rest, ty == MSG_TIC_BATCH_WIDE).map(BinaryMsg::TicBatch);
    }
    if ty == MSG_TIC_FRAME_WIDE {
        if rest.len() > MAX_WIDE_FRAME_PAYLOAD {
            return Err(ProtoError::TooLarge);
        }
        return parse_tic_frame_wide(rest).map(BinaryMsg::TicFrame);
    }
    if ty != MSG_VOICE_LEGACY && rest.len() > MAX_BINARY_PAYLOAD {
        return Err(ProtoError::TooLarge);
    }
    if ty == MSG_VOICE_LEGACY && rest.len() > 4_096 {
        return Err(ProtoError::TooLarge);
    }
    match ty {
        MSG_TIC_CMD => Ok(BinaryMsg::TicCmd(parse_tic_cmd(rest)?)),
        MSG_TIC_FRAME => Ok(BinaryMsg::TicFrame(parse_tic_frame(rest)?)),
        MSG_CHECKSUM => Ok(BinaryMsg::Checksum(parse_tic_checksum(rest)?)),
        MSG_DUKE_TIC_CMD => Ok(BinaryMsg::DukeTicCmd(decode_duke_sync_cmd(rest)?)),
        MSG_DUKE_TIC_FRAME => Ok(BinaryMsg::DukeTicFrame(decode_duke_sync_frame(rest)?)),
        MSG_PING => {
            let (nonce, server_time_us) = parse_ping_body(rest)?;
            Ok(BinaryMsg::Ping {
                nonce,
                server_time_us,
            })
        }
        MSG_PONG => {
            let (nonce, server_time_us) = parse_ping_body(rest)?;
            Ok(BinaryMsg::Pong {
                nonce,
                server_time_us,
            })
        }
        MSG_VOICE_LEGACY => Ok(BinaryMsg::VoiceLegacy(rest.to_vec())),
        _ => Err(ProtoError::UnknownType),
    }
}

fn parse_tic_batch(rest: &[u8], wide: bool) -> Result<Vec<TicFrameMsg>, ProtoError> {
    if rest.len() > MAX_TIC_BATCH_PAYLOAD {
        return Err(ProtoError::TooLarge);
    }
    if rest.len() < 2 {
        return Err(ProtoError::BadPacket);
    }
    let n = usize::from(u16::from_le_bytes([rest[0], rest[1]]));
    if n > MAX_TIC_BATCH_FRAMES {
        return Err(ProtoError::TooLarge);
    }
    let mut frames = Vec::with_capacity(n);
    let mut o = 2;
    for _ in 0..n {
        let len = tic_frame_len(rest.get(o..).ok_or(ProtoError::BadPacket)?, wide)?;
        let body = rest.get(o..o + len).ok_or(ProtoError::BadPacket)?;
        frames.push(if wide {
            parse_tic_frame_wide(body)?
        } else {
            parse_tic_frame(body)?
        });
        o += len;
    }
    if o != rest.len() {
        return Err(ProtoError::BadPacket);
    }
    Ok(frames)
}

fn parse_ping_body(rest: &[u8]) -> Result<(u64, u64), ProtoError> {
    if rest.len() != 16 {
        return Err(ProtoError::BadPacket);
    }
    let nonce = u64::from_le_bytes(rest[0..8].try_into().map_err(|_| ProtoError::BadPacket)?);
    let server_time_us =
        u64::from_le_bytes(rest[8..16].try_into().map_err(|_| ProtoError::BadPacket)?);
    Ok((nonce, server_time_us))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tic::{NetTicCmd, TicCmdMsg};

    #[test]
    fn v3_frames_and_batches_round_trip_and_v2_bytes_stay_the_same() {
        let wide = TicFrameMsg {
            tic: 5,
            seat_mask: (1 << 40) | 1,
            cmds: vec![NetTicCmd::default(); 2],
        };
        let pkt = encode_binary(&BinaryMsg::TicFrame(wide.clone()));
        assert_eq!(pkt[4], MSG_TIC_FRAME_WIDE);
        assert_eq!(parse_binary(&pkt).unwrap(), BinaryMsg::TicFrame(wide.clone()));
        let narrow = TicFrameMsg {
            tic: 6,
            seat_mask: 0b11,
            cmds: vec![NetTicCmd::default(); 2],
        };
        let pkt = encode_binary(&BinaryMsg::TicFrame(narrow.clone()));
        assert_eq!(pkt[4], MSG_TIC_FRAME);
        assert_eq!(pkt.len(), 5 + 5 + 2 * crate::tic::NET_TIC_CMD_LEN);
        // A batch with one wide frame goes v3 for all of them.
        let batch = BinaryMsg::TicBatch(vec![narrow.clone(), wide.clone()]);
        let pkt = encode_binary(&batch);
        assert_eq!(pkt[4], MSG_TIC_BATCH_WIDE);
        assert_eq!(parse_binary(&pkt).unwrap(), batch);
        let full = TicFrameMsg {
            tic: 1,
            seat_mask: u64::MAX,
            cmds: vec![NetTicCmd::default(); 64],
        };
        let pkt = encode_binary(&BinaryMsg::TicFrame(full.clone()));
        assert_eq!(parse_binary(&pkt).unwrap(), BinaryMsg::TicFrame(full));
    }

    #[test]
    fn binary_tic_cmd_roundtrip() {
        let msg = BinaryMsg::TicCmd(TicCmdMsg {
            tic: 7,
            cmd: NetTicCmd {
                yaw: 3,
                ..NetTicCmd::default()
            },
        });
        let enc = encode_binary(&msg);
        assert_eq!(parse_binary(&enc).unwrap(), msg);
    }

    #[test]
    fn tic_batch_roundtrip_and_rejects_garbage() {
        let frames: Vec<TicFrameMsg> = (0..200)
            .map(|tic| TicFrameMsg {
                tic,
                seat_mask: if tic % 2 == 0 { 0b11 } else { 0b1 },
                cmds: if tic % 2 == 0 {
                    vec![NetTicCmd::default(); 2]
                } else {
                    vec![NetTicCmd::default()]
                },
            })
            .collect();
        let msg = BinaryMsg::TicBatch(frames);
        let enc = encode_binary(&msg);
        assert_eq!(parse_binary(&enc).unwrap(), msg);
        assert!(parse_binary(&enc[..enc.len() - 1]).is_err());
        let mut extra = enc.clone();
        extra.push(0);
        assert!(parse_binary(&extra).is_err());
        let mut lying = enc;
        lying[5] = 0xFF;
        lying[6] = 0xFF;
        assert!(parse_binary(&lying).is_err());
    }
}

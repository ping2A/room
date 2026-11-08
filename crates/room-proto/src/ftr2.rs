//! `FTR2` binary framing.

use crate::ProtoError;
use crate::duke::{
    DukeSyncCmdMsg, DukeSyncFrameMsg, decode_duke_sync_cmd, decode_duke_sync_frame,
    encode_duke_sync_cmd, encode_duke_sync_frame,
};
use crate::tic::{
    TicChecksum, TicCmdMsg, TicFrameMsg, encode_tic_checksum, encode_tic_cmd, encode_tic_frame,
    parse_tic_checksum, parse_tic_cmd, parse_tic_frame,
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
        BinaryMsg::TicFrame(m) => (MSG_TIC_FRAME, encode_tic_frame(m)),
        BinaryMsg::TicBatch(frames) => {
            let mut body = Vec::with_capacity(2 + frames.len() * 40);
            let n = frames.len().min(MAX_TIC_BATCH_FRAMES);
            body.extend_from_slice(&(n as u16).to_le_bytes());
            for f in &frames[..n] {
                body.extend_from_slice(&encode_tic_frame(f));
            }
            (MSG_TIC_BATCH, body)
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
    if ty == MSG_TIC_BATCH {
        return parse_tic_batch(rest).map(BinaryMsg::TicBatch);
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

fn parse_tic_batch(rest: &[u8]) -> Result<Vec<TicFrameMsg>, ProtoError> {
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
        let mask = *rest.get(o + 4).ok_or(ProtoError::BadPacket)?;
        let len = 5 + mask.count_ones() as usize * crate::tic::NET_TIC_CMD_LEN;
        let body = rest.get(o..o + len).ok_or(ProtoError::BadPacket)?;
        frames.push(parse_tic_frame(body)?);
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

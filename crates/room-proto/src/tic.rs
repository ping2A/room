//! Tic command and frame codecs (17-byte `NetTicCmd` body).

use crate::ProtoError;

/// Compact per-tic input (matches `room-net::NetTicCmd` layout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetTicCmd {
    /// Button mask.
    pub buttons: u32,
    /// Pitch delta.
    pub pitch: i16,
    /// Yaw delta.
    pub yaw: i16,
    /// Roll delta.
    pub roll: i16,
    /// Forward move.
    pub forward_move: i16,
    /// Side move.
    pub side_move: i16,
    /// Up move.
    pub up_move: i16,
    /// Weapon select (`0` = none).
    pub weapon_select: u8,
}

/// Wire size of [`NetTicCmd`] without tic/peer headers.
pub const NET_TIC_CMD_LEN: usize = 17;

/// Client → server TIC_CMD payload after the FTR2 header type byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicCmdMsg {
    /// Simulation tic.
    pub tic: u32,
    /// Command body (no peer — seat comes from the connection).
    pub cmd: NetTicCmd,
}

/// Server → client TIC_FRAME.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicFrameMsg {
    /// Simulation tic.
    pub tic: u32,
    /// Bit `i` set ⇒ seat `i` has a command in `cmds` (in seat order).
    pub seat_mask: u8,
    /// Commands for set bits, low seat first.
    pub cmds: Vec<NetTicCmd>,
}

/// Client → server CHECKSUM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicChecksum {
    /// Tic being reported.
    pub tic: u32,
    /// World checksum.
    pub checksum: u64,
}

/// Encode a [`NetTicCmd`] body (17 bytes).
#[must_use]
pub fn encode_net_tic_cmd(cmd: &NetTicCmd) -> [u8; NET_TIC_CMD_LEN] {
    let mut v = [0u8; NET_TIC_CMD_LEN];
    v[0..4].copy_from_slice(&cmd.buttons.to_le_bytes());
    v[4..6].copy_from_slice(&cmd.pitch.to_le_bytes());
    v[6..8].copy_from_slice(&cmd.yaw.to_le_bytes());
    v[8..10].copy_from_slice(&cmd.roll.to_le_bytes());
    v[10..12].copy_from_slice(&cmd.forward_move.to_le_bytes());
    v[12..14].copy_from_slice(&cmd.side_move.to_le_bytes());
    v[14..16].copy_from_slice(&cmd.up_move.to_le_bytes());
    v[16] = cmd.weapon_select;
    v
}

/// Decode a [`NetTicCmd`] body.
///
/// # Errors
///
/// [`ProtoError::BadPacket`] when truncated.
pub fn decode_net_tic_cmd(bytes: &[u8]) -> Result<NetTicCmd, ProtoError> {
    if bytes.len() < NET_TIC_CMD_LEN {
        return Err(ProtoError::BadPacket);
    }
    Ok(NetTicCmd {
        buttons: u32::from_le_bytes(bytes[0..4].try_into().map_err(|_| ProtoError::BadPacket)?),
        pitch: i16::from_le_bytes(bytes[4..6].try_into().map_err(|_| ProtoError::BadPacket)?),
        yaw: i16::from_le_bytes(bytes[6..8].try_into().map_err(|_| ProtoError::BadPacket)?),
        roll: i16::from_le_bytes(bytes[8..10].try_into().map_err(|_| ProtoError::BadPacket)?),
        forward_move: i16::from_le_bytes(
            bytes[10..12]
                .try_into()
                .map_err(|_| ProtoError::BadPacket)?,
        ),
        side_move: i16::from_le_bytes(
            bytes[12..14]
                .try_into()
                .map_err(|_| ProtoError::BadPacket)?,
        ),
        up_move: i16::from_le_bytes(
            bytes[14..16]
                .try_into()
                .map_err(|_| ProtoError::BadPacket)?,
        ),
        weapon_select: bytes[16],
    })
}

/// Encode TIC_CMD payload (`u32 tic` + 17-byte cmd). Total 21 bytes.
#[must_use]
pub fn encode_tic_cmd(msg: &TicCmdMsg) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + NET_TIC_CMD_LEN);
    v.extend_from_slice(&msg.tic.to_le_bytes());
    v.extend_from_slice(&encode_net_tic_cmd(&msg.cmd));
    v
}

/// Parse TIC_CMD payload.
///
/// # Errors
///
/// Truncated / wrong length.
pub fn parse_tic_cmd(bytes: &[u8]) -> Result<TicCmdMsg, ProtoError> {
    if bytes.len() != 4 + NET_TIC_CMD_LEN {
        return Err(ProtoError::BadPacket);
    }
    let tic = u32::from_le_bytes(bytes[0..4].try_into().map_err(|_| ProtoError::BadPacket)?);
    let cmd = decode_net_tic_cmd(&bytes[4..])?;
    Ok(TicCmdMsg { tic, cmd })
}

/// Encode TIC_FRAME payload.
#[must_use]
pub fn encode_tic_frame(msg: &TicFrameMsg) -> Vec<u8> {
    let n = msg.seat_mask.count_ones() as usize;
    let mut v = Vec::with_capacity(5 + n * NET_TIC_CMD_LEN);
    v.extend_from_slice(&msg.tic.to_le_bytes());
    v.push(msg.seat_mask);
    for cmd in &msg.cmds {
        v.extend_from_slice(&encode_net_tic_cmd(cmd));
    }
    v
}

/// Parse TIC_FRAME payload.
///
/// # Errors
///
/// Truncated / seat_mask vs cmds length mismatch.
pub fn parse_tic_frame(bytes: &[u8]) -> Result<TicFrameMsg, ProtoError> {
    if bytes.len() < 5 {
        return Err(ProtoError::BadPacket);
    }
    let tic = u32::from_le_bytes(bytes[0..4].try_into().map_err(|_| ProtoError::BadPacket)?);
    let seat_mask = bytes[4];
    let n = seat_mask.count_ones() as usize;
    let need = 5 + n * NET_TIC_CMD_LEN;
    if bytes.len() != need {
        return Err(ProtoError::BadPacket);
    }
    let mut cmds = Vec::with_capacity(n);
    let mut o = 5;
    for _ in 0..n {
        cmds.push(decode_net_tic_cmd(&bytes[o..o + NET_TIC_CMD_LEN])?);
        o += NET_TIC_CMD_LEN;
    }
    Ok(TicFrameMsg {
        tic,
        seat_mask,
        cmds,
    })
}

/// Encode CHECKSUM payload.
#[must_use]
pub fn encode_tic_checksum(c: &TicChecksum) -> Vec<u8> {
    let mut v = Vec::with_capacity(12);
    v.extend_from_slice(&c.tic.to_le_bytes());
    v.extend_from_slice(&c.checksum.to_le_bytes());
    v
}

/// Parse CHECKSUM payload.
///
/// # Errors
///
/// Truncated.
pub fn parse_tic_checksum(bytes: &[u8]) -> Result<TicChecksum, ProtoError> {
    if bytes.len() != 12 {
        return Err(ProtoError::BadPacket);
    }
    Ok(TicChecksum {
        tic: u32::from_le_bytes(bytes[0..4].try_into().map_err(|_| ProtoError::BadPacket)?),
        checksum: u64::from_le_bytes(bytes[4..12].try_into().map_err(|_| ProtoError::BadPacket)?),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn net_tic_cmd_roundtrip_samples() {
        for buttons in [0u32, 1, u32::MAX] {
            for weapon_select in [0u8, 7, 255] {
                let c = NetTicCmd {
                    buttons,
                    pitch: -1,
                    yaw: 2,
                    roll: 0,
                    forward_move: 50,
                    side_move: -50,
                    up_move: 0,
                    weapon_select,
                };
                let enc = encode_net_tic_cmd(&c);
                assert_eq!(decode_net_tic_cmd(&enc).unwrap(), c);
            }
        }
    }

    #[test]
    fn tic_cmd_and_frame_roundtrip() {
        let cmd = TicCmdMsg {
            tic: 42,
            cmd: NetTicCmd {
                forward_move: 50,
                buttons: 1,
                ..NetTicCmd::default()
            },
        };
        let enc = encode_tic_cmd(&cmd);
        assert_eq!(parse_tic_cmd(&enc).unwrap(), cmd);

        let frame = TicFrameMsg {
            tic: 42,
            seat_mask: 0b11,
            cmds: vec![cmd.cmd, NetTicCmd::default()],
        };
        let enc = encode_tic_frame(&frame);
        assert_eq!(parse_tic_frame(&enc).unwrap(), frame);
    }
}

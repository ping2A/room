//! Native Duke Build input packet codec.

use crate::ProtoError;

/// Native Atomic/Chocolate `input` record (`#pragma pack(1)`, ten bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DukeSyncInput {
    /// Angular velocity.
    pub avel: i8,
    /// Horizon delta.
    pub horz: i8,
    /// Forward velocity.
    pub fvel: i16,
    /// Strafe velocity.
    pub svel: i16,
    /// Native Duke action bitfield.
    pub bits: u32,
}

/// Size of a packed native Duke input record.
pub const DUKE_SYNC_INPUT_LEN: usize = 10;

/// Client → server native Duke command with its simulation tic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DukeSyncCmdMsg {
    /// Simulation tic.
    pub tic: u32,
    /// Native Build input record.
    pub cmd: DukeSyncInput,
}

/// Server → client native Duke lockstep frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DukeSyncFrameMsg {
    /// Simulation tic.
    pub tic: u32,
    /// Bit `i` set when seat `i` has a command in `cmds`.
    pub seat_mask: u8,
    /// Commands for set bits, in ascending seat order.
    pub cmds: Vec<DukeSyncInput>,
}

/// Bounded deterministic input barrier for one Duke lockstep tic.
///
/// Seats are delivered in ascending seat order when [`Self::take_frame`]
/// returns a complete frame. The barrier owns only the current tic, so late,
/// duplicate, and out-of-roster commands are rejected instead of silently
/// changing the simulation order.
#[derive(Clone, Debug)]
pub struct DukeLockstepInbox {
    tic: u32,
    seat_mask: u8,
    cmds: [Option<DukeSyncInput>; 8],
}

impl DukeLockstepInbox {
    /// Start an input barrier for `tic` and the connected seats in `seat_mask`.
    ///
    /// # Errors
    ///
    /// Returns [`ProtoError::BadPacket`] for an empty roster.
    pub fn new(tic: u32, seat_mask: u8) -> Result<Self, ProtoError> {
        if seat_mask == 0 {
            return Err(ProtoError::BadPacket);
        }
        Ok(Self {
            tic,
            seat_mask,
            cmds: [None; 8],
        })
    }

    /// Add one seat command to the current tic barrier.
    ///
    /// # Errors
    ///
    /// Returns [`ProtoError::BadPacket`] for a wrong tic, absent seat, or
    /// duplicate command.
    pub fn submit(&mut self, msg: DukeSyncCmdMsg, seat: u8) -> Result<(), ProtoError> {
        if msg.tic != self.tic
            || seat >= 8
            || self.seat_mask & (1 << seat) == 0
            || self.cmds[usize::from(seat)].is_some()
        {
            return Err(ProtoError::BadPacket);
        }
        self.cmds[usize::from(seat)] = Some(msg.cmd);
        Ok(())
    }

    /// Return the complete frame once every connected seat submitted input.
    ///
    /// The returned commands are ordered by seat index, as required by the
    /// seat-mask wire format.
    #[must_use]
    pub fn take_frame(&mut self) -> Option<DukeSyncFrameMsg> {
        let mut cmds = Vec::with_capacity(self.seat_mask.count_ones() as usize);
        for seat in 0..8 {
            if self.seat_mask & (1 << seat) != 0 {
                cmds.push(self.cmds[seat as usize]?);
            }
        }
        Some(DukeSyncFrameMsg {
            tic: self.tic,
            seat_mask: self.seat_mask,
            cmds,
        })
    }
}

/// Encode one native Duke input in little-endian packed layout.
#[must_use]
pub fn encode_duke_sync_input(input: &DukeSyncInput) -> [u8; DUKE_SYNC_INPUT_LEN] {
    let mut out = [0; DUKE_SYNC_INPUT_LEN];
    out[0] = input.avel as u8;
    out[1] = input.horz as u8;
    out[2..4].copy_from_slice(&input.fvel.to_le_bytes());
    out[4..6].copy_from_slice(&input.svel.to_le_bytes());
    out[6..10].copy_from_slice(&input.bits.to_le_bytes());
    out
}

/// Decode one native Duke input record.
///
/// # Errors
///
/// Returns [`ProtoError::BadPacket`] unless `bytes` is exactly ten bytes.
pub fn decode_duke_sync_input(bytes: &[u8]) -> Result<DukeSyncInput, ProtoError> {
    if bytes.len() != DUKE_SYNC_INPUT_LEN {
        return Err(ProtoError::BadPacket);
    }
    Ok(DukeSyncInput {
        avel: bytes[0] as i8,
        horz: bytes[1] as i8,
        fvel: i16::from_le_bytes([bytes[2], bytes[3]]),
        svel: i16::from_le_bytes([bytes[4], bytes[5]]),
        bits: u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]),
    })
}

/// Encode a Duke TIC_CMD body (`u32 tic` + ten-byte input).
#[must_use]
pub fn encode_duke_sync_cmd(msg: &DukeSyncCmdMsg) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + DUKE_SYNC_INPUT_LEN);
    out.extend_from_slice(&msg.tic.to_le_bytes());
    out.extend_from_slice(&encode_duke_sync_input(&msg.cmd));
    out
}

/// Decode a Duke TIC_CMD body.
pub fn decode_duke_sync_cmd(bytes: &[u8]) -> Result<DukeSyncCmdMsg, ProtoError> {
    if bytes.len() != 4 + DUKE_SYNC_INPUT_LEN {
        return Err(ProtoError::BadPacket);
    }
    Ok(DukeSyncCmdMsg {
        tic: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| ProtoError::BadPacket)?),
        cmd: decode_duke_sync_input(&bytes[4..])?,
    })
}

/// Encode a Duke TIC_FRAME body (`u32 tic`, `u8 seat_mask`, packed commands).
#[must_use]
pub fn encode_duke_sync_frame(msg: &DukeSyncFrameMsg) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + msg.cmds.len().min(8) * DUKE_SYNC_INPUT_LEN);
    out.extend_from_slice(&msg.tic.to_le_bytes());
    out.push(msg.seat_mask);
    let count = msg.seat_mask.count_ones() as usize;
    for cmd in msg.cmds.iter().take(count.min(8)) {
        out.extend_from_slice(&encode_duke_sync_input(cmd));
    }
    out
}

/// Decode a Duke TIC_FRAME body, rejecting a mask/count mismatch.
pub fn decode_duke_sync_frame(bytes: &[u8]) -> Result<DukeSyncFrameMsg, ProtoError> {
    if bytes.len() < 5 {
        return Err(ProtoError::BadPacket);
    }
    let seat_mask = bytes[4];
    let count = seat_mask.count_ones() as usize;
    let expected = 5 + count * DUKE_SYNC_INPUT_LEN;
    if bytes.len() != expected {
        return Err(ProtoError::BadPacket);
    }
    let mut cmds = Vec::with_capacity(count);
    for chunk in bytes[5..].chunks_exact(DUKE_SYNC_INPUT_LEN) {
        cmds.push(decode_duke_sync_input(chunk)?);
    }
    Ok(DukeSyncFrameMsg {
        tic: u32::from_le_bytes(bytes[..4].try_into().map_err(|_| ProtoError::BadPacket)?),
        seat_mask,
        cmds,
    })
}

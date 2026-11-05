//! Protocol codecs for `room-server` (`docs/22-room-server.md` §4).
//!
//! Binary frames use the `FTR2` magic; JSON control messages use `{"t":…}`.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![forbid(unsafe_code)]

mod control;
mod error;
mod ftr2;
mod tic;

pub use control::{
    ControlMsg, HelloMsg, IceServerEntry, KNOWN_PACKS, MediaInfo, RejectMsg, ResultRow,
    RoomContentRef, RoomModRef, RosterSeat, SeatState, WelcomeMsg, WelcomeRoom,
    content_fingerprint, encode_control, is_known_pack, is_valid_content_file, is_valid_mod_name,
    parse_control, parse_control_bytes,
};
pub use error::ProtoError;
pub use ftr2::{
    BinaryMsg, FTR2_MAGIC, MAX_BINARY_PAYLOAD, MAX_TIC_BATCH_FRAMES, MAX_TIC_BATCH_PAYLOAD,
    MSG_CHECKSUM, MSG_PING, MSG_PONG, MSG_TIC_BATCH, MSG_TIC_CMD, MSG_TIC_FRAME, MSG_VOICE_LEGACY,
    encode_binary, parse_binary,
};
pub use tic::{
    NetTicCmd, TicChecksum, TicCmdMsg, TicFrameMsg, decode_net_tic_cmd, encode_net_tic_cmd,
    encode_tic_cmd, encode_tic_frame, parse_tic_checksum, parse_tic_cmd, parse_tic_frame,
};

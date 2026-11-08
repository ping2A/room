//! Native Duke input recording and replay (`.fdem`).
//!
//! This format stores native `DukeInput` values and a state digest after every
//! tic. It is independent from Doom's `TicCmd` and the original Atomic `.DMO`
//! wire format.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::player::DukeInput;
use crate::session::DukeSession;

fn demo_aim_modes(session: &DukeSession, players: usize) -> Vec<u8> {
    (0..players)
        .map(|seat| {
            session
                .player_for_seat(seat)
                .map_or(0, |player| player.aim_mode)
        })
        .collect()
}

fn demo_weapon_choices(session: &DukeSession, players: usize) -> Vec<[u8; 10]> {
    (0..players)
        .map(|seat| {
            session
                .player_for_seat(seat)
                .map_or([3, 4, 5, 7, 8, 6, 0, 2, 9, 1], |player| {
                    player.weapon_choices
                })
        })
        .collect()
}

/// Recorder for original Atomic Edition 1.5 `.DMO` input streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DukeAtomicDemoRecorder {
    volume: u8,
    level: u8,
    skill: u8,
    coop: u8,
    friendly_fire: u8,
    aim_modes: Vec<u8>,
    weapon_choices: Vec<[u8; 10]>,
    monsters_off: bool,
    player_inputs: Vec<Vec<room_build::DmoInput>>,
}

impl DukeAtomicDemoRecorder {
    /// Start recording using the current native session's skill and monster
    /// setting. `volume` and `level` are Atomic's zero-based map indices.
    #[must_use]
    pub fn new(session: &DukeSession, volume: u8, level: u8, skill: u8) -> Self {
        Self {
            volume,
            level,
            skill,
            coop: session.player.game_mode,
            friendly_fire: u8::from(session.player.friendly_fire),
            aim_modes: demo_aim_modes(session, 1),
            weapon_choices: demo_weapon_choices(session, 1),
            monsters_off: session.monsters_off,
            player_inputs: vec![Vec::new()],
        }
    }

    /// Start an Atomic recorder for 1–8 player streams.
    pub fn new_multiplayer(
        session: &DukeSession,
        volume: u8,
        level: u8,
        skill: u8,
        players: usize,
    ) -> Result<Self, DukeDemoError> {
        if !(1..=8).contains(&players) {
            return Err(DukeDemoError::Invalid(
                "Atomic demo player count must be 1..=8".into(),
            ));
        }
        let player_inputs = (0..players).map(|_| Vec::new()).collect();
        if players != session.player_count() {
            return Err(DukeDemoError::Invalid(
                "Atomic demo seat count must match the active Duke roster".into(),
            ));
        }
        Ok(Self {
            volume,
            level,
            skill,
            coop: session.player.game_mode,
            friendly_fire: u8::from(session.player.friendly_fire),
            aim_modes: demo_aim_modes(session, players),
            weapon_choices: demo_weapon_choices(session, players),
            monsters_off: session.monsters_off,
            player_inputs,
        })
    }

    /// Append the seat-ordered inputs for one tic. The count must match the
    /// recorder roster so Atomic's interleaved stream stays rectangular.
    pub fn record_frame(&mut self, inputs: &[DukeInput]) -> Result<(), DukeDemoError> {
        if inputs.len() != self.player_inputs.len() {
            return Err(DukeDemoError::Invalid(format!(
                "Atomic demo expected {} seats, got {}",
                self.player_inputs.len(),
                inputs.len()
            )));
        }
        for (stream, input) in self.player_inputs.iter_mut().zip(inputs) {
            stream.push(room_build::DmoInput {
                turn: input.avel,
                horizon: input.horz,
                forward: input.fvel,
                strafe: input.svel,
                bits: input.bits,
            });
        }
        Ok(())
    }

    /// Append a one-player tic. Use [`Self::record_frame`] for multiplayer.
    pub fn record_tic(&mut self, input: DukeInput) -> Result<(), DukeDemoError> {
        self.record_tic_with_weapon_actions(input, None, 0)
    }

    /// Append a desktop tic, encoding its out-of-band weapon controls into
    /// Chocolate's Atomic `sync.bits` selector before writing the `.DMO`.
    pub fn record_tic_with_weapon_actions(
        &mut self,
        mut input: DukeInput,
        weapon_request: Option<i16>,
        weapon_cycle: i8,
    ) -> Result<(), DukeDemoError> {
        if !(-1..=1).contains(&weapon_cycle) {
            return Err(DukeDemoError::Invalid("invalid Atomic weapon cycle".into()));
        }
        let selector = match weapon_request {
            Some(weapon @ 0..=9) => u32::try_from(weapon + 1).unwrap_or(0),
            Some(_) => 0,
            None if weapon_cycle < 0 => 11,
            None if weapon_cycle > 0 => 12,
            None => 0,
        };
        if selector != 0 {
            input.bits = (input.bits & !crate::weapons::INPUT_WEAPON_MASK)
                | (selector << crate::weapons::INPUT_WEAPON_SHIFT);
        }
        self.record_frame(std::slice::from_ref(&input))
    }

    /// Advance a one-player session and capture that input atomically.
    pub fn tick_session(
        &mut self,
        session: &mut DukeSession,
        input: DukeInput,
    ) -> Result<(), DukeDemoError> {
        if self.player_inputs.len() != 1 {
            return Err(DukeDemoError::Invalid(
                "tick_session requires a one-seat recorder".into(),
            ));
        }
        session.tick(input);
        self.record_tic(input)
    }

    /// Advance all configured seats and record the exact seat-ordered frame.
    pub fn tick_session_inputs(
        &mut self,
        session: &mut DukeSession,
        inputs: &[DukeInput],
    ) -> Result<(), DukeDemoError> {
        if inputs.len() != self.player_inputs.len() || inputs.len() != session.player_count() {
            return Err(DukeDemoError::Invalid(
                "Atomic demo streams must match the active Duke roster".into(),
            ));
        }
        session.tick_inputs(inputs);
        self.record_frame(inputs)
    }

    /// Number of tics captured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.player_inputs.first().map_or(0, Vec::len)
    }

    /// Whether no tics have been captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of recorded player streams.
    #[must_use]
    pub fn players(&self) -> usize {
        self.player_inputs.len()
    }

    /// Encode as vanilla Atomic revision 117. The produced stream is suitable
    /// for Chocolate Duke's demo loader; reference playback has not been run.
    pub fn to_dmo(&self) -> Result<Vec<u8>, DukeDemoError> {
        room_build::DmoDemo {
            volume: self.volume,
            level: self.level,
            skill: self.skill,
            coop: self.coop,
            friendly_fire: self.friendly_fire,
            players: self.player_inputs.len() as u8,
            tics: self.player_inputs.first().cloned().unwrap_or_default(),
            player_tics: self.player_inputs.clone(),
            aim_modes: self.aim_modes.clone(),
            weapon_choices: self.weapon_choices.clone(),
        }
        .encode_atomic_15(self.monsters_off)
        .map_err(|error| DukeDemoError::Invalid(error.to_string()))
    }
}

/// Seat-ordered player for the inputs in an Atomic `.DMO` stream.
///
/// The caller must load the map identified by [`Self::map_name`] with the
/// saved skill/session settings before advancing the first demo tic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DukeAtomicDemoPlayer {
    volume: u8,
    level: u8,
    skill: u8,
    coop: u8,
    friendly_fire: u8,
    aim_modes: Vec<u8>,
    weapon_choices: Vec<[u8; 10]>,
    player_inputs: Vec<Vec<room_build::DmoInput>>,
    cursor: usize,
    settings_applied: bool,
}

impl DukeAtomicDemoPlayer {
    /// Parse an Atomic 1.3D/1.5 `.DMO` into deterministic seat streams.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DukeDemoError> {
        let demo = room_build::DmoDemo::parse(bytes)
            .map_err(|error| DukeDemoError::Invalid(format!("Atomic DMO: {error}")))?;
        let player_inputs = if demo.player_tics.len() == usize::from(demo.players) {
            demo.player_tics
        } else if demo.players == 1 && demo.player_tics.is_empty() {
            vec![demo.tics]
        } else {
            return Err(DukeDemoError::Invalid(
                "Atomic DMO does not contain one stream per seat".into(),
            ));
        };
        let tic_count = player_inputs.first().map_or(0, Vec::len);
        if player_inputs.iter().any(|stream| stream.len() != tic_count) {
            return Err(DukeDemoError::Invalid(
                "Atomic DMO seat streams have different lengths".into(),
            ));
        }
        Ok(Self {
            volume: demo.volume,
            level: demo.level,
            skill: demo.skill,
            coop: demo.coop,
            friendly_fire: demo.friendly_fire,
            aim_modes: demo.aim_modes,
            weapon_choices: demo.weapon_choices,
            player_inputs,
            cursor: 0,
            settings_applied: false,
        })
    }

    /// Atomic zero-based volume index.
    #[must_use]
    pub const fn volume(&self) -> u8 {
        self.volume
    }

    /// Atomic zero-based level index.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// Skill that must be used while loading the map.
    #[must_use]
    pub const fn skill(&self) -> u8 {
        self.skill
    }

    /// Atomic game mode (`0` DukeMatch, `1` co-op; `2` is the legacy variant).
    #[must_use]
    pub const fn coop(&self) -> u8 {
        self.coop
    }

    /// Atomic friendly-fire setting.
    #[must_use]
    pub const fn friendly_fire(&self) -> u8 {
        self.friendly_fire
    }

    /// Per-seat Atomic `player_struct::aim_mode` values, when present.
    #[must_use]
    pub fn aim_modes(&self) -> &[u8] {
        &self.aim_modes
    }

    /// Per-seat Atomic `ud.wchoice` preference tables, when present.
    #[must_use]
    pub fn weapon_choices(&self) -> &[[u8; 10]] {
        &self.weapon_choices
    }

    /// Load the Atomic map and native multiplayer roster named by this demo.
    /// Ordered GRP layers use later archives as overrides, matching the game
    /// loader. The returned session is configured before tic zero.
    pub fn load_session(&self, grp_layers: &[&[u8]]) -> Result<DukeSession, DukeDemoError> {
        self.load_session_with_con(grp_layers, None, None)
    }

    /// Load the Atomic map with the same root CON override and include resolver
    /// used by a native campaign session.
    pub fn load_session_with_con(
        &self,
        grp_layers: &[&[u8]],
        con_override: Option<&[u8]>,
        additional_includes: Option<&mut dyn crate::con::IncludeResolver>,
    ) -> Result<DukeSession, DukeDemoError> {
        let mut session = crate::demo_sync::session_from_grp_layers_with_con_and_includes(
            grp_layers,
            &self.map_name(),
            self.skill,
            con_override,
            additional_includes,
        )
        .map_err(|error| DukeDemoError::Invalid(format!("Atomic DMO session: {error}")))?;
        session
            .configure_multiplayer_mode(self.players(), self.coop, self.friendly_fire != 0)
            .map_err(|error| DukeDemoError::Invalid(format!("Atomic DMO roster: {error}")))?;
        apply_demo_player_settings(
            &mut session,
            &self.aim_modes,
            &self.weapon_choices,
            self.coop,
            self.friendly_fire,
        )?;
        Ok(session)
    }

    /// Atomic map lump name, such as `E1L1`.
    #[must_use]
    pub fn map_name(&self) -> String {
        format!("E{}L{}", self.volume + 1, self.level + 1)
    }

    /// Number of seats encoded in the stream.
    #[must_use]
    pub fn players(&self) -> usize {
        self.player_inputs.len()
    }

    /// Number of recorded tics per seat.
    #[must_use]
    pub fn tic_count(&self) -> usize {
        self.player_inputs.first().map_or(0, Vec::len)
    }

    /// Current zero-based tic cursor.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Apply the next exact input frame to a preloaded matching native roster.
    pub fn playback_next(&mut self, session: &mut DukeSession) -> Result<(), DukeDemoError> {
        if self.cursor >= self.tic_count() {
            return Err(DukeDemoError::EndOfDemo);
        }
        if session.player_count() != self.players() {
            return Err(DukeDemoError::Invalid(format!(
                "Atomic DMO has {} seats, session has {}",
                self.players(),
                session.player_count()
            )));
        }
        if !self.settings_applied {
            apply_demo_player_settings(
                session,
                &self.aim_modes,
                &self.weapon_choices,
                self.coop,
                self.friendly_fire,
            )?;
            self.settings_applied = true;
        }
        let inputs: Vec<DukeInput> = self
            .player_inputs
            .iter()
            .map(|stream| {
                let input = stream[self.cursor];
                DukeInput {
                    avel: input.turn,
                    horz: input.horizon,
                    fvel: input.forward,
                    svel: input.strafe,
                    bits: input.bits,
                }
            })
            .collect();
        session.tick_inputs(&inputs);
        self.cursor += 1;
        Ok(())
    }
}

fn apply_demo_player_settings(
    session: &mut DukeSession,
    aim_modes: &[u8],
    weapon_choices: &[[u8; 10]],
    coop: u8,
    friendly_fire: u8,
) -> Result<(), DukeDemoError> {
    if coop > 2 || friendly_fire > 1 {
        return Err(DukeDemoError::Invalid(
            "Atomic DMO has invalid game-mode settings".into(),
        ));
    }
    for seat in 0..session.player_count() {
        let Some(player) = session.player_for_seat_mut(seat) else {
            return Err(DukeDemoError::Invalid(
                "Atomic DMO seat is missing from the native roster".into(),
            ));
        };
        player.coop_mode = coop == 1;
        player.game_mode = coop;
        player.friendly_fire = friendly_fire != 0;
        if let Some(&aim_mode) = aim_modes.get(seat) {
            player.aim_mode = aim_mode & 1;
        }
        if let Some(&choices) = weapon_choices.get(seat) {
            player.weapon_choices = choices;
        }
    }
    Ok(())
}

const MAGIC: &[u8; 8] = b"DUKEFDEM";
const VERSION: u16 = 1;
const MAX_FDEM_BYTES: usize = 64 * 1024 * 1024;

/// One recorded player input and the resulting native state digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DukeDemoTic {
    /// Original Build `sync` input for this tic.
    pub input: DukeInput,
    /// Desktop direct weapon request applied immediately before this tic.
    #[serde(default)]
    pub weapon_request: Option<i16>,
    /// Desktop weapon cycle applied immediately before this tic (-1 / 0 / 1).
    #[serde(default)]
    pub weapon_cycle: i8,
    /// SHA-256 of the resulting Room Duke trace snapshot.
    pub state_sha256: [u8; 32],
}

/// Native Duke `.fdem` recording. Inputs remain in Duke's native bit layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DukeDemoRecorder {
    /// Map volume (0-based).
    pub volume: u8,
    /// Map level (0-based).
    pub level: u8,
    /// Skill 0..=4.
    pub skill: u8,
    /// Build RNG seed at recording start.
    pub randomseed: i32,
    /// Native save snapshot captured before the first recorded tic.
    #[serde(default)]
    initial_fsave: Vec<u8>,
    /// Recorded inputs and post-tic state digests.
    pub tics: Vec<DukeDemoTic>,
    #[serde(skip)]
    playback_cursor: usize,
}

/// Invalid, incompatible, or divergent native Duke demo data.
#[derive(Debug, thiserror::Error)]
pub enum DukeDemoError {
    /// A malformed header, digest, or JSON body.
    #[error("invalid Duke fdem: {0}")]
    Invalid(String),
    /// The file exceeds the bounded decoder size.
    #[error("Duke fdem exceeds 64 MiB")]
    TooLarge,
    /// Replay's observed deterministic state differs from the recording.
    #[error("Duke fdem state mismatch at tic {tic}")]
    StateMismatch { tic: usize },
    /// Replay completed.
    #[error("Duke fdem playback is at end of recording")]
    EndOfDemo,
}

impl DukeDemoRecorder {
    /// Start a recording at the current native Build RNG state.
    #[must_use]
    pub fn new(
        session: &DukeSession,
        volume: u8,
        level: u8,
        skill: u8,
    ) -> Result<Self, DukeDemoError> {
        let initial_fsave = session
            .save_fsave()
            .map_err(|error| DukeDemoError::Invalid(format!("initial save: {error}")))?;
        Ok(Self {
            volume,
            level,
            skill,
            randomseed: session.randomseed,
            initial_fsave,
            tics: Vec::new(),
            playback_cursor: 0,
        })
    }

    /// Append input after its corresponding `session.tick(input)` call.
    pub fn record_tic(
        &mut self,
        session: &DukeSession,
        input: DukeInput,
    ) -> Result<(), DukeDemoError> {
        self.record_tic_with_weapon_actions(session, input, None, 0)
    }

    /// Append input plus the shell's pre-tic weapon-selection actions.
    pub fn record_tic_with_weapon_actions(
        &mut self,
        session: &DukeSession,
        input: DukeInput,
        weapon_request: Option<i16>,
        weapon_cycle: i8,
    ) -> Result<(), DukeDemoError> {
        if weapon_request.is_some_and(|weapon| !(0..12).contains(&weapon))
            || !(-1..=1).contains(&weapon_cycle)
        {
            return Err(DukeDemoError::Invalid("invalid weapon action".into()));
        }
        let snapshot = session.emit_trace_tic();
        let encoded =
            serde_json::to_vec(&snapshot).map_err(|e| DukeDemoError::Invalid(e.to_string()))?;
        self.tics.push(DukeDemoTic {
            input,
            weapon_request,
            weapon_cycle,
            state_sha256: Sha256::digest(encoded).into(),
        });
        Ok(())
    }

    /// Encode the versioned Room Duke `.fdem` envelope.
    pub fn to_fdem(&self) -> Result<Vec<u8>, DukeDemoError> {
        validate_metadata(self.volume, self.level, self.skill, self.tics.len())?;
        let payload =
            serde_json::to_vec(self).map_err(|e| DukeDemoError::Invalid(e.to_string()))?;
        let total = 8usize
            .checked_add(2)
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_add(payload.len()))
            .and_then(|n| n.checked_add(32))
            .ok_or(DukeDemoError::TooLarge)?;
        if total > MAX_FDEM_BYTES {
            return Err(DukeDemoError::TooLarge);
        }
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&Sha256::digest(&payload));
        Ok(out)
    }

    /// Decode a versioned Room Duke `.fdem` envelope.
    pub fn from_fdem(bytes: &[u8]) -> Result<Self, DukeDemoError> {
        if bytes.len() > MAX_FDEM_BYTES {
            return Err(DukeDemoError::TooLarge);
        }
        if bytes.len() < 46 || &bytes[..8] != MAGIC {
            return Err(DukeDemoError::Invalid("bad or truncated header".into()));
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != VERSION {
            return Err(DukeDemoError::Invalid(format!(
                "unsupported version {version}"
            )));
        }
        let payload_len = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
        let expected_len = 14usize
            .checked_add(payload_len)
            .and_then(|n| n.checked_add(32))
            .ok_or(DukeDemoError::TooLarge)?;
        if expected_len != bytes.len() {
            return Err(DukeDemoError::Invalid("payload length mismatch".into()));
        }
        let payload = &bytes[14..14 + payload_len];
        let digest = Sha256::digest(payload);
        if digest.as_slice() != &bytes[14 + payload_len..] {
            return Err(DukeDemoError::Invalid("payload digest mismatch".into()));
        }
        let mut demo: Self =
            serde_json::from_slice(payload).map_err(|e| DukeDemoError::Invalid(e.to_string()))?;
        validate_metadata(demo.volume, demo.level, demo.skill, demo.tics.len())?;
        demo.playback_cursor = 0;
        Ok(demo)
    }

    /// Tick the session with the next recorded input and verify its state.
    pub fn playback_next(&mut self, session: &mut DukeSession) -> Result<(), DukeDemoError> {
        let index = self.playback_cursor;
        let tic = self.tics.get(index).ok_or(DukeDemoError::EndOfDemo)?;
        let input = tic.input;
        let weapon_request = tic.weapon_request;
        let weapon_cycle = tic.weapon_cycle;
        let expected = tic.state_sha256;
        if index == 0 && !self.initial_fsave.is_empty() {
            session
                .load_fsave(&self.initial_fsave)
                .map_err(|error| DukeDemoError::Invalid(format!("initial save: {error}")))?;
        }
        if let Some(weapon) = weapon_request {
            crate::weapons::select_weapon(&mut session.player, weapon);
        }
        if weapon_cycle != 0 {
            crate::weapons::cycle_weapon(&mut session.player, i32::from(weapon_cycle));
        }
        session.tick(input);
        let actual = trace_digest(session)?;
        if actual != expected {
            return Err(DukeDemoError::StateMismatch { tic: index });
        }
        self.playback_cursor += 1;
        Ok(())
    }

    /// Number of recorded inputs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tics.len()
    }

    /// Whether the recording contains no inputs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tics.is_empty()
    }
}

fn validate_metadata(
    volume: u8,
    level: u8,
    skill: u8,
    tic_count: usize,
) -> Result<(), DukeDemoError> {
    if volume > 3 || level > 15 || skill > 4 || tic_count > u32::MAX as usize {
        return Err(DukeDemoError::Invalid(
            "metadata or tic count out of range".into(),
        ));
    }
    Ok(())
}

fn trace_digest(session: &DukeSession) -> Result<[u8; 32], DukeDemoError> {
    let encoded = serde_json::to_vec(&session.emit_trace_tic())
        .map_err(|e| DukeDemoError::Invalid(e.to_string()))?;
    Ok(Sha256::digest(encoded).into())
}

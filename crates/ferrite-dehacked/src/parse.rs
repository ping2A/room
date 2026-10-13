//! Line-oriented DeHackEd / BEX parser (M5 subset).

use crate::error::DehackedError;
use std::collections::BTreeMap;

/// Parsed patch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DehackedPatch {
    /// Doom version from header (`19` = Doom 1.9).
    pub doom_version: Option<i32>,
    /// Thing blocks.
    pub things: Vec<ThingPatch>,
    /// `Frame N` blocks (state edits; apply needs `states.txt` mapping).
    pub frames: Vec<FramePatch>,
    /// `Pointer N (Frame M)` blocks (code pointer remaps).
    pub pointers: Vec<PointerPatch>,
    /// `Weapon N` blocks.
    pub weapons: Vec<WeaponPatch>,
    /// `Ammo N` blocks.
    pub ammos: Vec<AmmoPatch>,
    /// Misc block.
    pub misc: Option<MiscPatch>,
    /// BEX `[STRINGS]` mnemonic → value.
    pub strings: BTreeMap<String, String>,
    /// BEX `[CODEPTR]` `FRAME n` → action mnemonic (`Chase`, `FirePistol`, …).
    pub codeptrs: BTreeMap<i32, String>,
    /// `Sound N` blocks (vanilla string-table patches; BEX `[SOUNDS]` preferred).
    pub sounds: Vec<SoundPatch>,
    /// `Sprite N` blocks.
    pub sprites: Vec<SpritePatch>,
    /// BEX `[SOUNDS]` original logical name / mnemonic → replacement.
    pub bex_sounds: BTreeMap<String, String>,
    /// BEX `[SPRITES]` original 4-char → replacement 4-char.
    pub bex_sprites: BTreeMap<String, String>,
}

/// One `Sound N` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundPatch {
    /// DeHackEd sound index (`0` = none).
    pub index: i32,
    /// Offset into the vanilla sound-name string table (rarely used).
    pub offset: Option<i32>,
    /// Zero / Value fields from vanilla DeHackEd (ignored by GZDoom apply).
    pub zero: Option<i32>,
    /// Value field.
    pub value: Option<i32>,
}

/// One `Sprite N` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpritePatch {
    /// Vanilla sprite table index.
    pub index: i32,
    /// Offset into the vanilla sprite-name string table.
    pub offset: Option<i32>,
}

/// One `Thing N` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThingPatch {
    /// 1-based DeHackEd thing index.
    pub index: i32,
    /// Optional name from `Thing N (Name)`.
    pub name: Option<String>,
    /// Hit points.
    pub hit_points: Option<i32>,
    /// Width (radius × 2 in map units; DeHackEd stores radius×65536 for fixed — we take map units if < 65536).
    pub width: Option<i32>,
    /// Height.
    pub height: Option<i32>,
    /// Speed.
    pub speed: Option<i32>,
    /// Mass.
    pub mass: Option<i32>,
    /// Reaction time.
    pub reaction_time: Option<i32>,
    /// Pain chance.
    pub pain_chance: Option<i32>,
    /// Missile damage.
    pub damage: Option<i32>,
    /// Bits (raw integer if present).
    pub bits: Option<u32>,
    /// MBF21 Bits (raw integer / mnemonic OR).
    pub mbf21_bits: Option<u32>,
    /// ID # (doomednum).
    pub doomednum: Option<i32>,
    /// Alert sound (DeHackEd sound index).
    pub alert_sound: Option<i32>,
    /// Attack sound.
    pub attack_sound: Option<i32>,
    /// Pain sound.
    pub pain_sound: Option<i32>,
    /// Death sound.
    pub death_sound: Option<i32>,
    /// Action / active sound.
    pub action_sound: Option<i32>,
    /// MBF21 `Fast speed` (Nightmare / `-fast`).
    pub fast_speed: Option<i32>,
    /// MBF21 `Melee range`.
    pub melee_range: Option<i32>,
    /// MBF21 infighting group id.
    pub infighting_group: Option<i32>,
    /// MBF21 projectile group id.
    pub projectile_group: Option<i32>,
    /// MBF21 splash group id.
    pub splash_group: Option<i32>,
}

/// One `Pointer N (Frame M)` block — copy code pointer onto frame M.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointerPatch {
    /// Pointer table index.
    pub index: i32,
    /// Target frame from header (`Pointer N (Frame M)`).
    pub frame: i32,
    /// Source frame whose code pointer is copied (`Codep Frame`).
    pub codep_frame: Option<i32>,
}

/// One `Frame N` block (vanilla `states[]` index).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FramePatch {
    /// Frame index.
    pub index: i32,
    /// Sprite number (vanilla sprite table index).
    pub sprite_number: Option<i32>,
    /// Sprite subnumber (frame letter; bit `0x8000` = fullbright).
    pub sprite_subnumber: Option<i32>,
    /// Duration in tics (`-1` forever).
    pub duration: Option<i32>,
    /// Next frame index.
    pub next_frame: Option<i32>,
    /// `Unknown 1` / misc1.
    pub unknown1: Option<i32>,
    /// `Unknown 2` / misc2.
    pub unknown2: Option<i32>,
    /// Code pointer source frame (`Codep Frame`).
    pub codep_frame: Option<i32>,
    /// MBF21 frame bits (`SKILL5FAST` = 1).
    pub mbf21_bits: Option<u32>,
    /// MBF21 Args1..Args8.
    pub args: [Option<i32>; 8],
}

/// One `Weapon N` block (0 Fist … 8 SuperShotgun).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponPatch {
    /// Weapon index.
    pub index: i32,
    /// Ammo type index (`0` Clip … `3` Rocket; `5` = none).
    pub ammo_type: Option<i32>,
    /// Deselect / lower frame.
    pub deselect_frame: Option<i32>,
    /// Select / raise frame.
    pub select_frame: Option<i32>,
    /// Bobbing / ready frame.
    pub bobbing_frame: Option<i32>,
    /// Shooting / fire frame.
    pub shooting_frame: Option<i32>,
    /// Firing / flash frame.
    pub firing_frame: Option<i32>,
    /// MBF21 ammo per shot.
    pub ammo_per_shot: Option<i32>,
}

/// One `Ammo N` block (0 Clip, 1 Shell, 2 Cell, 3 Rocket).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmmoPatch {
    /// Ammo index.
    pub index: i32,
    /// Max carried ammo.
    pub max_ammo: Option<i32>,
    /// Amount given per pickup.
    pub per_ammo: Option<i32>,
}

/// `Misc 0` fields used in M5.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MiscPatch {
    /// Initial health.
    pub initial_health: Option<i32>,
    /// Initial bullets.
    pub initial_bullets: Option<i32>,
    /// Max health.
    pub max_health: Option<i32>,
}

/// Parse a DeHackEd / BEX text blob.
pub fn parse_dehacked(src: &str) -> Result<DehackedPatch, DehackedError> {
    let mut patch = DehackedPatch::default();
    let mut lines = src.lines().map(str::trim_end).peekable();
    let mut in_strings = false;
    let mut in_codeptr = false;
    let mut in_bex_sounds = false;
    let mut in_bex_sprites = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("[STRINGS]") {
            in_strings = true;
            in_codeptr = false;
            in_bex_sounds = false;
            in_bex_sprites = false;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("[CODEPTR]") {
            in_codeptr = true;
            in_strings = false;
            in_bex_sounds = false;
            in_bex_sprites = false;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("[SOUNDS]") {
            in_bex_sounds = true;
            in_strings = false;
            in_codeptr = false;
            in_bex_sprites = false;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("[SPRITES]") {
            in_bex_sprites = true;
            in_strings = false;
            in_codeptr = false;
            in_bex_sounds = false;
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_strings = false;
            in_codeptr = false;
            in_bex_sounds = false;
            in_bex_sprites = false;
            continue;
        }
        if in_bex_sounds {
            if let Some((key, val)) = trimmed.split_once('=') {
                let k = key.trim();
                let v = val.trim();
                if !k.is_empty() && !v.is_empty() {
                    patch.bex_sounds.insert(k.to_owned(), v.to_owned());
                }
            }
            continue;
        }
        if in_bex_sprites {
            if let Some((key, val)) = trimmed.split_once('=') {
                let k = key.trim();
                let v = val.trim();
                if !k.is_empty() && !v.is_empty() {
                    patch.bex_sprites.insert(k.to_owned(), v.to_owned());
                }
            }
            continue;
        }
        if in_codeptr {
            if let Some((key, val)) = trimmed.split_once('=') {
                let key = key.trim();
                let val = val.trim();
                if let Some(frame) = key
                    .strip_prefix("FRAME")
                    .or_else(|| key.strip_prefix("Frame"))
                    .map(str::trim)
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    let mnemonic = val.trim().to_owned();
                    if !mnemonic.is_empty() {
                        patch.codeptrs.insert(frame, mnemonic);
                    }
                }
            }
            continue;
        }
        if in_strings {
            if let Some((key, val)) = trimmed.split_once('=') {
                let mut value = val.trim().to_owned();
                // Line continuation with trailing `\`.
                while value.ends_with('\\') {
                    value.pop();
                    if let Some(cont) = lines.next() {
                        value.push_str(cont.trim());
                    }
                }
                patch
                    .strings
                    .insert(key.trim().to_owned(), unescape_bex(&value));
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Doom version") {
            let v = rest
                .trim()
                .trim_start_matches('=')
                .trim()
                .parse()
                .map_err(|_| DehackedError::Message("bad Doom version".into()))?;
            patch.doom_version = Some(v);
            continue;
        }
        if is_indexed_block(trimmed, "Thing ") {
            let thing = parse_thing_block(trimmed, &mut lines)?;
            patch.things.push(thing);
            continue;
        }
        if is_indexed_block(trimmed, "Frame ") {
            let frame = parse_frame_block(trimmed, &mut lines)?;
            patch.frames.push(frame);
            continue;
        }
        if is_indexed_block(trimmed, "Pointer ") {
            let pointer = parse_pointer_block(trimmed, &mut lines)?;
            patch.pointers.push(pointer);
            continue;
        }
        if is_indexed_block(trimmed, "Weapon ") {
            let weapon = parse_weapon_block(trimmed, &mut lines)?;
            patch.weapons.push(weapon);
            continue;
        }
        if is_indexed_block(trimmed, "Ammo ") {
            let ammo = parse_ammo_block(trimmed, &mut lines)?;
            patch.ammos.push(ammo);
            continue;
        }
        if is_indexed_block(trimmed, "Misc ") {
            patch.misc = Some(parse_misc_block(&mut lines)?);
            continue;
        }
        if is_indexed_block(trimmed, "Sound ") {
            patch.sounds.push(parse_sound_block(trimmed, &mut lines)?);
            continue;
        }
        if is_indexed_block(trimmed, "Sprite ") {
            patch.sprites.push(parse_sprite_block(trimmed, &mut lines)?);
            continue;
        }
        // Text / Cheat — skip until blank.
        if starts_block(trimmed) {
            skip_block(&mut lines);
        }
    }
    Ok(patch)
}

/// True when `line` is a DeHackEd block header (`Ammo 0`, not `Ammo type = …`).
fn is_indexed_block(line: &str, prefix: &str) -> bool {
    let Some(rest) = line.strip_prefix(prefix) else {
        return false;
    };
    rest.trim_start()
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
}

fn starts_block(line: &str) -> bool {
    is_indexed_block(line, "Frame ")
        || is_indexed_block(line, "Pointer ")
        || is_indexed_block(line, "Sound ")
        || is_indexed_block(line, "Sprite ")
        || is_indexed_block(line, "Ammo ")
        || is_indexed_block(line, "Weapon ")
        || is_indexed_block(line, "Cheat ")
        || line.starts_with("Text ")
}

fn skip_block<'a>(lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>) {
    while let Some(l) = lines.peek() {
        let t = l.trim();
        if t.is_empty() {
            break;
        }
        if block_boundary(t) {
            break;
        }
        lines.next();
    }
}

fn parse_thing_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<ThingPatch, DehackedError> {
    // `Thing 2 (Former Human)` or `Thing 2`
    let rest = header["Thing ".len()..].trim();
    let (idx_str, name) = if let Some((n, rem)) = rest.split_once('(') {
        let name = rem.trim().trim_end_matches(')').trim();
        (n.trim(), Some(name.to_owned()))
    } else {
        (rest, None)
    };
    let index: i32 = idx_str
        .split_whitespace()
        .next()
        .ok_or_else(|| DehackedError::Message("Thing without index".into()))?
        .parse()
        .map_err(|_| DehackedError::Message(format!("bad Thing index in {header}")))?;
    let mut thing = ThingPatch {
        index,
        name,
        ..ThingPatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if t.is_empty() || starts_block(t) || is_indexed_block(t, "Thing ") || t.starts_with('[') {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Hit points" => thing.hit_points = Some(parse_i32(val)?),
                "Width" => thing.width = Some(parse_dim(val)),
                "Height" => thing.height = Some(parse_dim(val)),
                "Speed" => thing.speed = Some(parse_i32(val)?),
                "Mass" => thing.mass = Some(parse_i32(val)?),
                "Reaction time" => thing.reaction_time = Some(parse_i32(val)?),
                "Pain chance" => thing.pain_chance = Some(parse_i32(val)?),
                "Missile damage" => thing.damage = Some(parse_i32(val)?),
                "Bits" => thing.bits = parse_bits(val),
                "MBF21 Bits" => thing.mbf21_bits = parse_mbf21_bits(val),
                "ID #" => thing.doomednum = Some(parse_i32(val)?),
                "Alert sound" => thing.alert_sound = Some(parse_i32(val)?),
                "Attack sound" => thing.attack_sound = Some(parse_i32(val)?),
                "Pain sound" => thing.pain_sound = Some(parse_i32(val)?),
                "Death sound" => thing.death_sound = Some(parse_i32(val)?),
                "Action sound" => thing.action_sound = Some(parse_i32(val)?),
                "Fast speed" => thing.fast_speed = Some(parse_i32(val)?),
                "Melee range" => thing.melee_range = Some(parse_i32(val)?),
                "Infighting group" => thing.infighting_group = Some(parse_i32(val)?),
                "Projectile group" => thing.projectile_group = Some(parse_i32(val)?),
                "Splash group" => thing.splash_group = Some(parse_i32(val)?),
                _ => {}
            }
        }
    }
    Ok(thing)
}

fn parse_pointer_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<PointerPatch, DehackedError> {
    // `Pointer 1 (Frame 175)` or `Pointer 1`
    let rest = header["Pointer ".len()..].trim();
    let (index, frame) = if let Some((n, rem)) = rest.split_once('(') {
        let index: i32 = n
            .split_whitespace()
            .next()
            .ok_or_else(|| DehackedError::Message("Pointer without index".into()))?
            .parse()
            .map_err(|_| DehackedError::Message(format!("bad Pointer index in {header}")))?;
        let inside = rem.trim().trim_end_matches(')').trim();
        let frame = inside
            .strip_prefix("Frame")
            .or_else(|| inside.strip_prefix("frame"))
            .map(str::trim)
            .and_then(|s| s.parse::<i32>().ok())
            .ok_or_else(|| DehackedError::Message(format!("bad Pointer frame in {header}")))?;
        (index, frame)
    } else {
        // No `(Frame N)` — treat pointer index as the target frame.
        let index = parse_block_index(header, "Pointer")?;
        (index, index)
    };
    let mut pointer = PointerPatch {
        index,
        frame,
        codep_frame: None,
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            if key.trim() == "Codep Frame" {
                pointer.codep_frame = Some(parse_i32(val.trim())?);
            }
        }
    }
    Ok(pointer)
}

fn parse_frame_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<FramePatch, DehackedError> {
    let index = parse_block_index(header, "Frame")?;
    let mut frame = FramePatch {
        index,
        ..FramePatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Sprite number" => frame.sprite_number = Some(parse_i32(val)?),
                "Sprite subnumber" => frame.sprite_subnumber = Some(parse_i32(val)?),
                "Duration" => frame.duration = Some(parse_i32(val)?),
                "Next frame" => frame.next_frame = Some(parse_i32(val)?),
                "Unknown 1" => frame.unknown1 = Some(parse_i32(val)?),
                "Unknown 2" => frame.unknown2 = Some(parse_i32(val)?),
                "Codep Frame" => frame.codep_frame = Some(parse_i32(val)?),
                "MBF21 Bits" => frame.mbf21_bits = parse_mbf21_frame_bits(val),
                "Args1" => frame.args[0] = Some(parse_i32(val)?),
                "Args2" => frame.args[1] = Some(parse_i32(val)?),
                "Args3" => frame.args[2] = Some(parse_i32(val)?),
                "Args4" => frame.args[3] = Some(parse_i32(val)?),
                "Args5" => frame.args[4] = Some(parse_i32(val)?),
                "Args6" => frame.args[5] = Some(parse_i32(val)?),
                "Args7" => frame.args[6] = Some(parse_i32(val)?),
                "Args8" => frame.args[7] = Some(parse_i32(val)?),
                _ => {}
            }
        }
    }
    Ok(frame)
}

fn parse_sound_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<SoundPatch, DehackedError> {
    let index = parse_block_index(header, "Sound")?;
    let mut sound = SoundPatch {
        index,
        ..SoundPatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            match key.trim() {
                "Offset" => sound.offset = Some(parse_i32(val.trim())?),
                "Zero" => sound.zero = Some(parse_i32(val.trim())?),
                "Value" => sound.value = Some(parse_i32(val.trim())?),
                _ => {}
            }
        }
    }
    Ok(sound)
}

fn parse_sprite_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<SpritePatch, DehackedError> {
    let index = parse_block_index(header, "Sprite")?;
    let mut sprite = SpritePatch {
        index,
        ..SpritePatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            if key.trim() == "Offset" {
                sprite.offset = Some(parse_i32(val.trim())?);
            }
        }
    }
    Ok(sprite)
}

fn parse_weapon_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<WeaponPatch, DehackedError> {
    let index = parse_block_index(header, "Weapon")?;
    let mut weapon = WeaponPatch {
        index,
        ..WeaponPatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Ammo type" => weapon.ammo_type = Some(parse_i32(val)?),
                "Deselect frame" => weapon.deselect_frame = Some(parse_i32(val)?),
                "Select frame" => weapon.select_frame = Some(parse_i32(val)?),
                "Bobbing frame" => weapon.bobbing_frame = Some(parse_i32(val)?),
                "Shooting frame" => weapon.shooting_frame = Some(parse_i32(val)?),
                "Firing frame" => weapon.firing_frame = Some(parse_i32(val)?),
                "Ammo per shot" => weapon.ammo_per_shot = Some(parse_i32(val)?),
                _ => {}
            }
        }
    }
    Ok(weapon)
}

fn parse_ammo_block<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<AmmoPatch, DehackedError> {
    let index = parse_block_index(header, "Ammo")?;
    let mut ammo = AmmoPatch {
        index,
        ..AmmoPatch::default()
    };
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Max ammo" => ammo.max_ammo = Some(parse_i32(val)?),
                "Per ammo" => ammo.per_ammo = Some(parse_i32(val)?),
                _ => {}
            }
        }
    }
    Ok(ammo)
}

fn block_boundary(t: &str) -> bool {
    t.is_empty()
        || starts_block(t)
        || is_indexed_block(t, "Thing ")
        || is_indexed_block(t, "Misc ")
        || t.starts_with('[')
}

fn parse_block_index(header: &str, kind: &str) -> Result<i32, DehackedError> {
    let rest = header
        .get(kind.len()..)
        .ok_or_else(|| DehackedError::Message(format!("bad {kind} header")))?
        .trim();
    rest.split_whitespace()
        .next()
        .ok_or_else(|| DehackedError::Message(format!("{kind} without index")))?
        .parse()
        .map_err(|_| DehackedError::Message(format!("bad {kind} index in {header}")))
}

fn parse_misc_block<'a>(
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<MiscPatch, DehackedError> {
    let mut misc = MiscPatch::default();
    while let Some(l) = lines.peek().copied() {
        let t = l.trim();
        if block_boundary(t) {
            break;
        }
        lines.next();
        if let Some((key, val)) = t.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Initial Health" => misc.initial_health = Some(parse_i32(val)?),
                "Initial Bullets" => misc.initial_bullets = Some(parse_i32(val)?),
                "Max Health" => misc.max_health = Some(parse_i32(val)?),
                _ => {}
            }
        }
    }
    Ok(misc)
}

fn parse_i32(s: &str) -> Result<i32, DehackedError> {
    s.parse()
        .map_err(|_| DehackedError::Message(format!("bad integer {s}")))
}

/// DeHackEd width/height: if ≥ 65536 treat as fixed_t → map units.
fn parse_dim(s: &str) -> i32 {
    let Ok(v) = s.parse::<i32>() else {
        return 0;
    };
    if v >= 65536 { v / 65536 } else { v }
}

fn parse_bits(s: &str) -> Option<u32> {
    if let Ok(v) = s.parse::<u32>() {
        return Some(v);
    }
    // Mnemonic form: SOLID+SHOOTABLE or SOLID|SHOOTABLE
    let mut bits = 0u32;
    for part in s.split(['+', '|', ',', ' ']) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        bits |= match p.to_ascii_uppercase().as_str() {
            "SOLID" => 1 << 0,
            "SHOOTABLE" => 1 << 1,
            "NOBLOCKMAP" => 1 << 2,
            "AMBUSH" => 1 << 3,
            "COUNTKILL" => 1 << 4,
            _ => 0,
        };
    }
    Some(bits)
}

fn unescape_bex(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t")
}

/// MBF21 thing `MBF21 Bits` (numeric or `+`/`|`-separated mnemonics).
fn parse_mbf21_bits(val: &str) -> Option<u32> {
    if let Ok(n) = val.parse::<u32>() {
        return Some(n);
    }
    if let Ok(n) = val.parse::<i32>() {
        return Some(n as u32);
    }
    let mut bits = 0u32;
    for p in val.split(['+', '|', ',', ' ']) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        bits |= match p.to_ascii_uppercase().as_str() {
            "LOGRAV" => 1 << 0,
            "SHORTMRANGE" => 1 << 1,
            "DMGIGNORED" => 1 << 2,
            "NORADIUSDMG" => 1 << 3,
            "FORCERADIUSDMG" => 1 << 4,
            "HIGHERMPROB" => 1 << 5,
            "RANGEHALF" => 1 << 6,
            "NOTHRESHOLD" => 1 << 7,
            "LONGMELEE" => 1 << 8,
            "BOSS" => 1 << 9,
            "MAP07BOSS1" => 1 << 10,
            "MAP07BOSS2" => 1 << 11,
            "E1M8BOSS" => 1 << 12,
            "E2M8BOSS" => 1 << 13,
            "E3M8BOSS" => 1 << 14,
            "E4M8BOSS" => 1 << 15,
            "E4M6BOSS" => 1 << 16,
            "RIP" => 1 << 17,
            "FULLVOLSOUNDS" => 1 << 18,
            _ => 0,
        };
    }
    Some(bits)
}

/// MBF21 frame `MBF21 Bits` (`SKILL5FAST` = 1).
fn parse_mbf21_frame_bits(val: &str) -> Option<u32> {
    if let Ok(n) = val.parse::<u32>() {
        return Some(n);
    }
    let mut bits = 0u32;
    for p in val.split(['+', '|', ',', ' ']) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        bits |= match p.to_ascii_uppercase().as_str() {
            "SKILL5FAST" => 1 << 0,
            _ => 0,
        };
    }
    Some(bits)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_thing_and_strings() {
        let src = r#"
Patch File for DeHackEd v3.0
Doom version = 19

Thing 2 (ZombieMan)
Hit points = 40
Width = 40
Height = 56
Bits = SOLID+SHOOTABLE

[STRINGS]
GOTARMOR = Picked up the armor.
"#;
        let p = parse_dehacked(src).unwrap();
        assert_eq!(p.doom_version, Some(19));
        assert_eq!(p.things.len(), 1);
        let t = &p.things[0];
        assert_eq!(t.index, 2);
        assert_eq!(t.name.as_deref(), Some("ZombieMan"));
        assert_eq!(t.hit_points, Some(40));
        assert_eq!(t.width, Some(40));
        assert_eq!(t.bits, Some(3));
        assert_eq!(
            p.strings.get("GOTARMOR").map(String::as_str),
            Some("Picked up the armor.")
        );
    }

    #[test]
    fn parses_mbf21_thing_fields() {
        let src = r#"
Thing 2 (ZombieMan)
Fast speed = 16
Melee range = 80
MBF21 Bits = NORADIUSDMG+BOSS
Infighting group = 1
Projectile group = 2
Splash group = 3
"#;
        let p = parse_dehacked(src).unwrap();
        let t = &p.things[0];
        assert_eq!(t.fast_speed, Some(16));
        assert_eq!(t.melee_range, Some(80));
        assert_eq!(t.mbf21_bits, Some((1 << 3) | (1 << 9)));
        assert_eq!(t.infighting_group, Some(1));
        assert_eq!(t.projectile_group, Some(2));
        assert_eq!(t.splash_group, Some(3));
    }

    #[test]
    fn parses_frame_weapon_ammo() {
        let src = r#"
Frame 175
Sprite number = 16
Sprite subnumber = 32768
Duration = 4
Next frame = 176
MBF21 Bits = SKILL5FAST
Args1 = 10
Args2 = 20

Weapon 2
Ammo type = 1
Ammo per shot = 1
Shooting frame = 52

Ammo 0
Max ammo = 400
Per ammo = 20
"#;
        let p = parse_dehacked(src).unwrap();
        assert_eq!(p.frames.len(), 1);
        assert_eq!(p.frames[0].index, 175);
        assert_eq!(p.frames[0].sprite_number, Some(16));
        assert_eq!(p.frames[0].sprite_subnumber, Some(32768));
        assert_eq!(p.frames[0].duration, Some(4));
        assert_eq!(p.frames[0].next_frame, Some(176));
        assert_eq!(p.frames[0].mbf21_bits, Some(1));
        assert_eq!(p.frames[0].args[0], Some(10));
        assert_eq!(p.frames[0].args[1], Some(20));
        assert_eq!(p.weapons.len(), 1);
        assert_eq!(p.weapons[0].index, 2);
        assert_eq!(p.weapons[0].ammo_type, Some(1));
        assert_eq!(p.weapons[0].ammo_per_shot, Some(1));
        assert_eq!(p.ammos.len(), 1);
        assert_eq!(p.ammos[0].max_ammo, Some(400));
        assert_eq!(p.ammos[0].per_ammo, Some(20));
    }

    #[test]
    fn parses_bex_codeptr() {
        let src = r#"
[CODEPTR]
FRAME 175 = Chase
FRAME 176 = FaceTarget
Frame 200 = FirePistol
"#;
        let p = parse_dehacked(src).unwrap();
        assert_eq!(p.codeptrs.get(&175).map(String::as_str), Some("Chase"));
        assert_eq!(p.codeptrs.get(&176).map(String::as_str), Some("FaceTarget"));
        assert_eq!(p.codeptrs.get(&200).map(String::as_str), Some("FirePistol"));
    }

    #[test]
    fn parses_pointer_block() {
        let src = r#"
Pointer 1 (Frame 175)
Codep Frame = 17

Pointer 2
Codep Frame = 20
"#;
        let p = parse_dehacked(src).unwrap();
        assert_eq!(p.pointers.len(), 2);
        assert_eq!(p.pointers[0].index, 1);
        assert_eq!(p.pointers[0].frame, 175);
        assert_eq!(p.pointers[0].codep_frame, Some(17));
        assert_eq!(p.pointers[1].index, 2);
        assert_eq!(p.pointers[1].frame, 2);
        assert_eq!(p.pointers[1].codep_frame, Some(20));
    }

    #[test]
    fn parses_sound_sprite_and_bex_tables() {
        let src = r#"
Thing 2
Alert sound = 35
Attack sound = 12

Sound 1
Offset = 0
Value = 0

Sprite 29
Offset = 0

[SOUNDS]
weapons/pistol = custom/pistol
[SPRITES]
POSS = TROO
"#;
        let p = parse_dehacked(src).unwrap();
        assert_eq!(p.things[0].alert_sound, Some(35));
        assert_eq!(p.things[0].attack_sound, Some(12));
        assert_eq!(p.sounds.len(), 1);
        assert_eq!(p.sounds[0].index, 1);
        assert_eq!(p.sprites[0].index, 29);
        assert_eq!(
            p.bex_sounds.get("weapons/pistol").map(String::as_str),
            Some("custom/pistol")
        );
        assert_eq!(p.bex_sprites.get("POSS").map(String::as_str), Some("TROO"));
    }
}

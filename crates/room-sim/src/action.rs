//! Native action function dispatch (M5 bootstrap subset).

use crate::actor::{Actor, ActorId};
use crate::hitscan::{line_attack, rail_attack};
use crate::world::World;
use room_core::dmath;
use room_core::{Angle, Bam, DVec3, Fixed, StateId, math};

/// Result of running an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionResult {
    /// Continue with the state's next pointer when tics expire.
    Continue,
    /// Jump to another state immediately.
    Jump(StateId),
}

/// Dispatch a native action by name. Unknown names are no-ops.
pub fn call_action(world: &mut World, self_id: ActorId, name: &str) -> ActionResult {
    call_action_args(world, self_id, name, &[])
}

/// Dispatch a native action with DECORATE arguments.
pub fn call_action_args(
    world: &mut World,
    self_id: ActorId,
    name: &str,
    args: &[String],
) -> ActionResult {
    // DecoBlock false-path: skip following flattened 0-tic actions.
    if let Some(a) = world.actors.get_mut(self_id) {
        if a.action_skip > 0 {
            a.action_skip -= 1;
            return ActionResult::Continue;
        }
    }
    match name {
        "A_SkipUnlessHealthLower" => a_skip_unless_health_lower(world, self_id, args),
        "A_SkipUnlessHealth" => a_skip_unless_health(world, self_id, args),
        "A_SkipUnlessInventory" => a_skip_unless_inventory(world, self_id, args),
        "A_SkipUnlessArg" => a_skip_unless_arg(world, self_id, args),
        "A_SkipUnlessSpeed" => a_skip_unless_speed(world, self_id, args),
        "A_SkipUnlessAngle" => a_skip_unless_angle(world, self_id, args),
        "A_SkipUnlessPitch" => a_skip_unless_pitch(world, self_id, args),
        "A_SkipUnlessTid" => a_skip_unless_tid(world, self_id, args),
        "A_SkipUnlessMass" => a_skip_unless_mass(world, self_id, args),
        "A_SkipUnlessHeight" => a_skip_unless_height(world, self_id, args),
        "A_SkipUnlessRadius" => a_skip_unless_radius(world, self_id, args),
        "A_SkipUnlessVelX" => a_skip_unless_vel(world, self_id, args, 0),
        "A_SkipUnlessVelY" => a_skip_unless_vel(world, self_id, args, 1),
        "A_SkipUnlessVelZ" => a_skip_unless_vel(world, self_id, args, 2),
        "A_SkipUnlessAlpha" => a_skip_unless_scaled(world, self_id, args, |a| a.alpha),
        "A_SkipUnlessGravity" => a_skip_unless_scaled(world, self_id, args, |a| a.gravity),
        "A_SkipUnlessRoll" => {
            a_skip_unless_i32(world, self_id, args, |a| a.roll.degrees().round() as i32)
        }
        "A_SkipUnlessDamage" => a_skip_unless_i32(world, self_id, args, |a| a.damage),
        "A_SkipUnlessScore" => a_skip_unless_i32(world, self_id, args, |a| a.score),
        "A_SkipUnlessSpecial" => a_skip_unless_i32(world, self_id, args, |a| i32::from(a.special)),
        "A_SkipUnlessFriction" => a_skip_unless_scaled(world, self_id, args, |a| a.friction),
        "A_SkipUnlessDamageFactor" => {
            a_skip_unless_scaled(world, self_id, args, |a| a.damage_factor)
        }
        "A_SkipUnlessAccuracy" => a_skip_unless_i32(world, self_id, args, |a| a.accuracy),
        "A_SkipUnlessStamina" => a_skip_unless_i32(world, self_id, args, |a| a.stamina),
        "A_SkipUnlessTeam" => {
            a_skip_unless_i32(world, self_id, args, |a| i32::from(a.designated_team))
        }
        "A_SkipUnlessMeleeRange" => {
            a_skip_unless_i32(world, self_id, args, |a| a.melee_range.round() as i32)
        }
        "A_SkipUnlessViewHeight" => {
            a_skip_unless_i32(world, self_id, args, |a| a.view_height.round() as i32)
        }
        "A_SkipUnlessAttackZOffset" => {
            a_skip_unless_i32(world, self_id, args, |a| a.attack_z_offset.round() as i32)
        }
        "A_SkipUnlessMaxStepHeight" => {
            a_skip_unless_i32(world, self_id, args, |a| a.max_step_height.round() as i32)
        }
        "A_SkipUnlessMaxDropOffHeight" => a_skip_unless_i32(world, self_id, args, |a| {
            a.max_dropoff_height.round() as i32
        }),
        "A_SkipUnlessJumpZ" => a_skip_unless_i32(world, self_id, args, |a| a.jump_z.round() as i32),
        "A_SkipUnlessFloatSpeed" => {
            a_skip_unless_i32(world, self_id, args, |a| a.float_speed.round() as i32)
        }
        "A_SkipUnlessFastSpeed" => {
            a_skip_unless_i32(world, self_id, args, |a| a.fast_speed.round() as i32)
        }
        "A_SkipUnlessCameraHeight" => {
            a_skip_unless_i32(world, self_id, args, |a| a.camera_height.round() as i32)
        }
        "A_SkipUnlessCameraFOV" => {
            a_skip_unless_i32(world, self_id, args, |a| a.camera_fov.round() as i32)
        }
        "A_SkipUnlessSpecies" => a_skip_unless_species(world, self_id, args),
        "A_SkipUnlessReactionTime" => a_skip_unless_i32(world, self_id, args, |a| a.reaction_time),
        "A_SkipUnlessPainThreshold" => {
            a_skip_unless_i32(world, self_id, args, |a| a.pain_threshold)
        }
        "A_SkipUnlessThreshold" => a_skip_unless_i32(world, self_id, args, |a| a.threshold),
        "A_SetReactionTime" => a_set_reaction_time(world, self_id, args),
        "A_SkipUnlessUserVar" => a_skip_unless_user_var(world, self_id, args),
        "A_SkipAlways" => a_skip_always(world, self_id, args),
        "A_ClearInventory" => a_clear_inventory(world, self_id),
        "A_SetScale" => a_set_scale(world, self_id, args),
        "A_SetSize" => a_set_size(world, self_id, args),
        "A_ChangeFlag" => a_change_flag(world, self_id, args),
        "A_SetHealth" => a_set_health(world, self_id, args),
        "A_SetSpeed" => a_set_speed(world, self_id, args),
        "A_SetTics" => a_set_tics(world, self_id, args),
        "A_SetGravity" => a_set_gravity(world, self_id, args),
        "A_SetMass" => a_set_mass(world, self_id, args),
        "A_SetMeleeRange" => a_set_melee_range(world, self_id, args),
        "A_SetJumpZ" => a_set_jump_z(world, self_id, args),
        "A_SetChaseGoal" => a_set_chase_goal(world, self_id, args),
        "A_SetFriction" => a_set_friction(world, self_id, args),
        "A_SetMaxStepHeight" => a_set_max_step_height(world, self_id, args),
        "A_SetMaxDropOffHeight" => a_set_max_dropoff_height(world, self_id, args),
        "A_SetFloatSpeed" => a_set_float_speed(world, self_id, args),
        "A_SetFastSpeed" => a_set_fast_speed(world, self_id, args),
        "A_SetViewHeight" => a_set_view_height(world, self_id, args),
        "A_SetAttackZOffset" => a_set_attack_z_offset(world, self_id, args),
        "A_SetObituary" => a_set_obituary(world, self_id, args),
        "A_SetHitObituary" => a_set_hit_obituary(world, self_id, args),
        "A_SetDamageFactor" => a_set_damage_factor(world, self_id, args),
        "A_SetTeam" => a_set_team(world, self_id, args),
        "A_SetCameraHeight" => a_set_camera_height(world, self_id, args),
        "A_SetCameraFOV" => a_set_camera_fov(world, self_id, args),
        "A_SetAccuracy" => a_set_accuracy(world, self_id, args),
        "A_SetStamina" => a_set_stamina(world, self_id, args),
        "A_SetRipperLevel" => a_set_ripper_level(world, self_id, args),
        "A_SetRipMin" => a_set_rip_min(world, self_id, args),
        "A_SetRipMax" => a_set_rip_max(world, self_id, args),
        "A_SetAngle" => a_set_angle(world, self_id, args),
        "A_SetPitch" => a_set_pitch(world, self_id, args),
        "A_SetRoll" => a_set_roll(world, self_id, args),
        "A_SetFloatBobPhase" => a_set_float_bob_phase(world, self_id, args),
        "A_Weave" => a_weave(world, self_id, args),
        "A_SetPainThreshold" => a_set_pain_threshold(world, self_id, args),
        "A_SetThreshold" => a_set_threshold(world, self_id, args),
        "A_JumpIfHigherOrLower" => a_jump_if_higher_or_lower(world, self_id, args),
        "A_SetTranslucent" => a_set_translucent(world, self_id, args),
        "A_Recoil" => a_recoil(world, self_id, args),
        "A_SetUserVar" => a_set_user_var(world, self_id, args),
        "A_SetUserArray" => a_set_user_array(world, self_id, args),
        "A_CheckProximity" => a_check_proximity(world, self_id, args),
        "A_ClearMaster" => {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.master = None;
            }
            ActionResult::Continue
        }
        "A_ClearTracer" => a_clear_tracer(world, self_id),
        "A_ChangeVelocity" => a_change_velocity(world, self_id, args),
        "A_ScaleVelocity" => a_scale_velocity(world, self_id, args),
        "A_Jump" => a_jump(world, self_id, args),
        "A_JumpIfHealthLower" => a_jump_if_health_lower(world, self_id, args),
        "A_JumpIfCloser" => a_jump_if_closer(world, self_id, args),
        "A_CheckFlag" => a_check_flag(world, self_id, args),
        "A_CheckRange" => a_check_range(world, self_id, args),
        "A_CheckSight" => a_check_sight(world, self_id, args),
        "A_CheckSightOrRange" => a_check_sight_or_range(world, self_id, args),
        "A_CheckFloor" => a_check_floor(world, self_id, args),
        "A_CheckCeiling" => a_check_ceiling(world, self_id, args),
        "A_CheckBlock" => a_check_block(world, self_id, args),
        "A_CheckLOF" => a_check_lof(world, self_id, args),
        "A_SetRenderStyle" => a_set_render_style(world, self_id, args),
        "A_FadeOut" => a_fade_out(world, self_id, args),
        "A_FadeIn" => a_fade_in(world, self_id, args),
        "A_FadeTo" => a_fade_to(world, self_id, args),
        "A_Print" | "A_PrintBold" | "A_Log" => a_print(world, args),
        "A_LogInt" => a_log_int(world, args),
        "A_LogFloat" => a_log_float(world, args),
        "A_SetTeleFog" => a_set_tele_fog(world, self_id, args),
        "A_SwapTeleFog" => a_swap_tele_fog(world, self_id),
        "A_SprayDecal" => a_spray_decal(world, self_id, args),
        "A_SpawnParticle" => a_spawn_particle(world, self_id, args),
        "A_ClearLastHeard" => a_clear_last_heard(world, self_id),
        "A_PlaySound" | "A_StartSound" => a_play_sound(world, self_id, args),
        "A_StopSound" => a_stop_sound(world, self_id),
        "A_SetTranslation" => a_set_translation(world, self_id, args),
        "A_SetSprite" => a_set_sprite(world, self_id, args),
        "A_SetFrame" => a_set_frame(world, self_id, args),
        "A_CopySpriteFrame" => a_copy_sprite_frame(world, self_id, args),
        "A_TransferInventory" => a_transfer_inventory(world, self_id, args),
        "A_RaiseSelf" => a_raise_self(world, self_id),
        "A_JumpIfInventory" => a_jump_if_inventory(world, self_id, args),
        "A_JumpIfInTargetInventory" => a_jump_if_in_target_inventory(world, self_id, args),
        "A_GiveInventory" => a_give_inventory(world, self_id, args),
        "A_GiveToTarget" => a_give_to_pointer(world, self_id, args, FacePtr::Target),
        "A_GiveToMaster" => a_give_to_pointer(world, self_id, args, FacePtr::Master),
        "A_GiveToChildren" => a_give_to_children(world, self_id, args),
        "A_DropItem" | "A_DropInventory" => a_drop_item(world, self_id, args),
        "A_TakeInventory" => a_take_inventory(world, self_id, args),
        "A_TakeFromTarget" => a_take_from_pointer(world, self_id, args, FacePtr::Target),
        "A_TakeFromMaster" => a_take_from_pointer(world, self_id, args, FacePtr::Master),
        "A_TakeFromChildren" => a_take_from_children(world, self_id, args),
        "A_TakeFromSiblings" => a_take_from_siblings(world, self_id, args),
        "A_SetInventory" => a_set_inventory(world, self_id, args),
        "A_DamageSelf" => a_damage_self(world, self_id, args),
        "A_DamageTarget" => a_damage_pointer(world, self_id, args, FacePtr::Target),
        "A_DamageMaster" => a_damage_pointer(world, self_id, args, FacePtr::Master),
        "A_DamageTracer" => a_damage_pointer(world, self_id, args, FacePtr::Tracer),
        "A_DamageChildren" => a_damage_children(world, self_id, args),
        "A_ClearTarget" => a_clear_target(world, self_id),
        "A_CopyFriendliness" => a_copy_friendliness(world, self_id, args),
        "A_SetFriendly" => a_set_friendly(world, self_id, args),
        "A_SetSpecies" => a_set_species(world, self_id, args),
        "A_SetTag" => a_set_tag(world, self_id, args),
        "A_SetFriendlySeeBlocks" => a_set_friendly_see_blocks(world, self_id, args),
        "A_SetConversation" => a_set_conversation(world, self_id, args),
        "A_Respawn" => a_respawn(world, self_id, args),
        "A_JumpIfNoTarget" => a_jump_if_no_target(world, self_id, args),
        "A_JumpIfTargetInLOS" => a_jump_if_target_in_los(world, self_id, args),
        "A_Die" => a_die(world, self_id, args),
        "A_Remove" => a_remove(world, self_id),
        "A_SpawnItem" | "A_SpawnItemEx" => a_spawn_item(world, self_id, args),
        "A_CustomMissile" | "A_SpawnProjectile" => a_custom_missile(world, self_id, args),
        "A_ThrowGrenade" => a_throw_grenade(world, self_id, args),
        "A_SpawnDebris" => a_spawn_debris(world, self_id, args),
        "A_CustomMeleeAttack" => a_custom_melee_attack(world, self_id, args),
        "A_CustomComboAttack" | "A_BasicAttack" => a_custom_combo_attack(world, self_id, args),
        "A_CustomBulletAttack" => a_custom_bullet_attack(world, self_id, args),
        "A_RailAttack" | "A_CustomRailgun" => a_rail_attack(world, self_id, args),
        "A_SetDamageType" => a_set_damage_type(world, self_id, args),
        "A_SetDamage" => a_set_damage(world, self_id, args),
        "A_SetBloodColor" => a_set_blood_color(world, self_id, args),
        "A_SetFillColor" => a_set_fill_color(world, self_id, args),
        "A_SetScore" => a_set_score(world, self_id, args),
        "A_GiveScore" => a_give_score(world, self_id, args),
        "A_RadiusGive" => a_radius_give(world, self_id, args),
        "A_SetSpecial" => a_set_special(world, self_id, args),
        "A_SetArg" => a_set_arg(world, self_id, args),
        "A_SetTid" | "A_ChangeTID" => a_set_tid(world, self_id, args),
        "A_CallSpecial" => a_call_special(world, self_id, args),
        "A_TransferPointer" => a_transfer_pointer(world, self_id, args),
        "A_RearrangePointers" => a_rearrange_pointers(world, self_id, args),
        "A_Warp" => a_warp(world, self_id, args),
        "A_Teleport" => a_teleport(world, self_id, args),
        "A_SelectWeapon" => a_select_weapon(world, self_id, args),
        "A_Look" => a_look(world, self_id),
        "A_LookEx" => a_look_ex(world, self_id, args),
        "A_JumpIfTargetOutsideMeleeRange" => {
            a_jump_if_target_melee_range(world, self_id, args, false)
        }
        "A_JumpIfTargetInsideMeleeRange" => {
            a_jump_if_target_melee_range(world, self_id, args, true)
        }
        "A_Chase" => a_chase(world, self_id),
        "A_FaceTarget" => a_face_target(world, self_id),
        "A_FaceTracer" => a_face_named(world, self_id, FacePtr::Tracer),
        "A_FaceMaster" => a_face_named(world, self_id, FacePtr::Master),
        "A_FaceMovementDirection" => a_face_movement_direction(world, self_id),
        "A_AlertMonsters" => a_alert_monsters(world, self_id, args),
        "A_RaiseMaster" => a_raise_master(world, self_id),
        "A_RaiseChildren" => a_raise_children(world, self_id),
        "A_RemoveChildren" => a_remove_children(world, self_id),
        "A_KillChildren" => a_kill_children(world, self_id),
        "A_RaiseSiblings" => a_raise_siblings(world, self_id),
        "A_RemoveSiblings" => a_remove_siblings(world, self_id),
        "A_KillSiblings" => a_kill_siblings(world, self_id),
        "A_GiveToSiblings" => a_give_to_siblings(world, self_id, args),
        "A_DamageSiblings" => a_damage_siblings(world, self_id, args),
        "A_RemoveMaster" => a_remove_master(world, self_id),
        "A_KillMaster" => a_kill_master(world, self_id),
        "A_SetInvisible" => a_set_invisible(world, self_id, true),
        "A_UnsetInvisible" => a_set_invisible(world, self_id, false),
        "A_PosAttack" => a_hitscan_attack(world, self_id, 1, "grunt/attack"),
        "A_SPosAttack" => a_hitscan_attack(world, self_id, 3, "shotguy/attack"),
        "A_CPosAttack" => a_hitscan_attack(world, self_id, 1, "chainguy/attack"),
        "A_TroopAttack" => a_troop_attack(world, self_id),
        "A_SargAttack" => a_sarg_attack(world, self_id),
        "A_SkullAttack" => a_skull_attack(world, self_id),
        "A_HeadAttack" => a_head_attack(world, self_id),
        "A_BruisAttack" => a_bruis_attack(world, self_id),
        "A_CyberAttack" => a_cyber_attack(world, self_id),
        "A_Hoof" => a_walk_sound_chase(world, self_id, "cyber/hoof"),
        "A_Metal" => a_walk_sound_chase(world, self_id, "spider/walk"),
        "A_BabyMetal" => a_walk_sound_chase(world, self_id, "baby/walk"),
        "A_Punch" => a_punch(world, self_id),
        "A_CustomPunch" => a_custom_punch(world, self_id, args),
        "A_FireBullets" => a_fire_bullets(world, self_id, args),
        "A_Saw" => a_saw(world, self_id),
        "A_FirePistol" => a_fire_pistol(world, self_id),
        "A_FireCGun" => a_fire_cgun(world, self_id),
        "A_FireShotgun" => a_fire_shotgun(world, self_id),
        "A_FireShotgun2" => a_fire_shotgun2(world, self_id),
        "A_CheckReload" | "A_OpenShotgun2" | "A_LoadShotgun2" | "A_CloseShotgun2" => {
            ActionResult::Continue
        }
        "A_FireMissile" => a_fire_missile(world, self_id),
        "A_FireProjectile" | "A_FireCustomMissile" => a_fire_projectile(world, self_id, args),
        "A_FirePlasma" => a_fire_plasma(world, self_id),
        "A_FireBFG" => a_fire_bfg(world, self_id),
        "A_BFGSpray" => a_bfg_spray(world, self_id),
        "A_BFGsound" => {
            if let Some(a) = world.actors.get(self_id) {
                world.play_sound("weapons/bfgf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            ActionResult::Continue
        }
        "A_PainAttack" => a_pain_attack(world, self_id),
        "A_PainDie" => a_pain_die(world, self_id),
        "A_VileChase" => a_vile_chase(world, self_id),
        "A_VileStart" => a_vile_start(world, self_id),
        "A_VileTarget" => a_vile_target(world, self_id),
        "A_VileAttack" => a_vile_attack(world, self_id),
        "A_StartFire" => a_start_fire(world, self_id),
        "A_Fire" => a_fire(world, self_id),
        "A_FireCrackle" => a_fire_crackle(world, self_id),
        "A_SkelMissile" => a_skel_missile(world, self_id),
        "A_Tracer" => a_tracer(world, self_id),
        "A_SeekerMissile" => a_seeker_missile(world, self_id, args),
        "A_SkelWhoosh" => a_skel_whoosh(world, self_id),
        "A_SkelFist" => a_skel_fist(world, self_id),
        "A_BspiAttack" => a_bspi_attack(world, self_id),
        "A_FatAttack1" => a_fat_attack(world, self_id, 0.0),
        "A_FatAttack2" => a_fat_attack(world, self_id, 11.25),
        "A_FatAttack3" => a_fat_attack(world, self_id, -11.25),
        "A_SpidRefire" => a_refire(world, self_id, 10),
        "A_CPosRefire" => a_refire(world, self_id, 40),
        "A_MonsterRefire" => a_monster_refire(world, self_id, args),
        "A_SentinelRefire" => a_monster_refire(world, self_id, &["30".into(), "See".into()]),
        "A_BossDeath" | "A_KeenDie" => a_boss_death(world, self_id),
        "A_BrainAwake" => a_brain_awake(world, self_id),
        "A_BrainPain" => a_brain_pain(world, self_id),
        "A_BrainScream" => a_brain_scream(world, self_id),
        "A_BrainExplode" => a_brain_explode(world, self_id),
        "A_BrainDie" => a_brain_die(world),
        "A_BrainSpit" => a_brain_spit(world, self_id, args),
        "A_SpawnFly" => a_spawn_fly(world, self_id, args),
        "A_SpawnSound" => a_spawn_sound(world, self_id),
        "A_FatRaise" => {
            if let Some(a) = world.actors.get(self_id) {
                world.play_sound("fatso/raiseguns", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            ActionResult::Continue
        }
        "A_WeaponReady" | "A_ReFire" | "A_GunFlash" | "A_Light1" | "A_Light2" | "A_Light0"
        | "A_LightDone" => ActionResult::Continue,
        "A_Stop" => {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.vel = DVec3::ZERO;
            }
            ActionResult::Continue
        }
        "A_Wander" => a_wander(world, self_id),
        "A_XScream" => {
            // Vanilla always plays `sfx_slop` (DSSLOP) on gib.
            if let Some(a) = world.actors.get(self_id) {
                world.play_sound("misc/gibbed", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            ActionResult::Continue
        }
        "A_PlayerScream" => {
            // Vanilla: sfx_pldeth, or sfx_pdiehi when health < -50 (commercial).
            if let Some(a) = world.actors.get(self_id) {
                let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
                let health = a.health;
                let class = a.class;
                let death = world
                    .classes
                    .get(class)
                    .map(|c| c.death_sound.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "pl_death".into());
                let sound = if health < -50 {
                    match death.as_str() {
                        "pl_death" => "pl_xdeath".into(),
                        "player/male/death" => "player/male/xdeath".into(),
                        other if other.ends_with("/death") => {
                            format!("{}xdeath", &other[..other.len() - "death".len()])
                        }
                        _ => "player/male/xdeath".into(),
                    }
                } else {
                    death
                };
                world.play_sound(&sound, x, y, z, 1.0);
            }
            ActionResult::Continue
        }
        "A_Scream" => {
            if let Some(a) = world.actors.get(self_id) {
                let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
                let class = a.class;
                let flags = a.flags;
                let sound = world
                    .classes
                    .get(class)
                    .map(|c| c.death_sound.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "grunt/death".into());
                // Doom `A_Scream`: `sfx_podth*` → `P_Random()%3`, `sfx_bgdth*` → `%2`.
                // Freedoom maps several trooper death sounds into the podth family.
                if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed && !sound.is_empty()
                {
                    let s = sound.to_ascii_lowercase();
                    if s.contains("grunt/death")
                        || s.contains("shotguy/death")
                        || s.contains("chainguy/death")
                        || s.contains("wolfss/death")
                    {
                        let _ = world.p_random("pr_scream") % 3;
                    } else if s.contains("imp/death") {
                        let _ = world.p_random("pr_scream") % 2;
                    }
                }
                // Vanilla: Spider / Cyberdemon death is full volume (`S_StartSound(NULL, …)`).
                let attn = if (flags & crate::class::FLAG_BOSS) != 0 {
                    0.0
                } else {
                    1.0
                };
                world.play_sound_attn(&sound, x, y, z, 1.0, attn);
            }
            ActionResult::Continue
        }
        "A_Pain" => {
            if let Some(a) = world.actors.get(self_id) {
                let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
                let health = a.health;
                let class = a.class;
                let sound = world
                    .classes
                    .get(class)
                    .map(|c| {
                        if health <= 0 && !c.howl_sound.is_empty() {
                            c.howl_sound.clone()
                        } else if !c.pain_sound.is_empty() {
                            c.pain_sound.clone()
                        } else {
                            String::new()
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "grunt/pain".into());
                world.play_sound(&sound, x, y, z, 1.0);
            }
            ActionResult::Continue
        }
        "A_NoBlocking" | "A_Fall" => a_no_blocking(world, self_id),
        "A_QueueCorpse" => {
            world.queue_corpse(self_id);
            ActionResult::Continue
        }
        "A_DequeueCorpse" => {
            world.dequeue_corpse(self_id);
            ActionResult::Continue
        }
        "A_Explode" => a_explode(world, self_id),
        "A_Detonate" => a_detonate(world, self_id),
        "A_Mushroom" => a_mushroom(world, self_id, args),
        "A_Spawn" => a_mbf_spawn(world, self_id, args),
        "A_Turn" => a_turn(world, self_id, args),
        "A_Face" => a_face(world, self_id, args),
        "A_Scratch" => a_scratch(world, self_id, args),
        "A_RandomJump" => a_random_jump(world, self_id, args),
        "A_LineEffect" => a_line_effect(world, self_id, args),
        // MBF21
        "A_SpawnObject" => a_spawn_object(world, self_id, args),
        "A_MonsterProjectile" => a_monster_projectile(world, self_id, args),
        "A_MonsterBulletAttack" => a_monster_bullet_attack(world, self_id, args),
        "A_MonsterMeleeAttack" => a_monster_melee_attack(world, self_id, args),
        "A_RadiusDamage" => a_radius_damage(world, self_id, args),
        "A_NoiseAlert" => a_noise_alert(world, self_id),
        "A_FindTracer" => a_find_tracer(world, self_id, args),
        "A_SeekTracer" => a_seek_tracer(world, self_id, args),
        "A_HealChase" => a_heal_chase(world, self_id, args),
        "A_JumpIfHealthBelow" => a_jump_if_health_below(world, self_id, args),
        "A_JumpIfTargetInSight" => a_jump_if_target_in_sight(world, self_id, args),
        "A_JumpIfTargetCloser" => a_jump_if_target_closer_mbf21(world, self_id, args),
        "A_JumpIfTracerInSight" => a_jump_if_tracer_in_sight(world, self_id, args),
        "A_JumpIfTracerCloser" => a_jump_if_tracer_closer(world, self_id, args),
        "A_AddFlags" => a_add_flags(world, self_id, args),
        "A_RemoveFlags" => a_remove_flags(world, self_id, args),
        "A_JumpIfFlagsSet" => a_jump_if_flags_set(world, self_id, args),
        "A_RadiusThrust" => a_radius_thrust(world, self_id, args),
        "A_Quake" => a_quake(world, self_id, args),
        "A_QuakeEx" => a_quake_ex(world, self_id, args),
        "A_SetBlend" => a_set_blend(world, args),
        _ => ActionResult::Continue,
    }
}

/// Internal DecoBlock helper: if `health >= thresh`, skip the next `count` actions.
fn a_skip_unless_health_lower(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
) -> ActionResult {
    let thresh = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let count = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    let Some(a) = world.actors.get_mut(self_id) else {
        return ActionResult::Continue;
    };
    if a.health >= thresh && count > 0 {
        a.action_skip = count;
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: unconditionally skip the next `count` actions.
fn a_skip_always(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let count = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if count > 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: if health fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_health(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.health)
}

/// Internal DecoBlock helper: if speed fails `cmp` vs `value`, skip next `count`.
/// cmp: 0=`==`, 1=`!=`, 2=`>`, 3=`>=`, 4=`<`, 5=`<=` (value is map units as int).
fn a_skip_unless_speed(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.speed.round() as i32)
}

/// Internal DecoBlock helper: if angle (degrees) fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_angle(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.angle.degrees().round() as i32)
}

/// Internal DecoBlock helper: if pitch (degrees) fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_pitch(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.pitch.degrees().round() as i32)
}

/// Internal DecoBlock helper: if tid fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_tid(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.tid)
}

/// Internal DecoBlock helper: if mass fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_mass(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.mass)
}

/// Internal DecoBlock helper: if height (map units) fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_height(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.height.round() as i32)
}

/// Internal DecoBlock helper: if radius (map units) fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_radius(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| a.radius.round() as i32)
}

/// Internal DecoBlock helper: if species fails `==`/`!=` vs string, skip next `count`.
fn a_skip_unless_species(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let cmp = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let want = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let count = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if count == 0 {
        return ActionResult::Continue;
    }
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let cur = if a.species.is_empty() {
        world
            .classes
            .get(a.class)
            .map(|c| c.species.as_str())
            .unwrap_or("")
            .to_owned()
    } else {
        a.species.clone()
    };
    let eq = cur.eq_ignore_ascii_case(&want);
    let ok = match cmp {
        0 => eq,
        1 => !eq,
        _ => false,
    };
    if !ok {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: if `velx`/`vely`/`velz` fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_vel(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    axis: u8,
) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| {
        let v = match axis {
            0 => a.vel.x(),
            1 => a.vel.y(),
            _ => a.vel.z(),
        };
        v.round() as i32
    })
}

/// Internal DecoBlock helper: float property vs value stored as `round(x * 1000)`.
fn a_skip_unless_scaled(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    cur_fn: impl FnOnce(&Actor) -> f64,
) -> ActionResult {
    a_skip_unless_i32(world, self_id, args, |a| {
        (cur_fn(a) * 1000.0).round() as i32
    })
}

fn a_skip_unless_i32(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    cur_fn: impl FnOnce(&crate::actor::Actor) -> i32,
) -> ActionResult {
    let cmp = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let value = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let count = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if count == 0 {
        return ActionResult::Continue;
    }
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let cur = cur_fn(a);
    let ok = match cmp {
        0 => cur == value,
        1 => cur != value,
        2 => cur > value,
        3 => cur >= value,
        4 => cur < value,
        5 => cur <= value,
        _ => false,
    };
    if !ok {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: if `user_*` var fails `cmp` vs `value`, skip next `count`.
fn a_skip_unless_user_var(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_ascii_lowercase())
        .unwrap_or_default();
    let cmp = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let value = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let count = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if name.is_empty() || count == 0 {
        return ActionResult::Continue;
    }
    let key = if name.starts_with("user_") {
        name
    } else {
        format!("user_{name}")
    };
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let cur = a
        .user_vars
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&key))
        .map(|(_, v)| *v)
        .unwrap_or(0);
    let ok = match cmp {
        0 => cur == value,
        1 => cur != value,
        2 => cur > value,
        3 => cur >= value,
        4 => cur < value,
        5 => cur <= value,
        _ => false,
    };
    if !ok {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: if `args[index]` fails `cmp` vs `value`, skip next `count`.
/// cmp: 0=`==`, 1=`!=`, 2=`>`, 3=`>=`, 4=`<`, 5=`<=`.
fn a_skip_unless_arg(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let index = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let cmp = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let value = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let count = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if index > 4 || count == 0 {
        return ActionResult::Continue;
    }
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let cur = a.args[index];
    let ok = match cmp {
        0 => cur == value,
        1 => cur != value,
        2 => cur > value,
        3 => cur >= value,
        4 => cur < value,
        5 => cur <= value,
        _ => false,
    };
    if !ok {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// Internal DecoBlock helper: if inventory count `< amount`, skip the next `count` actions.
fn a_skip_unless_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    let count = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0);
    if item.is_empty() || amount <= 0 || count == 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
        return ActionResult::Continue;
    };
    let have = world.inventory.count(self_id, cid);
    if have < amount {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.action_skip = count;
        }
    }
    ActionResult::Continue
}

/// `A_ClearInventory` — wipe the actor's inventory bag.
fn a_clear_inventory(world: &mut World, self_id: ActorId) -> ActionResult {
    world.inventory.clear(self_id);
    ActionResult::Continue
}

/// `A_SetScale(x [, y])` — rescale collision box from class defaults.
fn a_set_scale(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let sx = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(1.0);
    let sy = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(sx);
    let (base_r, base_h) = world
        .actors
        .get(self_id)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| (c.radius, c.height))
        .unwrap_or((20.0, 16.0));
    if let Some(a) = world.actors.get_mut(self_id) {
        a.scale_x = sx;
        a.scale_y = sy;
        a.radius = base_r * sx.abs();
        a.height = base_h * sy.abs();
    }
    ActionResult::Continue
}

/// `A_SetSize(radius [, height])` — set absolute collision size.
fn a_set_size(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let radius = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok());
    let height = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok());
    if let Some(a) = world.actors.get_mut(self_id) {
        if let Some(r) = radius {
            a.radius = r.max(0.0);
        }
        if let Some(h) = height {
            a.height = h.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetReactionTime(n)` — set wake delay countdown.
fn a_set_reaction_time(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.reaction_time = n.max(0);
        }
    }
    ActionResult::Continue
}

/// `A_SetHealth(n)` — set hit points.
fn a_set_health(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.health = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetSpeed(n)` — set movement / chase speed.
fn a_set_speed(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.speed = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetTics(n)` — set remaining state duration.
fn a_set_tics(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.tics = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetGravity(n)` — set gravity multiplier.
fn a_set_gravity(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.gravity = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetMass(n)` — set mass for thrust calculations.
fn a_set_mass(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.mass = n.max(1);
        }
    }
    ActionResult::Continue
}

/// `A_SetMeleeRange(range)` — runtime melee reach (map units).
fn a_set_melee_range(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.melee_range = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetJumpZ(z)` — player jump impulse (map units / tic).
fn a_set_jump_z(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.jump_z = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetChaseGoal(true|false)` — chase `goal` instead of `target`.
fn a_set_chase_goal(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let on = args
        .first()
        .map(|s| {
            let t = s.trim_matches('"').to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes"
        })
        .unwrap_or(true);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.chase_goal = on;
    }
    ActionResult::Continue
}

/// `A_SetFriction(amount)` — ground friction multiplier.
fn a_set_friction(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.friction = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetMaxStepHeight(h)` — max stair climb (map units).
fn a_set_max_step_height(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.max_step_height = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetMaxDropOffHeight(h)` — max drop without DROPOFF (map units).
fn a_set_max_dropoff_height(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.max_dropoff_height = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetFloatSpeed(speed)` — FLOAT Z-adjust speed per tic.
fn a_set_float_speed(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.float_speed = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetFastSpeed(speed)` — nightmare/fast monsters chase speed.
fn a_set_fast_speed(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.fast_speed = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetViewHeight(h)` — eye height for aiming / ACS view height.
fn a_set_view_height(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.view_height = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetAttackZOffset(z)` — hitscan Z offset above view height.
fn a_set_attack_z_offset(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.attack_z_offset = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetObituary("text")` — death message when this actor kills a player.
fn a_set_obituary(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let text = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.obituary = text;
    }
    ActionResult::Continue
}

/// `A_SetHitObituary("text")` — melee death message.
fn a_set_hit_obituary(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let text = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.hit_obituary = text;
    }
    ActionResult::Continue
}

/// `A_SetDamageFactor(factor)` — incoming damage multiplier.
fn a_set_damage_factor(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.damage_factor = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetTeam(team)` — designated team (255 = none).
fn a_set_team(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<u8>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.designated_team = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetCameraHeight(h)` — camera actor eye height.
fn a_set_camera_height(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.camera_height = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetCameraFOV(fov)` — camera FOV in degrees (0 = default).
fn a_set_camera_fov(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.camera_fov = n.max(0.0);
        }
    }
    ActionResult::Continue
}

/// `A_SetAccuracy(n)` — Strife-style accuracy score.
fn a_set_accuracy(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.accuracy = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetStamina(n)` — Strife-style stamina score.
fn a_set_stamina(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.stamina = n;
        }
    }
    ActionResult::Continue
}

/// `A_SetRipperLevel(level)` — projectile/monster ripper level.
fn a_set_ripper_level(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.ripper_level = n.max(0);
        }
    }
    ActionResult::Continue
}

/// `A_SetRipMin(min)` — minimum ripper level that can rip this actor.
fn a_set_rip_min(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.rip_level_min = n.max(0);
        }
    }
    ActionResult::Continue
}

/// `A_SetRipMax(max)` — maximum ripper level that can rip this actor (`0` = unlimited).
fn a_set_rip_max(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.rip_level_max = n.max(0);
        }
    }
    ActionResult::Continue
}

/// Whether a missile is a ripper (`+RIPPER` or `RipperLevel` / `A_SetRipperLevel` > 0).
#[must_use]
pub(crate) fn is_ripper_missile(a: &crate::actor::Actor) -> bool {
    a.ripper_level > 0 || (a.flags & crate::class::FLAG_RIPPER) != 0
}

/// Whether projectile `ripper` may rip through `victim` (GZDoom rip level rules).
#[must_use]
pub(crate) fn can_rip_through(ripper: &crate::actor::Actor, victim: &crate::actor::Actor) -> bool {
    if (victim.flags & crate::class::FLAG_DONTRIP) != 0 {
        return false;
    }
    let level = ripper.ripper_level;
    if victim.rip_level_min > 0 && level < victim.rip_level_min {
        return false;
    }
    if victim.rip_level_max > 0 && level > victim.rip_level_max {
        return false;
    }
    true
}

/// `A_SetAngle(degrees)` — set yaw.
fn a_set_angle(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(deg) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.angle = Angle::from_degrees(deg);
        }
    }
    ActionResult::Continue
}

/// `A_SetPitch(degrees)` — set pitch.
fn a_set_pitch(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(deg) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.pitch = Angle::from_degrees(deg);
        }
    }
    ActionResult::Continue
}

/// `A_SetRoll(degrees)` — set roll (bank).
fn a_set_roll(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(deg) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.roll = Angle::from_degrees(deg);
        }
    }
    ActionResult::Continue
}

/// `A_SetPainThreshold(value)` — runtime pain HP gate.
fn a_set_pain_threshold(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let value = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.pain_threshold = value.max(0);
    }
    ActionResult::Continue
}

/// `A_SetThreshold(value)` — infighting hold tics (`Threshold` property).
fn a_set_threshold(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let value = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.threshold = value.max(0);
    }
    ActionResult::Continue
}

/// `A_JumpIfHigherOrLower(high, low [, ptr])` — jump by Z vs Target/Master/Tracer.
fn a_jump_if_higher_or_lower(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let high = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let low = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let which = match args
        .get(2)
        .map(|s| s.trim_matches('"').to_ascii_lowercase())
        .as_deref()
    {
        Some("master") => FacePtr::Master,
        Some("tracer") => FacePtr::Tracer,
        _ => FacePtr::Target,
    };
    let Some(other) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    let Some(self_z) = world.actors.get(self_id).map(|a| a.pos.z()) else {
        return ActionResult::Continue;
    };
    let Some(other_z) = world.actors.get(other).map(|a| a.pos.z()) else {
        return ActionResult::Continue;
    };
    if self_z > other_z && !high.is_empty() {
        jump_to_label(world, self_id, &high)
    } else if self_z < other_z && !low.is_empty() {
        jump_to_label(world, self_id, &low)
    } else {
        ActionResult::Continue
    }
}

/// `A_SetFloatBobPhase(phase)` — set weave/bob phase (`0..63`; `<0` leaves unchanged).
fn a_set_float_bob_phase(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let phase = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(-1);
    if phase >= 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.float_bob_phase = (phase & 63) as u8;
        }
    }
    ActionResult::Continue
}

/// `A_Weave(horzSpeed, vertSpeed, horzDist, vertDist)` — Heretic-style weave step.
fn a_weave(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let hspeed = parse_i32_arg(args, 0, 2);
    let vspeed = parse_i32_arg(args, 1, 2);
    let hdist = parse_f64_arg(args, 2, 2.0);
    let vdist = parse_f64_arg(args, 3, 2.0);
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let phase = i32::from(a.float_bob_phase);
    let xy_i = (phase.wrapping_add(hspeed.wrapping_mul(2))) & 63;
    let z_i = (phase.wrapping_add(vspeed)) & 63;
    let xy = dmath::sin(f64::from(xy_i) * (core::f64::consts::TAU / 64.0)) * hdist;
    let dz = dmath::sin(f64::from(z_i) * (core::f64::consts::TAU / 64.0)) * vdist;
    let rad = a.angle.radians();
    let dx = -dmath::sin(rad) * xy;
    let dy = dmath::cos(rad) * xy;
    let dest = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
    let check = crate::physics::check_position(world, &a, dest, Some(self_id));
    if let Some(m) = world.actors.get_mut(self_id) {
        if check.ok {
            m.pos = dest;
        } else {
            m.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + dz);
        }
        m.float_bob_phase = ((phase + 1) & 63) as u8;
    }
    ActionResult::Continue
}

/// `A_SetTranslucent(alpha [, mode])` — set opacity 0..=1.
fn a_set_translucent(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let alpha = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.alpha = alpha;
        a.render_style = crate::class::RenderStyle::Translucent;
    }
    ActionResult::Continue
}

/// `A_Recoil(speed)` — thrust opposite facing.
fn a_recoil(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let speed = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        let rad = a.angle.radians();
        a.vel = DVec3::new(
            a.vel.x() - dmath::cos(rad) * speed,
            a.vel.y() - dmath::sin(rad) * speed,
            a.vel.z(),
        );
    }
    ActionResult::Continue
}

/// `A_SetUserVar("user_name", value)`.
fn a_set_user_var(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let value = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if name.is_empty() {
        return ActionResult::Continue;
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.user_vars.insert(name, value);
    }
    ActionResult::Continue
}

/// `A_SetUserArray("user_name", index, value)`.
fn a_set_user_array(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let index = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let value = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if name.is_empty() {
        return ActionResult::Continue;
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.user_arrays.entry(name).or_default().insert(index, value);
    }
    ActionResult::Continue
}

/// `A_CheckProximity(class, dist, count, flags, "Label")` — jump if enough of `class` in range.
fn a_check_proximity(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let dist = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let need = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1)
        .max(1);
    let label = args
        .get(4)
        .or_else(|| args.get(3))
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| s.parse::<i32>().is_err())
        .unwrap_or_default();
    if class_name.is_empty() || label.is_empty() {
        return ActionResult::Continue;
    }
    let Some(want) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let Some(self_a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let origin = self_a.pos;
    let d2 = dist * dist;
    let mut n = 0i32;
    for &id in &world.thinkers.default {
        if id == self_id {
            continue;
        }
        let Some(o) = world.actors.get(id) else {
            continue;
        };
        if o.destroyed || o.health <= 0 || o.class != want {
            continue;
        }
        let dx = o.pos.x() - origin.x();
        let dy = o.pos.y() - origin.y();
        let dz = o.pos.z() - origin.z();
        if dx * dx + dy * dy + dz * dz <= d2 {
            n += 1;
            if n >= need {
                return jump_to_label(world, self_id, &label);
            }
        }
    }
    ActionResult::Continue
}

/// `A_ChangeVelocity(x, y, z [, flags])` — add (or replace) velocity.
/// Flag bit 0 = relative to yaw; bit 1 = replace instead of add.
fn a_change_velocity(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let x = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let y = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let z = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let flags = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let Some(a) = world.actors.get_mut(self_id) else {
        return ActionResult::Continue;
    };
    let (dx, dy) = if (flags & 1) != 0 {
        let rad = a.angle.radians();
        let c = dmath::cos(rad);
        let s = dmath::sin(rad);
        (x * c - y * s, x * s + y * c)
    } else {
        (x, y)
    };
    if (flags & 2) != 0 {
        a.vel = DVec3::new(dx, dy, z);
    } else {
        a.vel = DVec3::new(a.vel.x() + dx, a.vel.y() + dy, a.vel.z() + z);
    }
    ActionResult::Continue
}

/// `A_ScaleVelocity(factor)` — multiply velocity by `factor`.
fn a_scale_velocity(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let factor = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(1.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.vel = DVec3::new(a.vel.x() * factor, a.vel.y() * factor, a.vel.z() * factor);
    }
    ActionResult::Continue
}

/// `A_Jump(chance, "Label")` — random jump (chance 0..=256).
fn a_jump(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let chance = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if chance <= 0 || label.is_empty() {
        return ActionResult::Continue;
    }
    let roll = i32::from(world.p_random("pr_jump"));
    if roll >= chance {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

/// `A_JumpIfHealthLower(threshold, "Label")` — jump when health < threshold.
fn a_jump_if_health_lower(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let thresh = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    if a.health >= thresh || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = a.class;
    if let Some(sid) = world.states.find_label(class, &label) {
        ActionResult::Jump(sid)
    } else {
        ActionResult::Continue
    }
}

/// Resolve a class state label for `self_id`, if present.
fn jump_to_label(world: &World, self_id: ActorId, label: &str) -> ActionResult {
    if label.is_empty() {
        return ActionResult::Continue;
    }
    let Some(class) = world.actors.get(self_id).map(|a| a.class) else {
        return ActionResult::Continue;
    };
    if let Some(sid) = world.states.find_label(class, label) {
        ActionResult::Jump(sid)
    } else {
        ActionResult::Continue
    }
}

/// True if `tid` is a usable chase/attack target.
fn target_is_valid(world: &World, tid: ActorId) -> bool {
    let Some(a) = world.actors.get(tid) else {
        return false;
    };
    !a.destroyed && a.health > 0 && (a.flags & crate::class::FLAG_NOTARGET) == 0
}

fn is_player_mo(world: &World, tid: ActorId) -> bool {
    world.players.iter().flatten().any(|p| p.mo == Some(tid))
}

/// Hostile monster for a `+FRIENDLY` actor (shootable, not friendly, not a player).
fn is_enemy_monster(world: &World, tid: ActorId) -> bool {
    let Some(a) = world.actors.get(tid) else {
        return false;
    };
    if a.destroyed || a.health <= 0 {
        return false;
    }
    if (a.flags & crate::class::FLAG_NOTARGET) != 0 {
        return false;
    }
    if (a.flags & crate::class::FLAG_SHOOTABLE) == 0 {
        return false;
    }
    if (a.flags & crate::class::FLAG_FRIENDLY) != 0 {
        return false;
    }
    !is_player_mo(world, tid)
}

/// Nearest living player mobj (fallback when `Actor::target` is unset/invalid).
fn nearest_player_mo(world: &World, from: ActorId) -> Option<ActorId> {
    let origin = world.actors.get(from)?.pos;
    let mut best: Option<(ActorId, f64)> = None;
    for p in world.players.iter().flatten() {
        let Some(mo) = p.mo else {
            continue;
        };
        let Some(a) = world.actors.get(mo) else {
            continue;
        };
        if a.destroyed || a.health <= 0 || (a.flags & crate::class::FLAG_NOTARGET) != 0 {
            continue;
        }
        let dx = a.pos.x() - origin.x();
        let dy = a.pos.y() - origin.y();
        let d2 = dx * dx + dy * dy;
        if best.as_ref().is_none_or(|(_, bd)| d2 < *bd) {
            best = Some((mo, d2));
        }
    }
    best.map(|(id, _)| id)
}

/// Nearest hostile monster for friendly AI (`A_Look` / `A_Chase`).
fn nearest_enemy_monster(world: &World, from: ActorId) -> Option<ActorId> {
    let origin = world.actors.get(from)?.pos;
    let mut best: Option<(ActorId, f64)> = None;
    for &id in &world.thinkers.default {
        if id == from || !is_enemy_monster(world, id) {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let dx = a.pos.x() - origin.x();
        let dy = a.pos.y() - origin.y();
        let d2 = dx * dx + dy * dy;
        if best.as_ref().is_none_or(|(_, bd)| d2 < *bd) {
            best = Some((id, d2));
        }
    }
    best.map(|(id, _)| id)
}

/// Resolve chase target: stored `target` if valid, else nearest foe (and store it).
/// Friends hunt enemy monsters; when none are available they follow the nearest player.
/// Hostiles hunt players.
fn resolve_target(world: &mut World, self_id: ActorId) -> Option<ActorId> {
    let friendly = world
        .actors
        .get(self_id)
        .is_some_and(|a| (a.flags & crate::class::FLAG_FRIENDLY) != 0);
    if let Some(tid) = world.actors.get(self_id).and_then(|a| a.target) {
        if target_is_valid(world, tid) {
            if friendly {
                if is_enemy_monster(world, tid) {
                    return Some(tid);
                }
            } else {
                return Some(tid);
            }
        }
    }
    let tid = if friendly {
        nearest_enemy_monster(world, self_id).or_else(|| nearest_player_mo(world, self_id))?
    } else {
        nearest_player_mo(world, self_id)?
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.target = Some(tid);
    }
    Some(tid)
}

/// True when a friendly should attack `tid` (enemy monster), not merely follow.
fn friendly_should_attack(world: &World, self_id: ActorId, tid: ActorId) -> bool {
    let friendly = world
        .actors
        .get(self_id)
        .is_some_and(|a| (a.flags & crate::class::FLAG_FRIENDLY) != 0);
    !friendly || is_enemy_monster(world, tid)
}

/// Distance from `self` to a valid chase target, if any.
fn distance_to_target(world: &mut World, self_id: ActorId) -> Option<(ActorId, f64)> {
    let tid = resolve_target(world, self_id)?;
    let self_a = world.actors.get(self_id)?;
    let tgt = world.actors.get(tid)?;
    let dx = tgt.pos.x() - self_a.pos.x();
    let dy = tgt.pos.y() - self_a.pos.y();
    Some((tid, dmath::sqrt(dx * dx + dy * dy)))
}

/// `A_JumpIfCloser(dist, "Label")` — jump if chase target is within `dist`.
fn a_jump_if_closer(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dist = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    match distance_to_target(world, self_id) {
        Some((_, d)) if d < dist => jump_to_label(world, self_id, &label),
        _ => ActionResult::Continue,
    }
}

/// `A_CheckRange(dist, "Label")` — jump if target is farther than `dist` (or missing).
fn a_check_range(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dist = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    match distance_to_target(world, self_id) {
        Some((_, d)) if d <= dist => ActionResult::Continue,
        _ => jump_to_label(world, self_id, &label),
    }
}

/// `A_CheckSight("Label")` — jump if there is LOS to the chase target.
fn a_check_sight(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if crate::sight::check_sight(world, self_id, tid) {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_CheckSightOrRange(dist, "Label")` — jump if farther than `dist` or no LOS.
fn a_check_sight_or_range(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dist = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some((tid, d)) = distance_to_target(world, self_id) else {
        return jump_to_label(world, self_id, &label);
    };
    if d > dist || !crate::sight::check_sight(world, self_id, tid) {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// Floor/ceiling planes under the actor (via `check_position`).
fn actor_planes(world: &World, self_id: ActorId) -> Option<(f64, f64, f64, f64)> {
    let a = world.actors.get(self_id)?;
    let check = crate::physics::check_position(world, a, a.pos, Some(self_id));
    Some((a.pos.z(), a.height, check.floor_z, check.ceiling_z))
}

/// `A_CheckFloor("Label")` — jump when standing on (or below) the floor plane.
fn a_check_floor(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some((z, _, floor_z, _)) = actor_planes(world, self_id) else {
        return ActionResult::Continue;
    };
    if z <= floor_z + 1e-3 {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_CheckCeiling("Label")` — jump when the actor's top touches the ceiling.
fn a_check_ceiling(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some((z, height, _, ceiling_z)) = actor_planes(world, self_id) else {
        return ActionResult::Continue;
    };
    if z + height >= ceiling_z - 1e-3 {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `CBF_*` for [`a_check_block`] (GZDoom subset).
const CBF_NOLINES: i32 = 1 << 0;
const CBF_SETTARGET: i32 = 1 << 1;
const CBF_SETMASTER: i32 = 1 << 2;
const CBF_SETTRACER: i32 = 1 << 3;
const CBF_NOACTORS: i32 = 1 << 6;
const CBF_ABSOLUTEPOS: i32 = 1 << 7;
const CBF_ABSOLUTEANGLE: i32 = 1 << 8;

/// `A_CheckBlock("Label" [, flags [, ptr [, xofs, yofs, zofs [, angle]]]])`.
///
/// Jump when a probe at the (optionally offset) position is blocked.
fn a_check_block(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if label.is_empty() {
        return ActionResult::Continue;
    }
    let flags = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let xofs = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let yofs = args
        .get(4)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let zofs = args
        .get(5)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let ang_arg = args
        .get(6)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let mut angle = ang_arg;
    if flags & CBF_ABSOLUTEANGLE == 0 {
        angle += a.angle.degrees();
    }
    let rad = angle.to_radians();
    let (s, c) = (dmath::sin(rad), dmath::cos(rad));
    let pos = if flags & CBF_ABSOLUTEPOS != 0 {
        DVec3::new(xofs, yofs, zofs)
    } else {
        let dx = xofs * c - yofs * s;
        let dy = xofs * s + yofs * c;
        DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + zofs)
    };
    let mut probe = a;
    probe.pos = pos;
    let check = crate::physics::check_position(world, &probe, pos, Some(self_id));
    let actor_hit = check.blocking_actor.is_some();
    let line_hit = check.blocking_line.is_some() || (!check.ok && !actor_hit);
    let blocked =
        (actor_hit && flags & CBF_NOACTORS == 0) || (line_hit && flags & CBF_NOLINES == 0);
    if !blocked {
        return ActionResult::Continue;
    }
    if let Some(blocker) = check.blocking_actor {
        if flags & (CBF_SETTARGET | CBF_SETMASTER | CBF_SETTRACER) != 0 {
            if let Some(caller) = world.actors.get_mut(self_id) {
                if flags & CBF_SETTARGET != 0 {
                    caller.target = Some(blocker);
                }
                if flags & CBF_SETMASTER != 0 {
                    caller.master = Some(blocker);
                }
                if flags & CBF_SETTRACER != 0 {
                    caller.tracer = Some(blocker);
                }
            }
        }
    }
    jump_to_label(world, self_id, &label)
}

/// `A_CheckLOF("Label" [, flags [, range [, minrange]]])` — jump when line of
/// fire to `target` is clear (sight) and within range.
fn a_check_lof(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if label.is_empty() {
        return ActionResult::Continue;
    }
    let range = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let minrange = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let Some((tid, dist)) = distance_to_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if range > 0.0 && dist > range {
        return ActionResult::Continue;
    }
    if minrange > 0.0 && dist < minrange {
        return ActionResult::Continue;
    }
    if crate::sight::check_sight(world, self_id, tid) {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_SetRenderStyle(alpha, "Style")` — set opacity and render style.
fn a_set_render_style(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let alpha = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok());
    let style = args
        .get(1)
        .and_then(|s| crate::class::RenderStyle::parse(s.trim_matches('"')));
    if let Some(a) = world.actors.get_mut(self_id) {
        if let Some(alpha) = alpha {
            a.alpha = alpha.clamp(0.0, 1.0);
        }
        if let Some(style) = style {
            a.render_style = style;
        }
    }
    ActionResult::Continue
}

/// `A_FadeOut([step])` — reduce alpha; destroy when fully faded.
fn a_fade_out(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let step = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.1);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.alpha = (a.alpha - step).max(0.0);
        if a.alpha <= 0.0 {
            a.destroyed = true;
            a.state = None;
        }
    }
    ActionResult::Continue
}

/// `A_FadeIn([step])` — increase alpha toward 1.
fn a_fade_in(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let step = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.1);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.alpha = (a.alpha + step).min(1.0);
    }
    ActionResult::Continue
}

/// `A_FadeTo(target [, step])` — step alpha toward `target` (0..=1).
fn a_fade_to(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let target = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let step = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.1)
        .abs();
    if let Some(a) = world.actors.get_mut(self_id) {
        if a.alpha < target {
            a.alpha = (a.alpha + step).min(target);
        } else if a.alpha > target {
            a.alpha = (a.alpha - step).max(target);
        }
    }
    ActionResult::Continue
}

/// `A_Print` / `A_PrintBold` / `A_Log("text")` — queue a HUD message.
fn a_print(world: &mut World, args: &[String]) -> ActionResult {
    let text = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    if let Some(text) = text {
        world.push_message(text);
    }
    ActionResult::Continue
}

/// `A_LogInt(n)` — queue decimal integer as a HUD message.
fn a_log_int(world: &mut World, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        world.push_message(n.to_string());
    }
    ActionResult::Continue
}

/// `A_LogFloat(x)` — queue a float as a HUD message.
fn a_log_float(world: &mut World, args: &[String]) -> ActionResult {
    if let Some(x) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
    {
        world.push_message(format!("{x}"));
    }
    ActionResult::Continue
}

/// `A_SetTeleFog(source [, dest])` — custom teleport fog class names.
fn a_set_tele_fog(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let source = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let dest = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_else(|| source.clone());
    if let Some(a) = world.actors.get_mut(self_id) {
        a.telefog_source = source;
        a.telefog_dest = dest;
    }
    ActionResult::Continue
}

/// `A_SwapTeleFog` — swap source/dest telefog class names.
fn a_swap_tele_fog(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        std::mem::swap(&mut a.telefog_source, &mut a.telefog_dest);
    }
    ActionResult::Continue
}

/// `A_ClearLastHeard` — clear `soundtarget` in the actor's sector (and self-matches).
fn a_clear_last_heard(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(pos) = world.actors.get(self_id).map(|a| a.pos) else {
        return ActionResult::Continue;
    };
    if let Some(sec) = room_nodebuild::point_in_sector(&world.map, pos.x(), pos.y()) {
        let si = sec.raw() as usize;
        if let Some(s) = world.sectors.get_mut(si) {
            s.soundtarget = None;
        }
    }
    for s in &mut world.sectors {
        if s.soundtarget == Some(self_id) {
            s.soundtarget = None;
        }
    }
    ActionResult::Continue
}

/// `A_SprayDecal("Name" [, dist])` — queue a DECALDEF decal at self (optional forward offset).
fn a_spray_decal(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let Some(name) = name else {
        return ActionResult::Continue;
    };
    let dist = parse_f64_arg(args, 1, 0.0);
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let rad = a.angle.radians();
    let x = a.pos.x() + dmath::cos(rad) * dist;
    let y = a.pos.y() + dmath::sin(rad) * dist;
    let z = a.pos.z() + a.height * 0.5;
    world.spawn_decal(&name, x, y, z, None);
    ActionResult::Continue
}

/// `A_SpawnParticle(color [, flags, lifetime, size, angle, xoff, yoff, zoff])`.
///
/// Bootstrap: colour + lifetime/size/offsets; velocity/accel ignored. `SPF_FULLBRIGHT` = flags bit 0.
fn a_spawn_particle(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let (r, g, b) = args
        .first()
        .map(|s| s.as_str())
        .and_then(parse_blend_color)
        .unwrap_or((1.0, 1.0, 1.0));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let color = ((r * 255.0) as u32) << 16 | ((g * 255.0) as u32) << 8 | (b * 255.0) as u32;
    let flags = parse_i32_arg(args, 1, 0);
    let lifetime = parse_i32_arg(args, 2, 35);
    let size = parse_f64_arg(args, 3, 4.0);
    let xoff = parse_f64_arg(args, 5, 0.0);
    let yoff = parse_f64_arg(args, 6, 0.0);
    let zoff = parse_f64_arg(args, 7, 0.0);
    // Angle arg (4) rotates XY offsets in actor space (bootstrap).
    let ang = parse_f64_arg(args, 4, 0.0).to_radians() + a.angle.radians();
    let x = a.pos.x() + dmath::cos(ang) * xoff - dmath::sin(ang) * yoff;
    let y = a.pos.y() + dmath::sin(ang) * xoff + dmath::cos(ang) * yoff;
    let z = a.pos.z() + a.height * 0.5 + zoff;
    world.spawn_particle(color, flags & 1 != 0, lifetime, size, x, y, z);
    ActionResult::Continue
}

/// `A_StopSound` — drop queued SFX events near this actor (channel tracking later).
fn a_stop_sound(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
    world.sound_events.retain(|ev| {
        let dx = ev.x - x;
        let dy = ev.y - y;
        let dz = ev.z - z;
        dx * dx + dy * dy + dz * dz >= 1.0
    });
    ActionResult::Continue
}

/// `A_PlaySound` / `A_StartSound("name" [, volume])` — queue a positional SFX.
fn a_play_sound(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let volume = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if let Some(name) = name {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound(&name, a.pos.x(), a.pos.y(), a.pos.z(), volume);
        }
    }
    ActionResult::Continue
}

/// `A_SetTranslation("name")` — set palette translation (empty clears).
fn a_set_translation(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.translation = name;
    }
    ActionResult::Continue
}

/// `A_SetSprite("SPRT")` — set 4-letter sprite name.
fn a_set_sprite(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if name.is_empty() {
        return ActionResult::Continue;
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.sprite = name.to_ascii_uppercase();
    }
    ActionResult::Continue
}

/// `A_SetFrame("A"|n)` — set frame letter (`A`=0) or numeric index.
fn a_set_frame(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let raw = args.first().map(|s| s.trim_matches('"')).unwrap_or("A");
    let frame = if let Ok(n) = raw.parse::<u8>() {
        n
    } else {
        let c = raw.chars().next().unwrap_or('A').to_ascii_uppercase();
        if c.is_ascii_alphabetic() {
            (c as u8).saturating_sub(b'A')
        } else {
            0
        }
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.frame = frame;
    }
    ActionResult::Continue
}

/// `A_CopySpriteFrame([ptr])` — copy sprite+frame from target/master/tracer (default target).
fn a_copy_sprite_frame(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let which = match args
        .first()
        .map(|s| s.trim_matches('"').to_ascii_lowercase())
        .as_deref()
    {
        Some("master") => FacePtr::Master,
        Some("tracer") => FacePtr::Tracer,
        _ => FacePtr::Target,
    };
    let Some(src) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    let Some((sprite, frame)) = world.actors.get(src).map(|a| (a.sprite.clone(), a.frame)) else {
        return ActionResult::Continue;
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.sprite = sprite;
        a.frame = frame;
    }
    ActionResult::Continue
}

/// `A_RaiseSelf` — revive self via `Raise` state when dead/corpse.
fn a_raise_self(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = try_raise_actor(world, self_id);
    ActionResult::Continue
}

/// `A_TransferInventory(tid, "Item")` — move all of item from self to first actor with `tid`.
fn a_transfer_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let tid = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let item = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if tid == 0 || item.is_empty() {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    let Some(dest) = world.find_by_tid(tid).filter(|&id| id != self_id) else {
        return ActionResult::Continue;
    };
    let have = world.inventory.count(self_id, cid);
    if have <= 0 {
        return ActionResult::Continue;
    }
    let taken = world.inventory.take(self_id, cid, have);
    if taken > 0 {
        let _ = crate::inventory::give_inventory(world, dest, cid, taken);
    }
    ActionResult::Continue
}

/// `A_JumpIfInventory("Item", amount, "Label")` — jump when inventory count ≥ amount.
fn a_jump_if_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    let label = args
        .get(2)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if item.is_empty() || label.is_empty() || amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    let have = world.inventory.count(self_id, cid);
    if have >= amount {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_JumpIfInTargetInventory("Item", amount, "Label")` — jump when target has enough.
fn a_jump_if_in_target_inventory(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    let label = args
        .get(2)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if item.is_empty() || label.is_empty() || amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(tid) = world
        .actors
        .get(self_id)
        .and_then(|a| a.target)
        .filter(|&t| target_is_valid(world, t))
    else {
        return ActionResult::Continue;
    };
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    if world.inventory.count(tid, cid) >= amount {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_GiveInventory("Item" [, amount])` — grant inventory to self.
fn a_give_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    if item.is_empty() || amount <= 0 {
        return ActionResult::Continue;
    }
    if let Some(cid) = world.classes.find_name(&item) {
        let _ = crate::inventory::give_inventory(world, self_id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_DropItem("Item" [, amount [, chance]])` — spawn a dropped pickup at feet.
fn a_drop_item(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1)
        .clamp(1, 16);
    let chance = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(256);
    if item.is_empty() {
        return ActionResult::Continue;
    }
    if chance < 256 {
        let roll = i32::from(world.p_random("a_dropitem"));
        if roll >= chance {
            return ActionResult::Continue;
        }
    }
    let Some(drop_class) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    for _ in 0..amount {
        if let Ok(id) = world.spawn_at(drop_class, src.pos, src.angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.flags |= crate::class::FLAG_DROPPED;
            }
        }
    }
    ActionResult::Continue
}

/// `A_TakeInventory("Item" [, amount])` — remove inventory from self.
fn a_take_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    if item.is_empty() || amount <= 0 {
        return ActionResult::Continue;
    }
    if let Some(cid) = world.classes.find_name(&item) {
        let _ = world.inventory.take(self_id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_SetInventory("Item", amount)` — set absolute inventory count on self.
fn a_set_inventory(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if item.is_empty() {
        return ActionResult::Continue;
    }
    if let Some(cid) = world.classes.find_name(&item) {
        crate::inventory::set_inventory(world, self_id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_ClearTarget` — drop chase/attack target.
fn a_clear_target(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        a.target = None;
    }
    ActionResult::Continue
}

/// `A_SetFriendly([bool])` — set or clear `FRIENDLY` (default true).
fn a_set_friendly(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let set = args.first().is_none_or(|s| {
        let t = s.trim_matches('"').to_ascii_lowercase();
        t.is_empty() || t == "1" || t == "true" || t == "yes"
    });
    if let Some(a) = world.actors.get_mut(self_id) {
        if set {
            a.flags |= crate::class::FLAG_FRIENDLY;
        } else {
            a.flags &= !crate::class::FLAG_FRIENDLY;
        }
    }
    ActionResult::Continue
}

/// `A_SetSpecies("Name")` — runtime species override for same-species rules.
fn a_set_species(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.species = name;
    }
    ActionResult::Continue
}

/// `A_SetTag("Name")` — runtime display name (`APROP_Nametag`).
fn a_set_tag(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.nametag = name;
    }
    ActionResult::Continue
}

/// `A_SetFriendlySeeBlocks(n)` — friendly look range in blockmap blocks.
fn a_set_friendly_see_blocks(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.friendly_see_blocks = n.max(0);
        }
    }
    ActionResult::Continue
}

/// `A_SetConversation(id)` — Strife / UDMF dialogue ID (`0` clears).
fn a_set_conversation(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    if let Some(n) = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
    {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.conversation = n.max(0);
        }
    }
    ActionResult::Continue
}

/// `A_CopyFriendliness([ptr])` — copy `FRIENDLY` from Target/Master/Tracer (default Target).
fn a_copy_friendliness(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let which = match args
        .first()
        .map(|s| s.trim_matches('"').to_ascii_lowercase())
        .as_deref()
    {
        Some("master") => FacePtr::Master,
        Some("tracer") => FacePtr::Tracer,
        _ => FacePtr::Target,
    };
    let Some(src) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    let src_friendly = world
        .actors
        .get(src)
        .is_some_and(|a| (a.flags & crate::class::FLAG_FRIENDLY) != 0);
    if let Some(a) = world.actors.get_mut(self_id) {
        if src_friendly {
            a.flags |= crate::class::FLAG_FRIENDLY;
        } else {
            a.flags &= !crate::class::FLAG_FRIENDLY;
        }
    }
    ActionResult::Continue
}

/// `A_Respawn([flags])` — revive at `spawn_point` into `Spawn` (RSF_FOG = 1).
fn a_respawn(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let flags = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    let fog = flags & 1 != 0;
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    if a.destroyed {
        return ActionResult::Continue;
    }
    // Live actors with full health ignore (corpse / dead only).
    if a.health > 0 && (a.flags & crate::class::FLAG_CORPSE) == 0 {
        return ActionResult::Continue;
    }
    let spawn_pos = a.spawn_point;
    let spawn_angle = a.spawn_angle;
    let old_pos = a.pos;
    let probe = Actor {
        pos: spawn_pos,
        destroyed: false,
        health: 1,
        flags: a.flags & !crate::class::FLAG_CORPSE,
        ..a.clone()
    };
    let check = crate::physics::check_position(world, &probe, spawn_pos, Some(self_id));
    if !check.ok {
        return ActionResult::Continue;
    }
    let spawn_hp = world
        .classes
        .get(a.class)
        .map(|c| c.health.max(1))
        .unwrap_or(1);
    let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
    let Some(spawn) = world.states.find_label(a.class, "Spawn") else {
        return ActionResult::Continue;
    };
    if (class_flags & crate::class::FLAG_COUNTKILL) != 0 {
        world.level.found_kills = world.level.found_kills.saturating_sub(1);
    }
    if let Some(m) = world.actors.get_mut(self_id) {
        m.pos = spawn_pos;
        m.angle = spawn_angle;
        m.vel = DVec3::ZERO;
        m.health = spawn_hp;
        m.destroyed = false;
        m.flags = class_flags;
        m.target = None;
        m.reaction_time = 18;
        m.move_count = 0;
    }
    world.dequeue_corpse(self_id);
    if fog {
        crate::specials::spawn_teleport_fog(world, old_pos);
        crate::specials::spawn_teleport_fog(world, spawn_pos);
    }
    // Enter Spawn without running A_Look this tic (avoids immediate See/Chase).
    world.set_state(self_id, Some(spawn), false);
    ActionResult::Continue
}

/// `A_JumpIfNoTarget("Label")` — jump when target is missing or dead.
fn a_jump_if_no_target(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let has = world
        .actors
        .get(self_id)
        .and_then(|a| a.target)
        .is_some_and(|tid| target_is_valid(world, tid));
    if has {
        ActionResult::Continue
    } else {
        jump_to_label(world, self_id, &label)
    }
}

/// `A_JumpIfTargetInLOS("Label")` — jump when target is alive and in line of sight.
fn a_jump_if_target_in_los(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some(tid) = world.actors.get(self_id).and_then(|a| a.target) else {
        return ActionResult::Continue;
    };
    if !target_is_valid(world, tid) {
        return ActionResult::Continue;
    }
    if crate::sight::check_sight(world, self_id, tid) {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_JumpIfTargetOutsideMeleeRange` / `Inside` — distance vs class `MeleeRange`.
fn a_jump_if_target_melee_range(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    inside: bool,
) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some((tid, dist)) = distance_to_target(world, self_id) else {
        return ActionResult::Continue;
    };
    let _ = tid;
    let melee = world
        .actors
        .get(self_id)
        .map(|a| {
            if a.melee_range > 0.0 {
                a.melee_range
            } else {
                class_melee_range(world, a.class)
            }
        })
        .unwrap_or(64.0);
    let hit = if inside { dist <= melee } else { dist > melee };
    if hit {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_Die([damageType])` — force death (optional typed Death.* label).
fn a_die(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dtype = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let Some(a) = world.actors.get_mut(self_id) else {
        return ActionResult::Continue;
    };
    if a.destroyed || a.health <= 0 {
        return ActionResult::Continue;
    }
    let class = a.class;
    a.health = 0;
    crate::damage::die(world, self_id, class, None, dtype.as_deref());
    ActionResult::Continue
}

/// `A_Remove` — destroy self without a death sequence.
fn a_remove(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        a.destroyed = true;
        a.state = None;
        a.health = 0;
        a.flags &= !crate::class::FLAG_SOLID;
        a.flags &= !crate::class::FLAG_SHOOTABLE;
    }
    ActionResult::Continue
}

/// `A_SpawnItem` / `A_SpawnItemEx("class" [, x, y, z [, xvel, yvel, zvel, angle]])`.
/// Offsets are relative to the spawner's yaw.
fn a_spawn_item(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if class_name.is_empty() {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let ox = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let oy = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let oz = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let vx = args
        .get(4)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let vy = args
        .get(5)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let vz = args
        .get(6)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let ang_off = args
        .get(7)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let rad = src.angle.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let pos = DVec3::new(
        src.pos.x() + ox * c - oy * s,
        src.pos.y() + ox * s + oy * c,
        src.pos.z() + oz,
    );
    let ang = Angle::from_degrees(src.angle.degrees() + ang_off);
    if let Ok(id) = world.spawn_at(cid, pos, ang) {
        if let Some(m) = world.actors.get_mut(id) {
            m.vel = DVec3::new(vx * c - vy * s, vx * s + vy * c, vz);
            m.target = src.target;
            m.master = Some(self_id);
        }
    }
    ActionResult::Continue
}

/// `A_ThrowGrenade("type" [, zheight [, xyspeed [, zspeed]]])` — lofted projectile.
fn a_throw_grenade(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if class_name.is_empty() {
        return ActionResult::Continue;
    }
    let zheight = parse_f64_arg(args, 1, 32.0);
    let xyspeed = parse_f64_arg(args, 2, 10.0);
    let zspeed = parse_f64_arg(args, 3, 6.0);
    let Some(missile_class) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let ang = src.angle;
    let rad = ang.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let pos = DVec3::new(
        src.pos.x() + c * (src.radius + 8.0),
        src.pos.y() + s * (src.radius + 8.0),
        src.pos.z() + zheight,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return ActionResult::Continue;
    };
    if let Some(m) = world.actors.get_mut(id) {
        m.vel = DVec3::new(c * xyspeed, s * xyspeed, zspeed);
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.target = Some(self_id);
        m.master = Some(self_id);
        m.tracer = src.target;
    }
    check_missile_spawn(world, id);
    ActionResult::Continue
}

/// `A_SpawnDebris("type" [, transfer_translation [, mult_h [, mult_v]]])`.
fn a_spawn_debris(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if class_name.is_empty() {
        return ActionResult::Continue;
    }
    let transfer_tr = args
        .get(1)
        .map(|s| {
            let t = s.trim_matches('"').to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes"
        })
        .unwrap_or(false);
    let mult_h = parse_f64_arg(args, 2, 1.0).abs().max(0.01);
    let mult_v = parse_f64_arg(args, 3, 1.0).abs().max(0.01);
    let Some(debris_class) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    // Bootstrap: spawn a small burst (8) with randomized velocity.
    for _ in 0..8 {
        let Ok(id) = world.spawn_at(debris_class, src.pos, src.angle) else {
            continue;
        };
        let rx = i32::from(world.p_random("pr_spawndebris")) as f64 / 255.0 * 2.0 - 1.0;
        let ry = i32::from(world.p_random("pr_spawndebris")) as f64 / 255.0 * 2.0 - 1.0;
        let rz = i32::from(world.p_random("pr_spawndebris")) as f64 / 255.0;
        if let Some(d) = world.actors.get_mut(id) {
            d.vel = DVec3::new(
                rx * 4.0 * mult_h,
                ry * 4.0 * mult_h,
                (rz * 4.0 + 2.0) * mult_v,
            );
            d.flags &= !crate::class::FLAG_SOLID;
            d.master = Some(self_id);
            if transfer_tr {
                d.translation = src.translation.clone();
            }
        }
    }
    ActionResult::Continue
}

/// `A_CustomMissile` / `A_SpawnProjectile("class" [, spawnheight, spawnofs_xy, angle, flags, pitch])`.
fn a_custom_missile(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if class_name.is_empty() {
        return ActionResult::Continue;
    }
    let spawnheight = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(32.0);
    let spawnofs_xy = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let ang_off = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let _ = a_face_target(world, self_id);
    let Some(missile_class) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let ang = Angle::from_degrees(src.angle.degrees() + ang_off);
    let rad = ang.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let pos = DVec3::new(
        src.pos.x() + c * (src.radius + 8.0) - s * spawnofs_xy,
        src.pos.y() + s * (src.radius + 8.0) + c * spawnofs_xy,
        src.pos.z() + spawnheight,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return ActionResult::Continue;
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(20.0);
    if let Some(m) = world.actors.get_mut(id) {
        m.vel = DVec3::new(c * speed, s * speed, 0.0);
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.target = Some(self_id);
        m.master = Some(self_id);
        m.tracer = src.target;
    }
    check_missile_spawn(world, id);
    ActionResult::Continue
}

/// `A_CustomMeleeAttack(damage [, meleesound, misssound, damagetype, bleed])`.
fn a_custom_melee_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let damage = parse_i32_arg(args, 0, 0).max(0);
    let melee_sound = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let miss_sound = args
        .get(2)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let _ = a_face_target(world, self_id);
    let Some(tid) = resolve_target(world, self_id) else {
        if let (Some(snd), Some(a)) = (miss_sound.as_ref(), world.actors.get(self_id)) {
            world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        return ActionResult::Continue;
    };
    if !check_melee_range(world, self_id, tid) {
        if let (Some(snd), Some(a)) = (miss_sound.as_ref(), world.actors.get(self_id)) {
            world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        return ActionResult::Continue;
    }
    if let (Some(snd), Some(a)) = (melee_sound.as_ref(), world.actors.get(self_id)) {
        world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    if damage > 0 {
        crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), damage);
    }
    ActionResult::Continue
}

/// `A_CustomComboAttack` / `A_BasicAttack(missile, spawnheight, damage [, meleesound])`.
///
/// Melee when in range; otherwise spawn `missile` like `A_CustomMissile`.
fn a_custom_combo_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let missile = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let spawnheight = parse_f64_arg(args, 1, 32.0);
    let damage = parse_i32_arg(args, 2, 0).max(0);
    let melee_sound = args
        .get(3)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let _ = a_face_target(world, self_id);
    if let Some(tid) = resolve_target(world, self_id) {
        if check_melee_range(world, self_id, tid) {
            if let (Some(snd), Some(a)) = (melee_sound.as_ref(), world.actors.get(self_id)) {
                world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            if damage > 0 {
                crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), damage);
            }
            return ActionResult::Continue;
        }
    }
    if missile.is_empty() {
        return ActionResult::Continue;
    }
    a_custom_missile(
        world,
        self_id,
        &[missile, spawnheight.to_string(), "0".into(), "0".into()],
    )
}

/// `A_RailAttack` / `A_CustomRailgun(damage [, …, range, …, limit])` — piercing hitscan.
///
/// Bootstrap reads `damage` (arg0), optional `range` (arg10), optional `limit` (arg17).
fn a_rail_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let damage = parse_i32_arg(args, 0, 0).max(0);
    let range = args
        .get(10)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .filter(|r| *r > 0.0)
        .unwrap_or(8192.0);
    let limit = args
        .get(17)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let _ = a_face_target(world, self_id);
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    if damage > 0 {
        let _ = rail_attack(world, self_id, angle, range, damage, limit);
    }
    ActionResult::Continue
}

/// `A_CustomBulletAttack(spread_horiz, spread_vert, numbullets, damage [, range])`.
fn a_custom_bullet_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let spread_h = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let _spread_v = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let num = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1)
        .max(1);
    let damage = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(3);
    let range = args
        .get(4)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(2048.0);
    let _ = a_face_target(world, self_id);
    let Some(base) = world.actors.get(self_id).map(|a| a.angle) else {
        return ActionResult::Continue;
    };
    for i in 0..num {
        let t = if num == 1 {
            0.0
        } else {
            (f64::from(i) / f64::from(num - 1)) * 2.0 - 1.0
        };
        let ang = Angle::from_degrees(base.degrees() + t * spread_h);
        let _ = line_attack(world, self_id, ang, range, damage);
    }
    ActionResult::Continue
}

/// `A_CheckFlag("FLAG", "Label")` — jump if the actor has the named flag.
fn a_check_flag(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_ascii_uppercase())
        .unwrap_or_default();
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let bit = match name.as_str() {
        "SOLID" => crate::class::FLAG_SOLID,
        "SHOOTABLE" => crate::class::FLAG_SHOOTABLE,
        "NOGRAVITY" => crate::class::FLAG_NOGRAVITY,
        "FLOAT" => crate::class::FLAG_FLOAT,
        "DROPOFF" => crate::class::FLAG_DROPOFF,
        "DORMANT" => crate::class::FLAG_DORMANT,
        "FRIENDLY" => crate::class::FLAG_FRIENDLY,
        "NOTARGET" => crate::class::FLAG_NOTARGET,
        "INVULNERABLE" => crate::class::FLAG_INVULNERABLE,
        "NODAMAGE" => crate::class::FLAG_NODAMAGE,
        "NOBLOOD" => crate::class::FLAG_NOBLOOD,
        "FLOATBOB" => crate::class::FLAG_FLOATBOB,
        "AMBUSH" => crate::class::FLAG_AMBUSH,
        "MISSILE" => crate::class::FLAG_MISSILE,
        _ => 0,
    };
    if bit == 0 {
        return ActionResult::Continue;
    }
    let has = world
        .actors
        .get(self_id)
        .is_some_and(|a| (a.flags & bit) != 0);
    if has {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_ChangeFlag("FLAG", true|false)` — toggle a known flag bit on the actor.
fn a_change_flag(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_ascii_uppercase())
        .unwrap_or_default();
    let set = args.get(1).is_none_or(|s| {
        let t = s.trim_matches('"').to_ascii_lowercase();
        t == "1" || t == "true" || t == "yes"
    });
    let bit = match name.as_str() {
        "SOLID" => crate::class::FLAG_SOLID,
        "SHOOTABLE" => crate::class::FLAG_SHOOTABLE,
        "NOGRAVITY" => crate::class::FLAG_NOGRAVITY,
        "FLOAT" => crate::class::FLAG_FLOAT,
        "DROPOFF" => crate::class::FLAG_DROPOFF,
        "DORMANT" => crate::class::FLAG_DORMANT,
        "FRIENDLY" => crate::class::FLAG_FRIENDLY,
        "NOTARGET" => crate::class::FLAG_NOTARGET,
        "INVULNERABLE" => crate::class::FLAG_INVULNERABLE,
        "NODAMAGE" => crate::class::FLAG_NODAMAGE,
        "NOBLOOD" => crate::class::FLAG_NOBLOOD,
        "FLOATBOB" => crate::class::FLAG_FLOATBOB,
        "DONTDRAW" | "INVISIBLE" => crate::class::FLAG_DONTDRAW,
        "FRIGHTENED" => crate::class::FLAG_FRIGHTENED,
        "NOTRIGGER" => crate::class::FLAG_NOTRIGGER,
        "DONTRIP" => crate::class::FLAG_DONTRIP,
        "RIPPER" => crate::class::FLAG_RIPPER,
        _ => 0,
    };
    if bit != 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            if set {
                a.flags |= bit;
            } else {
                a.flags &= !bit;
            }
            if bit == crate::class::FLAG_FRIGHTENED {
                a.frightened = set;
            }
        }
    }
    ActionResult::Continue
}

/// `A_NoBlocking` / `A_Fall`: clear SOLID so corpses are walk-through.
/// (Doom `A_Fall` — height already shrunk in `P_KillMobj` / `die`.)
fn a_no_blocking(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        a.flags &= !crate::class::FLAG_SOLID;
    }
    ActionResult::Continue
}

/// `A_SetDamageType("Type")` — set inflictor damage type (empty clears).
fn a_set_damage_type(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.damage_type = name;
    }
    ActionResult::Continue
}

/// `A_SetDamage(value)` — runtime projectile/melee damage override.
fn a_set_damage(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let value = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.damage = value.max(0);
    }
    ActionResult::Continue
}

/// `A_SetBloodColor(r,g,b|"Name")` — runtime blood tint (`None` clears to default).
fn a_set_blood_color(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let cleaned: Vec<String> = args
        .iter()
        .map(|s| s.trim_matches('"').to_owned())
        .collect();
    let rgb = if cleaned.len() == 1 && cleaned[0].eq_ignore_ascii_case("none") {
        None
    } else {
        crate::class::parse_blood_color(&cleaned)
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.blood_color = rgb;
    }
    ActionResult::Continue
}

/// `A_SetFillColor(r,g,b|"Name")` — runtime stencil/fill colour (`None` clears).
fn a_set_fill_color(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let cleaned: Vec<String> = args
        .iter()
        .map(|s| s.trim_matches('"').to_owned())
        .collect();
    let rgb = if cleaned.len() == 1 && cleaned[0].eq_ignore_ascii_case("none") {
        None
    } else {
        crate::class::parse_blood_color(&cleaned)
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.fill_color = rgb;
    }
    ActionResult::Continue
}

/// `A_SetScore(value)` — absolute score.
fn a_set_score(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let value = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.score = value;
    }
    ActionResult::Continue
}

/// `A_GiveScore(amount)` — add to score.
fn a_give_score(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let amount = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.score = a.score.saturating_add(amount);
    }
    ActionResult::Continue
}

/// `A_RadiusGive("Item", distance, flags, amount)` — grant inventory to nearby actors.
fn a_radius_give(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let distance = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let amount = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .or_else(|| {
            args.get(2)
                .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        })
        .unwrap_or(1);
    if item.is_empty() || distance <= 0.0 || amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    let Some(origin) = world.actors.get(self_id).map(|a| a.pos) else {
        return ActionResult::Continue;
    };
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == self_id {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        if other.destroyed || other.health <= 0 {
            continue;
        }
        let dx = other.pos.x() - origin.x();
        let dy = other.pos.y() - origin.y();
        if dmath::sqrt(dx * dx + dy * dy) > distance {
            continue;
        }
        let _ = crate::inventory::give_inventory(world, id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_SetSpecial(special [, arg0..arg4])` — set thing special and optional args.
fn a_set_special(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let special = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<u16>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.special = special;
        for (i, slot) in a.args.iter_mut().enumerate() {
            if let Some(v) = args
                .get(i + 1)
                .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
            {
                *slot = v;
            }
        }
    }
    ActionResult::Continue
}

/// `A_SetArg(index, value)` — set one thing arg (`0..=4`).
fn a_set_arg(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let index = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<usize>().ok())
        .unwrap_or(0);
    let value = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if index < 5 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.args[index] = value;
        }
    }
    ActionResult::Continue
}

/// `A_SetTid` / `A_ChangeTID(tid)` — set Hexen/UDMF thing ID.
fn a_set_tid(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let tid = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.tid = tid;
    }
    ActionResult::Continue
}

/// `A_CallSpecial(special [, arg0..arg4])` — run a line special as this actor.
fn a_call_special(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let special = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<u16>().ok())
        .unwrap_or(0);
    if special == 0 {
        return ActionResult::Continue;
    }
    let mut sp_args = [0i32; 5];
    for (i, slot) in sp_args.iter_mut().enumerate() {
        if let Some(v) = args
            .get(i + 1)
            .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        {
            *slot = v;
        }
    }
    let _ = crate::specials::execute_special(world, special, &sp_args, Some(self_id), None);
    ActionResult::Continue
}

/// Read a named actor pointer (`Target` / `Master` / `Tracer`).
fn get_pointer(a: &crate::actor::Actor, name: &str) -> Option<ActorId> {
    match name.trim_matches('"').to_ascii_lowercase().as_str() {
        "target" | "aaptr_target" => a.target,
        "master" | "aaptr_master" => a.master,
        "tracer" | "aaptr_tracer" => a.tracer,
        _ => None,
    }
}

/// Write a named actor pointer.
fn set_pointer(a: &mut crate::actor::Actor, name: &str, value: Option<ActorId>) {
    match name.trim_matches('"').to_ascii_lowercase().as_str() {
        "target" | "aaptr_target" => a.target = value,
        "master" | "aaptr_master" => a.master = value,
        "tracer" | "aaptr_tracer" => a.tracer = value,
        _ => {}
    }
}

/// `A_TransferPointer(srcPtr, destPtr)` — copy one of self's pointers to another.
fn a_transfer_pointer(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let src = args.first().map(|s| s.as_str()).unwrap_or("");
    let dest = args.get(1).map(|s| s.as_str()).unwrap_or("");
    if src.is_empty() || dest.is_empty() {
        return ActionResult::Continue;
    }
    let value = world.actors.get(self_id).and_then(|a| get_pointer(a, src));
    if let Some(a) = world.actors.get_mut(self_id) {
        set_pointer(a, dest, value);
    }
    ActionResult::Continue
}

/// Resolve rearrange source: `null`/`default`/`target`/`master`/`tracer`.
/// `None` = keep current; `Some(v)` = assign `v`.
fn rearrange_value(a: &Actor, token: &str) -> Option<Option<ActorId>> {
    match token.trim_matches('"').to_ascii_lowercase().as_str() {
        "" | "default" | "aaptr_default" => None,
        "null" | "none" | "aaptr_null" => Some(None),
        "target" | "aaptr_target" => Some(a.target),
        "master" | "aaptr_master" => Some(a.master),
        "tracer" | "aaptr_tracer" => Some(a.tracer),
        _ => None,
    }
}

/// `A_RearrangePointers(ptrT, ptrM, ptrR [, flags])` — assign Target/Master/Tracer
/// from named sources (`null` clears; `default` keeps; `self` → self).
fn a_rearrange_pointers(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let resolve = |tok: &str| -> Option<Option<ActorId>> {
        let key = tok.trim_matches('"').to_ascii_lowercase();
        if key == "self" || key == "aaptr_self" {
            return Some(Some(self_id));
        }
        rearrange_value(&a, tok)
    };
    let new_t = args.first().map(|s| s.as_str()).and_then(resolve);
    let new_m = args.get(1).map(|s| s.as_str()).and_then(resolve);
    let new_r = args.get(2).map(|s| s.as_str()).and_then(resolve);
    if let Some(m) = world.actors.get_mut(self_id) {
        if let Some(v) = new_t {
            m.target = v;
        }
        if let Some(v) = new_m {
            m.master = v;
        }
        if let Some(v) = new_r {
            m.tracer = v;
        }
    }
    ActionResult::Continue
}

/// Resolve warp destination: pointer name, or numeric TID.
fn warp_destination(world: &World, self_id: ActorId, dest: &str) -> Option<ActorId> {
    let key = dest.trim_matches('"');
    if let Ok(tid) = key.parse::<i32>() {
        return world.find_by_tid(tid);
    }
    world.actors.get(self_id).and_then(|a| get_pointer(a, key))
}

/// `A_Teleport([state [, targettype [, fogtype [, flags]]]])` — warp to a spot
/// of `targettype` (default `TeleportDest`); optional fog + state jump.
fn a_teleport(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let state_label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let target_type = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "TeleportDest".into());
    let fog_type = args
        .get(2)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let flags = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    // TF_FORCED = 4 — keep velocity (bootstrap: still zero unless set).
    let keep_vel = flags & 4 != 0;
    let Some(dest_class) = world.classes.find_name(&target_type) else {
        return ActionResult::Continue;
    };
    let dest = world.thinkers.default.iter().copied().find(|&id| {
        world
            .actors
            .get(id)
            .is_some_and(|a| !a.destroyed && a.class == dest_class && id != self_id)
    });
    let Some(dest_id) = dest else {
        return ActionResult::Continue;
    };
    let Some(spot) = world.actors.get(dest_id).cloned() else {
        return ActionResult::Continue;
    };
    let old = world.actors.get(self_id).map(|a| a.pos).unwrap_or(spot.pos);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.pos = spot.pos;
        a.angle = spot.angle;
        if !keep_vel {
            a.vel = DVec3::ZERO;
        }
        a.reaction_time = 18;
    }
    // Fog at old and new (unless TF_NOFOG / empty fogtype "").
    let no_fog = flags & 1 != 0 || fog_type.as_deref() == Some("");
    if !no_fog {
        if let Some(ref fog) = fog_type {
            if let Some(fc) = world.classes.find_name(fog) {
                let _ = world.spawn_at(fc, old, Angle::ZERO);
                let _ = world.spawn_at(fc, spot.pos, Angle::ZERO);
            }
        } else {
            crate::specials::spawn_teleport_fog(world, old);
            crate::specials::spawn_teleport_fog(world, spot.pos);
        }
    }
    if let Some(label) = state_label {
        jump_to_label(world, self_id, &label)
    } else {
        ActionResult::Continue
    }
}

/// `A_SelectWeapon("Weapon")` — queue a weapon change on the owning player.
fn a_select_weapon(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if name.is_empty() {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&name) else {
        return ActionResult::Continue;
    };
    let Some(pi) = world
        .players
        .iter()
        .position(|p| p.as_ref().is_some_and(|pl| pl.mo == Some(self_id)))
    else {
        return ActionResult::Continue;
    };
    if world.inventory.count(self_id, cid) <= 0 {
        return ActionResult::Continue;
    }
    if let Some(mut player) = world.players[pi].take() {
        crate::think_player::request_weapon_change(world, &mut player, self_id, cid);
        world.players[pi] = Some(player);
    }
    ActionResult::Continue
}

/// `A_Warp(dest, xofs, yofs, zofs [, angle])` — move relative to a pointer or TID.
fn a_warp(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dest_name = args.first().map(|s| s.as_str()).unwrap_or("Target");
    let Some(dest_id) = warp_destination(world, self_id, dest_name) else {
        return ActionResult::Continue;
    };
    if dest_id == self_id {
        return ActionResult::Continue;
    }
    let ox = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let oy = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let oz = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let ang_off = args
        .get(4)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok());
    let Some(dest) = world.actors.get(dest_id).cloned() else {
        return ActionResult::Continue;
    };
    let rad = dest.angle.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let pos = DVec3::new(
        dest.pos.x() + ox * c - oy * s,
        dest.pos.y() + ox * s + oy * c,
        dest.pos.z() + oz,
    );
    let ang = ang_off.map_or(dest.angle, |d| {
        Angle::from_degrees(dest.angle.degrees() + d)
    });
    if let Some(a) = world.actors.get_mut(self_id) {
        a.pos = pos;
        a.angle = ang;
        a.vel = DVec3::ZERO;
    }
    ActionResult::Continue
}

/// `A_Look`: wake on sector `soundtarget` or a visible foe.
fn a_look(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    // Doom: `actor->threshold = 0` — any shot wakes.
    if let Some(a) = world.actors.get_mut(self_id) {
        a.threshold = 0;
    }
    let friendly = (self_actor.flags & crate::class::FLAG_FRIENDLY) != 0;
    // Friends: `FriendlySeeBlocks` × 128 map units (GZDoom default 10 → 1280).
    let sight_range = if friendly {
        f64::from(self_actor.friendly_see_blocks.max(0)) * 128.0
    } else {
        2048.0
    };
    let mut found: Option<ActorId> = None;
    // Doom: prefer `sector->soundtarget` from `P_NoiseAlert` flood.
    if !friendly {
        if let Some(sec) =
            room_nodebuild::point_in_sector(&world.map, self_actor.pos.x(), self_actor.pos.y())
        {
            let targ = world
                .sectors
                .get(sec.raw() as usize)
                .and_then(|s| s.soundtarget);
            if let Some(tid) = targ {
                if let Some(t) = world.actors.get(tid) {
                    let shootable = (t.flags
                        | world.classes.get(t.class).map(|c| c.flags).unwrap_or(0))
                        & crate::class::FLAG_SHOOTABLE
                        != 0;
                    if !t.destroyed && t.health > 0 && shootable {
                        let ambush = (self_actor.flags & crate::class::FLAG_AMBUSH) != 0;
                        if !ambush || crate::sight::check_sight(world, self_id, tid) {
                            found = Some(tid);
                        }
                    }
                }
            }
        }
    }
    if found.is_none() {
        if friendly {
            for &mo in &world.thinkers.default {
                if mo == self_id || !is_enemy_monster(world, mo) {
                    continue;
                }
                let Some(other) = world.actors.get(mo) else {
                    continue;
                };
                let dx = other.pos.x() - self_actor.pos.x();
                let dy = other.pos.y() - self_actor.pos.y();
                if dx * dx + dy * dy > sight_range * sight_range {
                    continue;
                }
                if crate::sight::check_sight(world, self_id, mo) {
                    found = Some(mo);
                    break;
                }
            }
        } else {
            for slot in &world.players {
                let Some(p) = slot else {
                    continue;
                };
                let Some(mo) = p.mo else {
                    continue;
                };
                if mo == self_id {
                    continue;
                }
                let Some(other) = world.actors.get(mo) else {
                    continue;
                };
                if (other.flags & crate::class::FLAG_NOTARGET) != 0 {
                    continue;
                }
                let dx = other.pos.x() - self_actor.pos.x();
                let dy = other.pos.y() - self_actor.pos.y();
                if dx * dx + dy * dy > sight_range * sight_range {
                    continue;
                }
                if crate::sight::check_sight(world, self_id, mo) {
                    found = Some(mo);
                    break;
                }
            }
        }
    }
    let Some(tid) = found else {
        return ActionResult::Continue;
    };
    if std::env::var_os("ROOM_TRACE_WAKE").is_some() {
        let cname = world
            .classes
            .get(self_actor.class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        eprintln!(
            "WAKE@{} {} ({:.0},{:.0}) → target sight ambush={}",
            world.level.time,
            cname,
            self_actor.pos.x(),
            self_actor.pos.y(),
            (self_actor.flags & crate::class::FLAG_AMBUSH) != 0
        );
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.target = Some(tid);
    }
    let sound = world
        .classes
        .get(self_actor.class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "misc/awakening".into());
    // Doom `A_Look`: `sfx_posit*` → `P_Random()%3`, `sfx_bgsit*` → `P_Random()%2`.
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed && !sound.is_empty() {
        let s = sound.to_ascii_lowercase();
        if s.contains("grunt/sight")
            || s.contains("shotguy/sight")
            || s.contains("chainguy/sight")
            || s.contains("wolfss/sight")
        {
            let _ = world.p_random("pr_seesound") % 3;
        } else if s.contains("imp/sight") {
            let _ = world.p_random("pr_seesound") % 2;
        }
    }
    // Vanilla: Spider / Cyberdemon see sound is full volume.
    let attn = if (self_actor.flags & crate::class::FLAG_BOSS) != 0 {
        0.0
    } else {
        1.0
    };
    world.play_sound_attn(
        &sound,
        self_actor.pos.x(),
        self_actor.pos.y(),
        self_actor.pos.z(),
        1.0,
        attn,
    );
    if let Some(see) = world.states.find_label(self_actor.class, "See") {
        ActionResult::Jump(see)
    } else {
        ActionResult::Continue
    }
}

/// True when `other` lies within `fov` degrees of `self`'s yaw (`fov <= 0` → full circle).
fn within_fov(self_a: &Actor, other: &Actor, fov: f64) -> bool {
    if fov <= 0.0 || fov >= 360.0 {
        return true;
    }
    let dx = other.pos.x() - self_a.pos.x();
    let dy = other.pos.y() - self_a.pos.y();
    let bearing = Angle::from_radians(dmath::atan2(dy, dx)).degrees();
    let mut delta = (bearing - self_a.angle.degrees()).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    delta.abs() <= fov * 0.5
}

/// `A_LookEx(flags, min, max, maxhear, fov [, sound])` — ranged/FOV wake (ZDoom).
fn a_look_ex(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let flags = parse_i32_arg(args, 0, 0);
    let min_dist = parse_f64_arg(args, 1, 0.0).max(0.0);
    let max_dist = {
        let m = parse_f64_arg(args, 2, 0.0);
        if m <= 0.0 { 2048.0 } else { m }
    };
    let fov = parse_f64_arg(args, 4, 0.0).abs();
    let custom_sound = args
        .get(5)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    // LOF_NOSIGHTCHECK=1, LOF_NOSOUNDCHECK=2, LOF_NOSEESOUND=8, LOF_NOJUMP=32
    let no_sight = flags & 1 != 0;
    let no_sound = flags & 2 != 0;
    let no_see_sound = flags & 8 != 0;
    let no_jump = flags & 32 != 0;
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.threshold = 0;
    }
    let friendly = (self_actor.flags & crate::class::FLAG_FRIENDLY) != 0;
    let mut found: Option<ActorId> = None;
    if !no_sound && !friendly {
        if let Some(sec) =
            room_nodebuild::point_in_sector(&world.map, self_actor.pos.x(), self_actor.pos.y())
        {
            let targ = world
                .sectors
                .get(sec.raw() as usize)
                .and_then(|s| s.soundtarget);
            if let Some(tid) = targ {
                if target_is_valid(world, tid) {
                    if let Some(t) = world.actors.get(tid) {
                        let dx = t.pos.x() - self_actor.pos.x();
                        let dy = t.pos.y() - self_actor.pos.y();
                        let d = dmath::sqrt(dx * dx + dy * dy);
                        if d >= min_dist && d <= max_dist && within_fov(&self_actor, t, fov) {
                            let ambush = (self_actor.flags & crate::class::FLAG_AMBUSH) != 0;
                            if no_sight || !ambush || crate::sight::check_sight(world, self_id, tid)
                            {
                                found = Some(tid);
                            }
                        }
                    }
                }
            }
        }
    }
    if found.is_none() {
        let candidates: Vec<ActorId> = if friendly {
            world
                .thinkers
                .default
                .iter()
                .copied()
                .filter(|&mo| mo != self_id && is_enemy_monster(world, mo))
                .collect()
        } else {
            world
                .players
                .iter()
                .filter_map(|p| p.as_ref().and_then(|pl| pl.mo))
                .filter(|&mo| mo != self_id)
                .collect()
        };
        for mo in candidates {
            let Some(other) = world.actors.get(mo) else {
                continue;
            };
            if other.destroyed || other.health <= 0 {
                continue;
            }
            if (other.flags & crate::class::FLAG_NOTARGET) != 0 {
                continue;
            }
            let dx = other.pos.x() - self_actor.pos.x();
            let dy = other.pos.y() - self_actor.pos.y();
            let d = dmath::sqrt(dx * dx + dy * dy);
            if d < min_dist || d > max_dist || !within_fov(&self_actor, other, fov) {
                continue;
            }
            if !no_sight && !crate::sight::check_sight(world, self_id, mo) {
                continue;
            }
            found = Some(mo);
            break;
        }
    }
    let Some(tid) = found else {
        return ActionResult::Continue;
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.target = Some(tid);
    }
    if !no_see_sound {
        let sound = custom_sound.unwrap_or_else(|| {
            world
                .classes
                .get(self_actor.class)
                .map(|c| c.see_sound.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "misc/awakening".into())
        });
        let attn = if flags & 16 != 0 || (self_actor.flags & crate::class::FLAG_BOSS) != 0 {
            0.0
        } else {
            1.0
        };
        world.play_sound_attn(
            &sound,
            self_actor.pos.x(),
            self_actor.pos.y(),
            self_actor.pos.z(),
            1.0,
            attn,
        );
    }
    if no_jump {
        return ActionResult::Continue;
    }
    if let Some(see) = world.states.find_label(self_actor.class, "See") {
        ActionResult::Jump(see)
    } else {
        ActionResult::Continue
    }
}

/// `A_VileChase`: raise a nearby corpse, else chase like `A_Chase`.
fn a_vile_chase(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(vile) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let raise_range = 48.0;
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == self_id {
            continue;
        }
        let Some(other) = world.actors.get(id).cloned() else {
            continue;
        };
        if other.health > 0 || other.destroyed {
            continue;
        }
        let Some(raise) = world.states.find_label(other.class, "Raise") else {
            continue;
        };
        let dx = other.pos.x() - vile.pos.x();
        let dy = other.pos.y() - vile.pos.y();
        if dx * dx + dy * dy > raise_range * raise_range {
            continue;
        }
        let spawn_hp = world
            .classes
            .get(other.class)
            .map(|c| c.health.max(1))
            .unwrap_or(1);
        let flags = world.classes.get(other.class).map(|c| c.flags).unwrap_or(0);
        if (flags & crate::class::FLAG_COUNTKILL) != 0 {
            world.level.found_kills = world.level.found_kills.saturating_sub(1);
        }
        if let Some(a) = world.actors.get_mut(id) {
            a.health = spawn_hp;
            a.destroyed = false;
            a.flags = flags;
            a.vel = DVec3::ZERO;
            a.reaction_time = 0;
        }
        world.set_state(id, Some(raise), true);
        world.play_sound("vile/raise", vile.pos.x(), vile.pos.y(), vile.pos.z(), 1.0);
        if let Some(heal) = world.states.find_label(vile.class, "Heal") {
            return ActionResult::Jump(heal);
        }
        return ActionResult::Continue;
    }
    a_chase(world, self_id)
}

/// Doom `xspeed` / `yspeed` (`p_enemy.c`) as 16.16 fixed ints.
const CHASE_XSPEED: [i32; 8] = [65536, 47000, 0, -47000, -65536, -47000, 0, 47000];
const CHASE_YSPEED: [i32; 8] = [0, 47000, 65536, 47000, 0, -47000, -65536, -47000];
/// Float form for non-vanilla math (`47000/65536` ≈ 0.717 on diagonals).
const CHASE_DIR_X: [f64; 8] = [
    1.0,
    47000.0 / 65536.0,
    0.0,
    -47000.0 / 65536.0,
    -1.0,
    -47000.0 / 65536.0,
    0.0,
    47000.0 / 65536.0,
];
const CHASE_DIR_Y: [f64; 8] = [
    0.0,
    47000.0 / 65536.0,
    1.0,
    47000.0 / 65536.0,
    0.0,
    -47000.0 / 65536.0,
    -1.0,
    -47000.0 / 65536.0,
];

/// Doom `P_CheckMissileRange`.
fn check_missile_range(world: &mut World, self_id: ActorId, tid: ActorId) -> bool {
    use crate::class::FLAG_JUSTHIT;
    use crate::sight::check_sight;
    use crate::vanilla::approx_distance;
    use room_core::{FRACBITS, FRACUNIT, Fixed};

    if !check_sight(world, self_id, tid) {
        return false;
    }
    let just_hit = world
        .actors
        .get(self_id)
        .is_some_and(|a| (a.flags & FLAG_JUSTHIT) != 0);
    if just_hit {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.flags &= !FLAG_JUSTHIT;
        }
        return true;
    }
    if world
        .actors
        .get(self_id)
        .is_some_and(|a| a.reaction_time > 0)
    {
        return false;
    }
    let (sx, sy, tx, ty, class) = {
        let Some(s) = world.actors.get(self_id) else {
            return false;
        };
        let Some(t) = world.actors.get(tid) else {
            return false;
        };
        (s.pos.x(), s.pos.y(), t.pos.x(), t.pos.y(), s.class)
    };
    let mut dist = approx_distance(Fixed::from_f64(sx - tx), Fixed::from_f64(sy - ty));
    dist = Fixed(dist.0 - 64 * FRACUNIT);
    // No melee state → fire more often (vanilla −128 map units).
    if world.states.find_label(class, "Melee").is_none() {
        dist = Fixed(dist.0 - 128 * FRACUNIT);
    }
    let mut dist_i = dist.0 >> FRACBITS;
    let name = world
        .classes
        .get(class)
        .map(|c| c.name.to_ascii_lowercase())
        .unwrap_or_default();
    if (name.contains("archvile") || name == "vile") && dist_i > 14 * 64 {
        return false;
    }
    if name.contains("revenant") || name == "undead" {
        if dist_i < 196 {
            return false;
        }
        dist_i >>= 1;
    }
    if name.contains("cyberdemon")
        || name.contains("spidermastermind")
        || name.contains("spiderdemon")
        || name.contains("lostsoul")
        || name == "skull"
    {
        dist_i >>= 1;
    }
    if dist_i > 200 {
        dist_i = 200;
    }
    if name.contains("cyberdemon") && dist_i > 160 {
        dist_i = 160;
    }
    if i32::from(world.p_random("pr_checkmissilerange")) < dist_i {
        return false;
    }
    true
}

/// `A_Chase`: 8-dir walk toward target (`P_NewChaseDir` / `P_Move`); Melee/Missile by range.
fn a_chase(world: &mut World, self_id: ActorId) -> ActionResult {
    use crate::class::{FLAG_JUSTATTACKED, FLAG_JUSTHIT};
    // Doom: decrement ReactionTime / Threshold but keep walking; missile checks
    // gate on `reactiontime` inside `P_CheckMissileRange` only.
    if let Some(a) = world.actors.get_mut(self_id) {
        if a.reaction_time > 0 {
            a.reaction_time -= 1;
        }
        if a.threshold > 0 {
            // Vanilla 1.9+: clear threshold if target gone; bootstrap always ticks down.
            a.threshold -= 1;
        }
    }
    // Doom: do not attack twice in a row.
    let just_attacked = world
        .actors
        .get(self_id)
        .is_some_and(|a| (a.flags & FLAG_JUSTATTACKED) != 0);
    if just_attacked {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.flags &= !FLAG_JUSTATTACKED;
        }
        if !world.fast_monsters {
            if let Some(tid) = world.actors.get(self_id).and_then(|a| a.target) {
                new_chase_dir(world, self_id, tid);
            }
        }
        return ActionResult::Continue;
    }
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    let attack = friendly_should_attack(world, self_id, tid);
    let (sx, sy, sz, tx, ty, tz, class, flags, move_count) = {
        let Some(s) = world.actors.get(self_id) else {
            return ActionResult::Continue;
        };
        let Some(t) = world.actors.get(tid) else {
            return ActionResult::Continue;
        };
        (
            s.pos.x(),
            s.pos.y(),
            s.pos.z(),
            t.pos.x(),
            t.pos.y(),
            t.pos.z(),
            s.class,
            s.flags,
            s.move_count,
        )
    };
    let dx = tx - sx;
    let dy = ty - sy;
    let len = dmath::sqrt(dx * dx + dy * dy);
    // Keep a small follow distance when trailing the player (MBF friends).
    let follow_keep = 96.0;
    if !attack && len < follow_keep {
        return ActionResult::Continue;
    }
    // Doom `P_CheckMeleeRange` (1.5+): `MELEERANGE-20+target->radius`, `P_AproxDistance`, sight.
    if attack
        && world.states.find_label(class, "Melee").is_some()
        && check_melee_range(world, self_id, tid)
    {
        if let Some(melee) = world.states.find_label(class, "Melee") {
            // Vanilla plays AttackSound when entering melee from chase.
            let atk = world
                .classes
                .get(class)
                .map(|c| c.attack_sound.clone())
                .filter(|s| !s.is_empty());
            if let Some(sound) = atk {
                world.play_sound(&sound, sx, sy, sz, 1.0);
            }
            return ActionResult::Jump(melee);
        }
    }
    // Doom: `if (actor->info->missilestate)` — no `P_CheckMissileRange` without one
    // (Demon/Spectre have Melee only; burning RNG here desynced DEMO1 @1727).
    let missile_state = world.states.find_label(class, "Missile");
    // Doom: skip missile while `movecount != 0` (unless nightmare/fast).
    let can_missile = world.fast_monsters || move_count == 0;
    // GZDoom `MaxTargetRange` hard cap (0 = unlimited).
    let max_range = world
        .classes
        .get(class)
        .map(|c| c.max_target_range)
        .unwrap_or(0.0);
    let in_max_range = max_range <= 0.0 || len <= max_range;
    // Doom `P_CheckMissileRange` when allowed this tic.
    if attack
        && missile_state.is_some()
        && can_missile
        && in_max_range
        && check_missile_range(world, self_id, tid)
    {
        let melee_threshold = world
            .classes
            .get(class)
            .map(|c| c.melee_threshold)
            .unwrap_or(0.0);
        if melee_threshold > 0.0 && len < melee_threshold {
            if let Some(melee) = world.states.find_label(class, "Melee") {
                return ActionResult::Jump(melee);
            }
        }
        // GZDoom `MinMissileChance` (default 200 = vanilla: no extra RNG).
        // Non-default: 256 never fires; else require `pr >= chance` (do not touch
        // the vanilla `P_Random` stream when chance is 200).
        let chance = world
            .classes
            .get(class)
            .map(|c| c.min_missile_chance)
            .unwrap_or(200)
            .clamp(0, 256);
        let chance_ok = if chance == 200 {
            true
        } else if chance >= 256 {
            false
        } else {
            i32::from(world.p_random("pr_trywalk")) >= chance
        };
        if chance_ok {
            if let Some(missile) = missile_state {
                if let Some(a) = world.actors.get_mut(self_id) {
                    a.flags |= FLAG_JUSTATTACKED;
                    a.flags &= !FLAG_JUSTHIT;
                }
                return ActionResult::Jump(missile);
            }
        }
    }
    if len < 1.0 {
        return ActionResult::Continue;
    }
    // Float height adjust (Doom `P_Move` MF_FLOAT subset) toward target Z.
    if (flags & crate::class::FLAG_FLOAT) != 0 {
        let float_speed = world
            .actors
            .get(self_id)
            .map(|a| {
                if a.float_speed > 0.0 {
                    a.float_speed
                } else {
                    world
                        .classes
                        .get(class)
                        .map(|c| c.float_speed)
                        .filter(|&s| s > 0.0)
                        .unwrap_or(4.0)
                }
            })
            .unwrap_or(4.0);
        if let Some(a) = world.actors.get_mut(self_id) {
            let dz = tz - sz;
            if dz > float_speed {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + float_speed);
            } else if dz < -float_speed {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() - float_speed);
            } else {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + dz);
            }
        }
    }

    // Doom: `if (--movecount < 0 || !P_Move) P_NewChaseDir` — before ActiveSound.
    let mc = world
        .actors
        .get_mut(self_id)
        .map(|a| {
            a.move_count -= 1;
            a.move_count
        })
        .unwrap_or(-1);
    let moved = mc >= 0 && chase_move(world, self_id);
    if std::env::var_os("ROOM_TRACE_CHASEMOVE").is_some()
        && world.level.time >= 1705
        && world.level.time <= 1715
    {
        let (name, px, py, md) = world
            .actors
            .get(self_id)
            .map(|a| {
                let n = world
                    .classes
                    .get(a.class)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                (n.to_string(), a.pos.x(), a.pos.y(), a.move_dir)
            })
            .unwrap_or_else(|| ("?".into(), 0.0, 0.0, -1));
        eprintln!(
            "CHASEMOVE@{} {} ({:.0},{:.0}) mc={} md={} moved={} →{}",
            world.level.time,
            name,
            px,
            py,
            mc,
            md,
            moved,
            if mc < 0 || !moved {
                "NewChaseDir"
            } else {
                "ok"
            }
        );
    }
    if mc < 0 || !moved {
        new_chase_dir(world, self_id, tid);
    }
    // Doom keeps damage-thrust `momxy` through chase (applied next tic in
    // `P_XYMovement` before `A_Chase`). Only sync facing to `movedir`.
    if let Some(a) = world.actors.get_mut(self_id) {
        let dir = a.move_dir;
        if (0..=7).contains(&dir) {
            a.angle = Angle::from_degrees(f64::from(dir) * 45.0);
        }
    }
    // Doom: `if (activesound && P_Random() < 3)` — short-circuit: no roll without sound.
    let sound = world
        .classes
        .get(class)
        .map(|c| c.active_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(sound) = sound {
        if i32::from(world.p_random("pr_chase")) < 3 {
            if let Some(a) = world.actors.get(self_id) {
                world.play_sound(&sound, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
        }
    }
    ActionResult::Continue
}

/// Doom `P_Move`: step one chase-dir cell via `P_TryMove`.
fn chase_move(world: &mut World, self_id: ActorId) -> bool {
    let Some(a) = world.actors.get(self_id).cloned() else {
        return false;
    };
    let dir = a.move_dir;
    if !(0..=7).contains(&dir) {
        return false;
    }
    let mut speed = if a.speed > 0.0 {
        a.speed
    } else {
        world
            .classes
            .get(a.class)
            .map(|c| c.speed)
            .filter(|&s| s > 0.0)
            .unwrap_or(8.0)
    };
    if world.fast_monsters {
        let fast = if a.fast_speed > 0.0 {
            Some(a.fast_speed)
        } else {
            world
                .classes
                .get(a.class)
                .map(|c| c.fast_speed)
                .filter(|&s| s > 0.0)
        };
        if let Some(fast) = fast {
            speed = fast;
        }
    }
    let dir = dir as usize;
    // Doom: `tryx = actor->x + actor->info->speed*xspeed[movedir]` (int×fixed).
    let dest = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        let speed_i = speed.round() as i32;
        let x = room_core::Fixed::from_f64(a.pos.x())
            .0
            .wrapping_add(speed_i.wrapping_mul(CHASE_XSPEED[dir]));
        let y = room_core::Fixed::from_f64(a.pos.y())
            .0
            .wrapping_add(speed_i.wrapping_mul(CHASE_YSPEED[dir]));
        DVec3::new(
            room_core::Fixed(x).to_f64(),
            room_core::Fixed(y).to_f64(),
            a.pos.z(),
        )
    } else {
        DVec3::new(
            a.pos.x() + CHASE_DIR_X[dir] * speed,
            a.pos.y() + CHASE_DIR_Y[dir] * speed,
            a.pos.z(),
        )
    };
    let ok = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        crate::try_move_fixed(world, self_id, dest).ok
    } else {
        crate::physics::try_move(world, self_id, dest)
    };
    if !ok {
        if (a.flags & crate::class::FLAG_CANUSEWALLS) != 0 {
            let _ = crate::use_lines::use_lines(world, self_id);
        }
        // Doom sets `movedir = DI_NODIR` only in the spechit path; keep dir for retry.
        return false;
    }
    if (a.flags & crate::class::FLAG_FLOAT) == 0 {
        // Doom `P_Move`: `actor->z = actor->floorz` from the last `P_CheckPosition`.
        let Some(moved) = world.actors.get(self_id).cloned() else {
            return true;
        };
        let floor = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
            crate::vanilla::check_position_fixed(world, &moved, moved.pos, Some(self_id)).floor_z
        } else {
            crate::physics::check_position(world, &moved, moved.pos, Some(self_id)).floor_z
        };
        if let Some(actor) = world.actors.get_mut(self_id) {
            actor.pos = DVec3::new(actor.pos.x(), actor.pos.y(), floor);
        }
    }
    true
}

/// Doom `P_TryWalk`.
fn try_walk(world: &mut World, self_id: ActorId) -> bool {
    if !chase_move(world, self_id) {
        return false;
    }
    let mc = i32::from(world.p_random("pr_trywalk")) & 15;
    if let Some(a) = world.actors.get_mut(self_id) {
        a.move_count = mc;
    }
    true
}

/// Vanilla `P_NewChaseDir` (TryWalk-immediate, Doom RNG order).
fn new_chase_dir(world: &mut World, self_id: ActorId, target: ActorId) {
    let Some((sx, sy, tx, ty, old_dir)) = world.actors.get(self_id).and_then(|s| {
        let t = world.actors.get(target)?;
        Some((s.pos.x(), s.pos.y(), t.pos.x(), t.pos.y(), s.move_dir))
    }) else {
        return;
    };
    let trace = std::env::var_os("ROOM_TRACE_CHASEMOVE").is_some()
        && world.level.time >= 1705
        && world.level.time <= 1715;
    let idx0 = world.vanilla_rng.prndindex;
    let deltax = tx - sx;
    let deltay = ty - sy;
    let turnaround = if old_dir >= 0 {
        (old_dir + 4) & 7
    } else {
        crate::actor::MOVE_DIR_NONE
    };

    let mut d1: i8 = crate::actor::MOVE_DIR_NONE; // east/west
    let mut d2: i8 = crate::actor::MOVE_DIR_NONE; // north/south
    if deltax > 10.0 {
        d1 = 0;
    } else if deltax < -10.0 {
        d1 = 4;
    }
    if deltay < -10.0 {
        d2 = 6; // south
    } else if deltay > 10.0 {
        d2 = 2; // north
    }

    // Direct diagonal (`diags[((deltay<0)<<1)+(deltax>0)]`).
    if d1 >= 0 && d2 >= 0 {
        let diag = match (d2 == 6, d1 == 0) {
            (false, false) => 3, // NW
            (false, true) => 1,  // NE
            (true, false) => 5,  // SW
            (true, true) => 7,   // SE
        };
        if diag != turnaround {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.move_dir = diag;
            }
            if try_walk(world, self_id) {
                if trace {
                    eprintln!(
                        "NEWCHASE@{} id={:?} path=diag md={} idx {}→{}",
                        world.level.time, self_id, diag, idx0, world.vanilla_rng.prndindex
                    );
                }
                return;
            }
        }
    }

    // Swap axes when `P_Random()>200` or |dy| > |dx|.
    if i32::from(world.p_random("pr_newchasedir")) > 200 || deltay.abs() > deltax.abs() {
        core::mem::swap(&mut d1, &mut d2);
    }
    if d1 == turnaround {
        d1 = crate::actor::MOVE_DIR_NONE;
    }
    if d2 == turnaround {
        d2 = crate::actor::MOVE_DIR_NONE;
    }
    if d1 >= 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.move_dir = d1;
        }
        if try_walk(world, self_id) {
            if trace {
                eprintln!(
                    "NEWCHASE@{} id={:?} path=d1 md={} idx {}→{}",
                    world.level.time, self_id, d1, idx0, world.vanilla_rng.prndindex
                );
            }
            return;
        }
    }
    if d2 >= 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.move_dir = d2;
        }
        if try_walk(world, self_id) {
            if trace {
                eprintln!(
                    "NEWCHASE@{} id={:?} path=d2 md={} idx {}→{}",
                    world.level.time, self_id, d2, idx0, world.vanilla_rng.prndindex
                );
            }
            return;
        }
    }

    // Old direction.
    if old_dir >= 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.move_dir = old_dir;
        }
        if try_walk(world, self_id) {
            if trace {
                eprintln!(
                    "NEWCHASE@{} id={:?} path=old md={} idx {}→{}",
                    world.level.time, self_id, old_dir, idx0, world.vanilla_rng.prndindex
                );
            }
            return;
        }
    }

    // Sweep remaining directions.
    if (world.p_random("pr_newchasedir") & 1) != 0 {
        for tdir in 0i8..=7 {
            if tdir != turnaround {
                if let Some(a) = world.actors.get_mut(self_id) {
                    a.move_dir = tdir;
                }
                if try_walk(world, self_id) {
                    if trace {
                        eprintln!(
                            "NEWCHASE@{} id={:?} path=sweep md={} idx {}→{}",
                            world.level.time, self_id, tdir, idx0, world.vanilla_rng.prndindex
                        );
                    }
                    return;
                }
            }
        }
    } else {
        for tdir in (0i8..=7).rev() {
            if tdir != turnaround {
                if let Some(a) = world.actors.get_mut(self_id) {
                    a.move_dir = tdir;
                }
                if try_walk(world, self_id) {
                    if trace {
                        eprintln!(
                            "NEWCHASE@{} id={:?} path=sweep md={} idx {}→{}",
                            world.level.time, self_id, tdir, idx0, world.vanilla_rng.prndindex
                        );
                    }
                    return;
                }
            }
        }
    }

    if turnaround >= 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.move_dir = turnaround;
        }
        if try_walk(world, self_id) {
            if trace {
                eprintln!(
                    "NEWCHASE@{} id={:?} path=turnaround md={} idx {}→{}",
                    world.level.time, self_id, turnaround, idx0, world.vanilla_rng.prndindex
                );
            }
            return;
        }
    }

    if let Some(a) = world.actors.get_mut(self_id) {
        a.move_dir = crate::actor::MOVE_DIR_NONE;
    }
    if trace {
        eprintln!(
            "NEWCHASE@{} id={:?} path=nodir idx {}→{}",
            world.level.time, self_id, idx0, world.vanilla_rng.prndindex
        );
    }
}

/// `A_FaceTarget`: yaw toward `Actor::target` (else nearest player).
fn a_face_target(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    face_actor(world, self_id, tid)
}

/// `A_FaceMovementDirection` — yaw toward current XY velocity (no-op if still).
fn a_face_movement_direction(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(a) = world.actors.get(self_id) else {
        return ActionResult::Continue;
    };
    let vx = a.vel.x();
    let vy = a.vel.y();
    if vx * vx + vy * vy < 1e-8 {
        return ActionResult::Continue;
    }
    if let Some(m) = world.actors.get_mut(self_id) {
        m.angle = Angle::from_radians(dmath::atan2(vy, vx));
    }
    ActionResult::Continue
}

/// `A_AlertMonsters([maxdist])` — `P_NoiseAlert` with self as emitter; optional range.
fn a_alert_monsters(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let maxdist = parse_f64_arg(args, 0, 0.0).abs();
    let target = world
        .actors
        .get(self_id)
        .and_then(|a| a.target)
        .filter(|&t| target_is_valid(world, t))
        .unwrap_or(self_id);
    noise_alert_ex(world, target, self_id);
    if maxdist > 0.0 {
        // Bootstrap: also stamp chase target onto nearby hostiles within maxdist.
        let origin = world
            .actors
            .get(self_id)
            .map(|a| a.pos)
            .unwrap_or(DVec3::ZERO);
        let r2 = maxdist * maxdist;
        let ids: Vec<ActorId> = world.thinkers.default.clone();
        for id in ids {
            if id == self_id || id == target {
                continue;
            }
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed || a.health <= 0 {
                continue;
            }
            if (a.flags & crate::class::FLAG_FRIENDLY) != 0 {
                continue;
            }
            if (a.flags & crate::class::FLAG_COUNTKILL) == 0
                && (a.flags & crate::class::FLAG_SHOOTABLE) == 0
            {
                continue;
            }
            let dx = a.pos.x() - origin.x();
            let dy = a.pos.y() - origin.y();
            if dx * dx + dy * dy > r2 {
                continue;
            }
            if let Some(m) = world.actors.get_mut(id) {
                if m.target.is_none() {
                    m.target = Some(target);
                }
            }
        }
    }
    ActionResult::Continue
}

#[derive(Clone, Copy)]
enum FacePtr {
    Target,
    Tracer,
    Master,
}

fn resolve_face_ptr(world: &mut World, self_id: ActorId, which: FacePtr) -> Option<ActorId> {
    match which {
        FacePtr::Target => resolve_target(world, self_id),
        FacePtr::Tracer => world.actors.get(self_id).and_then(|a| a.tracer),
        FacePtr::Master => world.actors.get(self_id).and_then(|a| a.master),
    }
}

/// `A_FaceTracer` / `A_FaceMaster` — yaw toward the named pointer.
fn a_face_named(world: &mut World, self_id: ActorId, which: FacePtr) -> ActionResult {
    let Some(tid) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    face_actor(world, self_id, tid)
}

fn parse_item_amount(args: &[String]) -> Option<(String, i32)> {
    let item = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())?;
    let amount = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(1);
    Some((item, amount))
}

/// `A_GiveToTarget("Item" [, amount])`.
fn a_give_to_pointer(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    which: FacePtr,
) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(tid) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    if let Some(cid) = world.classes.find_name(&item) {
        let _ = crate::inventory::give_inventory(world, tid, cid, amount);
    }
    ActionResult::Continue
}

/// `A_GiveToChildren("Item" [, amount])`.
fn a_give_to_children(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    for id in children_of(world, self_id) {
        let _ = crate::inventory::give_inventory(world, id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_TakeFromChildren("Item" [, amount])`.
fn a_take_from_children(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    for id in children_of(world, self_id) {
        let _ = world.inventory.take(id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_TakeFromSiblings("Item" [, amount])`.
fn a_take_from_siblings(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    for id in siblings_of(world, self_id) {
        let _ = world.inventory.take(id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_TakeFromTarget("Item" [, amount])`.
fn a_take_from_pointer(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    which: FacePtr,
) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(tid) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    if let Some(cid) = world.classes.find_name(&item) {
        let _ = world.inventory.take(tid, cid, amount);
    }
    ActionResult::Continue
}

/// `A_DamageSelf(damage [, "mod"])` — hurt the calling actor.
fn a_damage_self(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dmg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if dmg <= 0 {
        return ActionResult::Continue;
    }
    let mod_name = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let prev = if let Some(ref m) = mod_name {
        world.actors.get_mut(self_id).map(|a| {
            let old = std::mem::replace(&mut a.damage_type, m.clone());
            old
        })
    } else {
        None
    };
    crate::damage::damage_mobj(world, self_id, Some(self_id), Some(self_id), dmg);
    if let Some(old) = prev {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.damage_type = old;
        }
    }
    ActionResult::Continue
}

/// `A_DamageTarget` / `Master` / `Tracer(damage [, "mod"])`.
fn a_damage_pointer(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
    which: FacePtr,
) -> ActionResult {
    let dmg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if dmg <= 0 {
        return ActionResult::Continue;
    }
    let Some(tid) = resolve_face_ptr(world, self_id, which) else {
        return ActionResult::Continue;
    };
    crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), dmg);
    ActionResult::Continue
}

/// `A_DamageChildren(damage [, "mod"])`.
fn a_damage_children(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dmg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if dmg <= 0 {
        return ActionResult::Continue;
    }
    for id in children_of(world, self_id) {
        crate::damage::damage_mobj(world, id, Some(self_id), Some(self_id), dmg);
    }
    ActionResult::Continue
}

fn face_actor(world: &mut World, self_id: ActorId, tid: ActorId) -> ActionResult {
    let (sx, sy, tx, ty) = {
        let Some(s) = world.actors.get(self_id) else {
            return ActionResult::Continue;
        };
        let Some(t) = world.actors.get(tid) else {
            return ActionResult::Continue;
        };
        (s.pos.x(), s.pos.y(), t.pos.x(), t.pos.y())
    };
    if let Some(a) = world.actors.get_mut(self_id) {
        a.angle = Angle::from_radians(dmath::atan2(ty - sy, tx - sx));
    }
    ActionResult::Continue
}

fn children_of(world: &World, master: ActorId) -> Vec<ActorId> {
    world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.master == Some(master))
        })
        .collect()
}

/// Siblings share the same `master` (excluding self).
fn siblings_of(world: &World, self_id: ActorId) -> Vec<ActorId> {
    let Some(master) = world.actors.get(self_id).and_then(|a| a.master) else {
        return Vec::new();
    };
    world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            id != self_id
                && world
                    .actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && a.master == Some(master))
        })
        .collect()
}

/// `A_SetInvisible` / `A_UnsetInvisible` — toggle `+DONTDRAW`.
fn a_set_invisible(world: &mut World, self_id: ActorId, invisible: bool) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        if invisible {
            a.flags |= crate::class::FLAG_DONTDRAW;
        } else {
            a.flags &= !crate::class::FLAG_DONTDRAW;
        }
    }
    ActionResult::Continue
}

/// Revive a corpse into its `Raise` state (Thing_Raise / A_Raise*).
fn try_raise_actor(world: &mut World, id: ActorId) -> bool {
    let Some(a) = world.actors.get(id).cloned() else {
        return false;
    };
    if a.destroyed || (a.health > 0 && (a.flags & crate::class::FLAG_CORPSE) == 0) {
        return false;
    }
    let Some(raise) = world.states.find_label(a.class, "Raise") else {
        return false;
    };
    let spawn_hp = world
        .classes
        .get(a.class)
        .map(|c| c.health.max(1))
        .unwrap_or(1);
    let flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
    if (flags & crate::class::FLAG_COUNTKILL) != 0 {
        world.level.found_kills = world.level.found_kills.saturating_sub(1);
    }
    if let Some(m) = world.actors.get_mut(id) {
        m.health = spawn_hp;
        m.destroyed = false;
        m.flags = flags;
        m.vel = DVec3::ZERO;
        m.reaction_time = 0;
    }
    world.dequeue_corpse(id);
    world.set_state(id, Some(raise), true);
    true
}

/// `A_RaiseMaster` — revive `master` if it is a corpse.
fn a_raise_master(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(m) = world.actors.get(self_id).and_then(|a| a.master) {
        let _ = try_raise_actor(world, m);
    }
    ActionResult::Continue
}

/// `A_RaiseChildren` — revive all actors with `master == self`.
fn a_raise_children(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in children_of(world, self_id) {
        let _ = try_raise_actor(world, id);
    }
    ActionResult::Continue
}

/// `A_RemoveChildren` — destroy children without death sequence.
fn a_remove_children(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in children_of(world, self_id) {
        let _ = a_remove(world, id);
    }
    ActionResult::Continue
}

/// `A_KillChildren` — kill children via death sequence.
fn a_kill_children(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in children_of(world, self_id) {
        let _ = a_die(world, id, &[]);
    }
    ActionResult::Continue
}

/// `A_RaiseSiblings` — revive other actors that share `master`.
fn a_raise_siblings(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in siblings_of(world, self_id) {
        let _ = try_raise_actor(world, id);
    }
    ActionResult::Continue
}

/// `A_RemoveSiblings` — destroy siblings without death sequence.
fn a_remove_siblings(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in siblings_of(world, self_id) {
        let _ = a_remove(world, id);
    }
    ActionResult::Continue
}

/// `A_KillSiblings` — kill siblings via death sequence.
fn a_kill_siblings(world: &mut World, self_id: ActorId) -> ActionResult {
    for id in siblings_of(world, self_id) {
        let _ = a_die(world, id, &[]);
    }
    ActionResult::Continue
}

/// `A_GiveToSiblings("Item" [, amount])` — give inventory to siblings.
fn a_give_to_siblings(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some((item, amount)) = parse_item_amount(args) else {
        return ActionResult::Continue;
    };
    if amount <= 0 {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&item) else {
        return ActionResult::Continue;
    };
    for id in siblings_of(world, self_id) {
        let _ = crate::inventory::give_inventory(world, id, cid, amount);
    }
    ActionResult::Continue
}

/// `A_DamageSiblings(damage [, "mod"])` — damage siblings.
fn a_damage_siblings(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dmg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if dmg <= 0 {
        return ActionResult::Continue;
    }
    for id in siblings_of(world, self_id) {
        crate::damage::damage_mobj(world, id, Some(self_id), Some(self_id), dmg);
    }
    ActionResult::Continue
}

/// `A_RemoveMaster` — destroy master without death sequence.
fn a_remove_master(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(m) = world.actors.get(self_id).and_then(|a| a.master) {
        let _ = a_remove(world, m);
    }
    ActionResult::Continue
}

/// `A_KillMaster` — kill master via death sequence.
fn a_kill_master(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(m) = world.actors.get(self_id).and_then(|a| a.master) {
        let _ = a_die(world, m, &[]);
    }
    ActionResult::Continue
}

/// Spawn a missile class along the actor's facing (M5 projectile bootstrap).
fn spawn_missile(world: &mut World, self_id: ActorId, class_name: &str, speed: f64) {
    // Doom `P_SpawnMissile`: aim via `R_PointToAngle2` + `momz` from aprox distance.
    // VanillaFixed keeps bit-exact spawn-at-source XY; Double offsets out of the
    // shooter (like `spawn_missile_angled`) so `P_CheckMissileSpawn` does not
    // immediately explode — but still aims `momz` at the target (elevated imps).
    if let Some(dest) = world.actors.get(self_id).and_then(|a| a.target) {
        if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
            spawn_missile_vanilla(world, self_id, dest, class_name, speed);
        } else {
            spawn_missile_aimed(world, self_id, dest, class_name, speed);
        }
        return;
    }
    spawn_missile_angled(world, self_id, class_name, speed, 0.0);
}

/// Class `Speed`, or `FastSpeed` under fast monsters (Nightmare fireballs).
pub(crate) fn class_move_speed(world: &World, class: &crate::class::ClassDef) -> f64 {
    if world.fast_monsters && class.fast_speed > 0.0 {
        class.fast_speed
    } else {
        class.speed
    }
}

/// Double-math `P_SpawnMissile`: target aim + spawn offset clear of the shooter.
fn spawn_missile_aimed(
    world: &mut World,
    source: ActorId,
    dest: ActorId,
    class_name: &str,
    speed_fallback: f64,
) {
    let Some(missile_class) = world.classes.find_name(class_name) else {
        return;
    };
    let Some(src) = world.actors.get(source).cloned() else {
        return;
    };
    let Some(dst) = world.actors.get(dest).cloned() else {
        return;
    };
    let dx = dst.pos.x() - src.pos.x();
    let dy = dst.pos.y() - src.pos.y();
    let dist_xy = dmath::sqrt(dx * dx + dy * dy).max(1.0);
    let ang = Angle::from_radians(dmath::atan2(dy, dx));
    let rad = ang.radians();
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(speed_fallback);
    // Vanilla spawn Z is `source->z + 32`; clear the shooter body in XY.
    let offset = src.radius + 8.0;
    let spawn_z = src.pos.z() + 32.0;
    let pos = DVec3::new(
        src.pos.x() + dmath::cos(rad) * offset,
        src.pos.y() + dmath::sin(rad) * offset,
        spawn_z,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return;
    };
    // `momz = (dest->z - source->z) / (dist / speed)` — aim at target feet.
    let steps = (dist_xy / speed).max(1.0);
    let momz = (dst.pos.z() - src.pos.z()) / steps;
    if let Some(m) = world.actors.get_mut(id) {
        m.angle = ang;
        m.vel = DVec3::new(dmath::cos(rad) * speed, dmath::sin(rad) * speed, momz);
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.health = 1000;
        m.target = Some(source);
        m.master = Some(source);
        m.tracer = src.target;
    }
    check_missile_spawn(world, id);
    record_missile_shot(world, source, dest, class_name, pos);
    let see = world
        .classes
        .get(missile_class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = see {
        if let Some(m) = world.actors.get(id) {
            world.play_sound(&snd, m.pos.x(), m.pos.y(), m.pos.z(), 1.0);
        }
    }
}

/// Vanilla `P_SpawnMissile(source, dest, type)`.
fn spawn_missile_vanilla(
    world: &mut World,
    source: ActorId,
    dest: ActorId,
    class_name: &str,
    speed_fallback: f64,
) {
    let Some(missile_class) = world.classes.find_name(class_name) else {
        return;
    };
    let Some(src) = world.actors.get(source).cloned() else {
        return;
    };
    let Some(dst) = world.actors.get(dest).cloned() else {
        return;
    };
    let sx = Fixed::from_f64(src.pos.x());
    let sy = Fixed::from_f64(src.pos.y());
    let sz = Fixed::from_f64(src.pos.z());
    let dx = Fixed::from_f64(dst.pos.x());
    let dy = Fixed::from_f64(dst.pos.y());
    let dz = Fixed::from_f64(dst.pos.z());
    // `source->z + 4*8*FRACUNIT`
    let pos = DVec3::new(
        sx.to_f64(),
        sy.to_f64(),
        (sz + Fixed::from_int(32)).to_f64(),
    );
    let an = math::point_to_angle2(sx.0, sy.0, dx.0, dy.0);
    let ang = Bam(an).to_angle();
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return;
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(speed_fallback);
    let speed_f = Fixed::from_f64(speed);
    let fine = math::bam_to_fine(an);
    let momx = Fixed(math::finecosine(fine)).mul(speed_f);
    let momy = Fixed(math::finesine(fine)).mul(speed_f);
    let mut dist = math::aprox_distance(dx.0.wrapping_sub(sx.0), dy.0.wrapping_sub(sy.0));
    // Doom: `dist = P_AproxDistance(...) / th->info->speed` (both `fixed_t`).
    if speed_f.0 != 0 {
        dist /= speed_f.0;
    }
    if dist < 1 {
        dist = 1;
    }
    let momz = Fixed((dz.0 - sz.0) / dist);
    if let Some(m) = world.actors.get_mut(id) {
        m.angle = ang;
        m.vel = DVec3::new(momx.to_f64(), momy.to_f64(), momz.to_f64());
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.health = 1000;
        m.target = Some(source);
        m.master = Some(source);
        m.tracer = src.target;
    }
    check_missile_spawn(world, id);
    record_missile_shot(world, source, dest, class_name, pos);
    let see = world
        .classes
        .get(missile_class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = see {
        if let Some(m) = world.actors.get(id) {
            world.play_sound(&snd, m.pos.x(), m.pos.y(), m.pos.z(), 1.0);
        }
    }
}

/// Doom `P_CheckMissileSpawn`: burn tics RNG, nudge forward, explode if blocked.
fn check_missile_spawn(world: &mut World, missile_id: ActorId) {
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        let roll = i32::from(world.p_random("pr_spawnmobj") & 3);
        if let Some(m) = world.actors.get_mut(missile_id) {
            m.tics = (m.tics - roll).max(1);
        }
    }
    let Some(m) = world.actors.get(missile_id).cloned() else {
        return;
    };
    let dest = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        // Doom: `mo->x += mo->momx>>1` (arithmetic shift on `fixed_t`).
        let mx = Fixed::from_f64(m.vel.x());
        let my = Fixed::from_f64(m.vel.y());
        let mz = Fixed::from_f64(m.vel.z());
        DVec3::new(
            (Fixed::from_f64(m.pos.x()) + Fixed(mx.0 >> 1)).to_f64(),
            (Fixed::from_f64(m.pos.y()) + Fixed(my.0 >> 1)).to_f64(),
            (Fixed::from_f64(m.pos.z()) + Fixed(mz.0 >> 1)).to_f64(),
        )
    } else {
        DVec3::new(
            m.pos.x() + m.vel.x() * 0.5,
            m.pos.y() + m.vel.y() * 0.5,
            m.pos.z() + m.vel.z() * 0.5,
        )
    };
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        if !crate::vanilla::try_move_fixed(world, missile_id, dest).ok {
            crate::physics::explode_missile(world, missile_id);
        }
    } else if !crate::physics::try_move(world, missile_id, dest) {
        crate::physics::explode_missile(world, missile_id);
    }
}

/// Spawn a missile with a yaw offset in degrees from the shooter's facing.
fn spawn_missile_angled(
    world: &mut World,
    self_id: ActorId,
    class_name: &str,
    speed: f64,
    yaw_offset_deg: f64,
) {
    let Some(missile_class) = world.classes.find_name(class_name) else {
        return;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return;
    };
    let ang = Angle::from_degrees(src.angle.degrees() + yaw_offset_deg);
    let rad = ang.radians();
    let pr = src.pitch.radians();
    let cp = dmath::cos(pr);
    let sp = dmath::sin(pr);
    let offset = src.radius + 8.0;
    // Prefer eye / shoot origin so rockets leave the muzzle, not the waist.
    let spawn_z = crate::hitscan::shoot_origin(world, self_id)
        .map(|(p, _)| p.z())
        .unwrap_or(src.pos.z() + src.height * 0.5);
    let pos = DVec3::new(
        src.pos.x() + dmath::cos(rad) * cp * offset,
        src.pos.y() + dmath::sin(rad) * cp * offset,
        spawn_z + sp * offset,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return;
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(speed);
    if let Some(m) = world.actors.get_mut(id) {
        m.pitch = src.pitch;
        m.vel = DVec3::new(
            dmath::cos(rad) * cp * speed,
            dmath::sin(rad) * cp * speed,
            sp * speed,
        );
        // Keep MISSILE; clear SOLID so projectiles don't block movers.
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.health = 1000;
        m.target = Some(self_id);
        m.master = Some(self_id);
        m.tracer = src.target;
    }
    check_missile_spawn(world, id);
    let aim = src.target.unwrap_or(self_id);
    record_missile_shot(world, self_id, aim, class_name, pos);
    // Doom missiles play `SeeSound` on launch (e.g. weapons/rocklf).
    let see = world
        .classes
        .get(missile_class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = see {
        if let Some(m) = world.actors.get(id) {
            world.play_sound(&snd, m.pos.x(), m.pos.y(), m.pos.z(), 1.0);
        }
    }
}

/// Record a missile launch for HIT / shot debug (`from` spawn → `to` target).
fn record_missile_shot(
    world: &mut World,
    source: ActorId,
    target: ActorId,
    missile_class: &str,
    from: DVec3,
) {
    let source_class = world
        .actors
        .get(source)
        .and_then(|a| world.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let (to, hit, hit_class) = if let Some(t) = world.actors.get(target) {
        let hit_class = world
            .classes
            .get(t.class)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        (
            [t.pos.x(), t.pos.y(), t.pos.z() + t.height * 0.5],
            Some(target),
            hit_class,
        )
    } else {
        ([from.x(), from.y(), from.z()], None, String::new())
    };
    world.push_shot_trace(crate::world::ShotTrace {
        tic: world.level.time,
        kind: crate::world::ShotKind::Missile,
        source,
        source_class,
        from: [from.x(), from.y(), from.z()],
        to,
        hit,
        hit_class,
        damage: 0,
        missile_class: missile_class.to_owned(),
    });
}

/// Count live Lost Souls (vanilla soft cap before Pain Elemental spits).
fn count_lost_souls(world: &World) -> usize {
    let Some(soul) = world.classes.find_name("LostSoul") else {
        return 0;
    };
    world
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.class == soul && a.health > 0)
        })
        .count()
}

/// Spawn a `LostSoul` ahead of `self_id` (Pain Elemental).
/// Returns `false` if the spawn was skipped or immediately killed (blocked).
fn spawn_lost_soul(world: &mut World, self_id: ActorId, yaw_offset_deg: f64) -> bool {
    // Vanilla: stop spawning once 20+ Lost Souls are already alive.
    if count_lost_souls(world) >= 20 {
        return false;
    }
    let Some(soul_class) = world.classes.find_name("LostSoul") else {
        return false;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return false;
    };
    let ang = Angle::from_degrees(src.angle.degrees() + yaw_offset_deg);
    let rad = ang.radians();
    // Vanilla: prestep ≈ 4*FRACUNIT + 3*(actor.radius + soul.radius)/2
    let soul_r = world
        .classes
        .get(soul_class)
        .map(|c| c.radius)
        .unwrap_or(16.0);
    let prestep = 4.0 + 1.5 * (src.radius + soul_r);
    let pos = DVec3::new(
        src.pos.x() + dmath::cos(rad) * prestep,
        src.pos.y() + dmath::sin(rad) * prestep,
        src.pos.z() + 8.0,
    );
    let Ok(id) = world.spawn_at(soul_class, pos, ang) else {
        return false;
    };
    if let Some(m) = world.actors.get_mut(id) {
        m.master = Some(self_id);
        m.target = src.target;
    }
    // If the soul is stuck in a wall/actor, kill it immediately (vanilla).
    if !crate::physics::try_move(world, id, pos) {
        crate::damage::damage_mobj(world, id, Some(self_id), Some(self_id), 10_000);
        return false;
    }
    let _ = a_skull_attack(world, id);
    true
}

/// `A_PainAttack`: face target and spit one LostSoul.
fn a_pain_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let _ = spawn_lost_soul(world, self_id, 0.0);
    ActionResult::Continue
}

/// `A_PainDie`: `A_Fall`, then three Lost Souls at +90° / +180° / +270°
/// (Doom `p_enemy.c`; none straight ahead).
fn a_pain_die(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_no_blocking(world, self_id);
    for i in 1..4 {
        let _ = spawn_lost_soul(world, self_id, f64::from(i) * 90.0);
    }
    ActionResult::Continue
}

/// `A_SkelMissile`: spawn `RevenantTracer`.
fn a_skel_missile(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if world.classes.find_name("RevenantTracer").is_some() {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("skeleton/attack", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        spawn_missile(world, self_id, "RevenantTracer", 10.0);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// Turn `self_id` toward `aim` by at most `max_turn` degrees; keep XY speed.
fn home_toward(world: &mut World, self_id: ActorId, aim: ActorId, max_turn: f64) {
    let Some(self_a) = world.actors.get(self_id).cloned() else {
        return;
    };
    let Some(target) = world.actors.get(aim) else {
        return;
    };
    let dx = target.pos.x() - self_a.pos.x();
    let dy = target.pos.y() - self_a.pos.y();
    let desired = Angle::from_radians(dmath::atan2(dy, dx)).degrees();
    let mut cur = self_a.angle.degrees();
    let mut delta = desired - cur;
    while delta > 180.0 {
        delta -= 360.0;
    }
    while delta < -180.0 {
        delta += 360.0;
    }
    let delta = delta.clamp(-max_turn, max_turn);
    cur += delta;
    let speed =
        dmath::sqrt(self_a.vel.x() * self_a.vel.x() + self_a.vel.y() * self_a.vel.y()).max(10.0);
    let rad = Angle::from_degrees(cur).radians();
    if let Some(a) = world.actors.get_mut(self_id) {
        a.angle = Angle::from_degrees(cur);
        a.vel = DVec3::new(dmath::cos(rad) * speed, dmath::sin(rad) * speed, a.vel.z());
    }
}

/// Angle difference in degrees (−180..=180) from `self` facing to `aim`.
fn angle_to_aim(world: &World, self_id: ActorId, aim: ActorId) -> Option<f64> {
    let self_a = world.actors.get(self_id)?;
    let target = world.actors.get(aim)?;
    let dx = target.pos.x() - self_a.pos.x();
    let dy = target.pos.y() - self_a.pos.y();
    let desired = Angle::from_radians(dmath::atan2(dy, dx)).degrees();
    let mut delta = desired - self_a.angle.degrees();
    while delta > 180.0 {
        delta -= 360.0;
    }
    while delta < -180.0 {
        delta += 360.0;
    }
    Some(delta)
}

/// `A_Tracer`: puff + smoke trail, then home toward `tracer` (else nearest player).
/// Homing / trail every other tic (vanilla TRACEANGLE cadence bootstrap).
fn a_tracer(world: &mut World, self_id: ActorId) -> ActionResult {
    if world.level.time % 2 != 0 {
        return ActionResult::Continue;
    }
    // Vanilla: puff at current pos; smoke one step behind (pos - vel).
    let Some(src) = world.actors.get(self_id).map(|a| (a.pos, a.vel)) else {
        return ActionResult::Continue;
    };
    let (pos, vel) = src;
    crate::hitscan::spawn_puff(world, pos);
    if let Some(smoke) = world.classes.find_name("RevenantTracerSmoke") {
        let behind = DVec3::new(pos.x() - vel.x(), pos.y() - vel.y(), pos.z());
        if let Ok(sid) = world.spawn_at(smoke, behind, Angle::ZERO) {
            if let Some(a) = world.actors.get_mut(sid) {
                a.vel = DVec3::new(0.0, 0.0, 1.0);
            }
            // Shorten display tics slightly (vanilla: tics -= P_Random()&3).
            let trim = i32::from(world.p_random("pr_tracer") & 3);
            if let Some(a) = world.actors.get_mut(sid) {
                a.tics = (a.tics - trim).max(1);
            }
        }
    }
    let aim = world
        .actors
        .get(self_id)
        .and_then(|a| a.tracer)
        .filter(|&tid| target_is_valid(world, tid))
        .or_else(|| nearest_player_mo(world, self_id));
    if let Some(aim) = aim {
        home_toward(world, self_id, aim, 5.0);
    }
    ActionResult::Continue
}

/// `A_SeekerMissile(threshold, maxturnangle [, flags])` — home on `tracer`.
/// Flag bit 0 (`SMF_LOOK`): acquire nearest player into `tracer` when unset.
fn a_seeker_missile(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let threshold = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(90.0)
        .abs();
    let max_turn = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(10.0)
        .abs();
    let flags = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let mut aim = world
        .actors
        .get(self_id)
        .and_then(|a| a.tracer)
        .filter(|&tid| target_is_valid(world, tid));
    if aim.is_none() && (flags & 1) != 0 {
        if let Some(tid) = nearest_player_mo(world, self_id) {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.tracer = Some(tid);
            }
            aim = Some(tid);
        }
    }
    let Some(aim) = aim else {
        return ActionResult::Continue;
    };
    let Some(delta) = angle_to_aim(world, self_id, aim) else {
        return ActionResult::Continue;
    };
    if delta.abs() > threshold {
        return ActionResult::Continue;
    }
    home_toward(world, self_id, aim, max_turn);
    ActionResult::Continue
}

/// `A_BspiAttack`: spawn `ArachnotronPlasma`.
fn a_bspi_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if world.classes.find_name("ArachnotronPlasma").is_some() {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("baby/attack", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        spawn_missile(world, self_id, "ArachnotronPlasma", 25.0);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// `A_FatAttack*`: spawn `FatShot` with optional yaw spread.
fn a_fat_attack(world: &mut World, self_id: ActorId, yaw_offset_deg: f64) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if world.classes.find_name("FatShot").is_some() {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("fatso/attack", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        spawn_missile_angled(world, self_id, "FatShot", 20.0, yaw_offset_deg);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// `A_TroopAttack`: melee if close, else spawn `DoomImpBall`.
fn a_troop_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let melee_range = class_melee_range(world, self_actor.class);
    for slot in &world.players {
        let Some(p) = slot else {
            continue;
        };
        let Some(mo) = p.mo else {
            continue;
        };
        let Some(other) = world.actors.get(mo) else {
            continue;
        };
        let dx = other.pos.x() - self_actor.pos.x();
        let dy = other.pos.y() - self_actor.pos.y();
        let dist = dmath::sqrt(dx * dx + dy * dy);
        if dist <= melee_range {
            world.play_sound(
                "imp/melee",
                self_actor.pos.x(),
                self_actor.pos.y(),
                self_actor.pos.z(),
                1.0,
            );
            // Vanilla: ((rnd%8)+1)*3
            let dmg = melee_dice(world, 8, 3);
            let _ = crate::damage::damage_mobj(world, mo, Some(self_id), Some(self_id), dmg);
            return ActionResult::Continue;
        }
    }
    world.play_sound(
        "imp/attack",
        self_actor.pos.x(),
        self_actor.pos.y(),
        self_actor.pos.z(),
        1.0,
    );
    spawn_missile(world, self_id, "DoomImpBall", 10.0);
    ActionResult::Continue
}

/// `A_HeadAttack`: spawn `CacodemonBall` (hitscan fallback if missing).
fn a_head_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if world.classes.find_name("CacodemonBall").is_some() {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("caco/attack", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        spawn_missile(world, self_id, "CacodemonBall", 10.0);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// `A_BruisAttack`: melee if close, else spawn `BaronBall`.
fn a_bruis_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let melee_range = class_melee_range(world, self_actor.class);
    for slot in &world.players {
        let Some(p) = slot else {
            continue;
        };
        let Some(mo) = p.mo else {
            continue;
        };
        let Some(other) = world.actors.get(mo) else {
            continue;
        };
        let dx = other.pos.x() - self_actor.pos.x();
        let dy = other.pos.y() - self_actor.pos.y();
        let dist = dmath::sqrt(dx * dx + dy * dy);
        if dist <= melee_range {
            world.play_sound(
                "baron/melee",
                self_actor.pos.x(),
                self_actor.pos.y(),
                self_actor.pos.z(),
                1.0,
            );
            // Vanilla: ((rnd%8)+1)*10
            let dmg = melee_dice(world, 8, 10);
            let _ = crate::damage::damage_mobj(world, mo, Some(self_id), Some(self_id), dmg);
            return ActionResult::Continue;
        }
    }
    if world.classes.find_name("BaronBall").is_some() {
        world.play_sound(
            "baron/attack",
            self_actor.pos.x(),
            self_actor.pos.y(),
            self_actor.pos.z(),
            1.0,
        );
        spawn_missile(world, self_id, "BaronBall", 15.0);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// `A_CyberAttack`: spawn `Rocket`.
fn a_cyber_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if world.classes.find_name("Rocket").is_some() {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("weapons/rocklf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        spawn_missile(world, self_id, "Rocket", 20.0);
        return ActionResult::Continue;
    }
    a_hitscan_attack(world, self_id, 1, "grunt/attack")
}

/// `A_Hoof` / `A_Metal` / `A_BabyMetal`: walk SFX then chase.
fn a_walk_sound_chase(world: &mut World, self_id: ActorId, sound: &str) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound(sound, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    a_chase(world, self_id)
}

/// Brief fullbright puff just ahead of a hitscanning monster (gun-height FX).
fn spawn_monster_muzzle_flash(world: &mut World, self_id: ActorId) {
    let Some(puff_class) = world
        .classes
        .find_name("BulletPuff")
        .or_else(|| world.classes.find_name("BulletChip"))
    else {
        return;
    };
    let Some(a) = world.actors.get(self_id) else {
        return;
    };
    let ang = a.angle;
    let dist = a.radius.max(16.0) + 12.0;
    let x = a.pos.x() + ang.cos() * dist;
    let y = a.pos.y() + ang.sin() * dist;
    let z = a.pos.z() + a.height * 0.65;
    let Ok(id) = world.spawn_at(puff_class, DVec3::new(x, y, z), ang) else {
        return;
    };
    // Oversized + opaque so the shooter's muzzle reads clearly next to body sprites.
    if let Some(p) = world.actors.get_mut(id) {
        p.scale_x = 1.75;
        p.scale_y = 1.75;
        p.alpha = 1.0;
        p.render_style = crate::class::RenderStyle::Normal;
    }
}

/// Shared zombie / shotgun / chaingun hitscan (`A_PosAttack` / `A_SPosAttack` / `A_CPosAttack`).
///
/// Vanilla: face target, play AttackSound (or `fallback`), fire `pellets` with
/// `(P_Random()-P_Random())<<20` yaw spread and `((P_Random()%5)+1)*3` damage each.
fn a_hitscan_attack(
    world: &mut World,
    self_id: ActorId,
    pellets: u32,
    fallback_sound: &str,
) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let (base, sound, x, y, z) = {
        let Some(a) = world.actors.get(self_id) else {
            return ActionResult::Continue;
        };
        let sound = world
            .classes
            .get(a.class)
            .map(|c| c.attack_sound.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_sound.into());
        (a.angle, sound, a.pos.x(), a.pos.y(), a.pos.z())
    };
    world.play_sound(&sound, x, y, z, 1.0);
    // Muzzle flash billboard at gun height so attacks read from the shooter's
    // side (body Missile E/F can be easy to miss in dark sectors).
    spawn_monster_muzzle_flash(world, self_id);
    if std::env::var_os("ROOM_TRACE_POSATK").is_some() {
        let tgt = world
            .actors
            .get(self_id)
            .and_then(|a| a.target)
            .and_then(|tid| world.actors.get(tid).map(|t| (t.pos.x(), t.pos.y())));
        eprintln!(
            "POSATK@{} from=({x:.1},{y:.1}) tgt={tgt:?} pellets={pellets}",
            world.level.time
        );
    }
    for _ in 0..pellets.max(1) {
        // BAM `(rnd-rnd)<<20` → degrees: delta * 360 / 4096.
        let r1 = i32::from(world.p_random("pr_hitscan"));
        let r2 = i32::from(world.p_random("pr_hitscan"));
        let spread_deg = f64::from(r1 - r2) * (360.0 / 4096.0);
        let ang = Angle::from_degrees(base.degrees() + spread_deg);
        let dmg_roll = i32::from(world.p_random("pr_hitscan"));
        let damage = (dmg_roll % 5 + 1) * 3;
        let _ = line_attack(world, self_id, ang, 2048.0, damage);
    }
    ActionResult::Continue
}

/// `A_BrainAwake`: Icon of Sin eye wake sound.
fn a_brain_awake(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("brain/sight", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_BrainPain`: Icon of Sin pain sound.
fn a_brain_pain(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("brain/pain", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// Spawn a stationary `Rocket` already in its `Death` (explosion) state.
fn spawn_brain_rocket(world: &mut World, x: f64, y: f64, z: f64) {
    let Some(rocket) = world.classes.find_name("Rocket") else {
        return;
    };
    let Ok(id) = world.spawn_at(rocket, DVec3::new(x, y, z), Angle::ZERO) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.vel = DVec3::ZERO;
        a.flags &= !crate::class::FLAG_MISSILE;
    }
    let death = world.states.find_label(rocket, "Death");
    // Call A_Explode on entry (vanilla brain rockets explode immediately).
    world.set_state(id, death, true);
    if let Some(a) = world.actors.get(id) {
        world.play_sound("weapons/rocklx", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
}

/// `A_BrainScream`: spray exploding rockets across the Icon, then death sound.
fn a_brain_scream(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(a) = world.actors.get(self_id).map(|a| (a.pos.x(), a.pos.y())) else {
        return ActionResult::Continue;
    };
    let (bx, by) = a;
    // Vanilla: x from brain.x-196 … brain.x+320 step 8; y = brain.y-320.
    let mut x = bx - 196.0;
    while x < bx + 320.0 {
        let r = world.p_random("pr_brainscream");
        let z = 128.0 + f64::from(r) * 2.0;
        spawn_brain_rocket(world, x, by - 320.0, z);
        x += 8.0;
    }
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("brain/death", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_BrainExplode`: one exploding rocket at a jittered offset from the actor.
fn a_brain_explode(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(a) = world.actors.get(self_id).map(|a| a.pos) else {
        return ActionResult::Continue;
    };
    let rx = world.p_random("pr_brainexplode");
    let rz = world.p_random("pr_brainexplode");
    // GZDoom: x += Random2()/32, z = 128 + Random()*2.
    let x = a.x() + f64::from(i32::from(rx) - 128) / 32.0;
    let z = 128.0 + f64::from(rz) * 2.0;
    spawn_brain_rocket(world, x, a.y(), z);
    ActionResult::Continue
}

/// `A_BrainDie`: exit the level.
fn a_brain_die(world: &mut World) -> ActionResult {
    world.level.exiting = true;
    world.level.secret_exit = false;
    ActionResult::Continue
}

/// Collect live `BossTarget` actors in thinker order.
fn boss_targets(world: &World) -> Vec<ActorId> {
    let Some(target_class) = world.classes.find_name("BossTarget") else {
        return Vec::new();
    };
    world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.class == target_class && a.health > 0)
        })
        .collect()
}

/// `A_BrainSpit([spawntype])`: fire a cube at the next `BossTarget`.
fn a_brain_spit(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    world.brain_spit_easy = !world.brain_spit_easy;
    if world.skill <= 2 && !world.brain_spit_easy {
        return ActionResult::Continue;
    }
    let targets = boss_targets(world);
    if targets.is_empty() {
        return ActionResult::Continue;
    }
    let idx = world.boss_target_cursor % targets.len();
    world.boss_target_cursor = world.boss_target_cursor.wrapping_add(1);
    let targ = targets[idx];
    let spawn_type = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "SpawnShot".into());
    let is_default = spawn_type.eq_ignore_ascii_case("SpawnShot");
    spawn_missile_at(world, self_id, targ, &spawn_type, 10.0);
    if let Some(a) = world.actors.get(self_id) {
        let (x, y, z, class) = (a.pos.x(), a.pos.y(), a.pos.z(), a.class);
        let sound = if is_default {
            "brain/spit".to_owned()
        } else {
            world
                .classes
                .get(class)
                .map(|c| c.attack_sound.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "brain/spit".into())
        };
        world.play_sound(&sound, x, y, z, 1.0);
    }
    ActionResult::Continue
}

/// Aim a missile from `self_id` toward `dest`, wiring cube pointers.
fn spawn_missile_at(
    world: &mut World,
    self_id: ActorId,
    dest: ActorId,
    class_name: &str,
    default_speed: f64,
) {
    let Some(missile_class) = world.classes.find_name(class_name) else {
        return;
    };
    let Some(src) = world.actors.get(self_id).cloned() else {
        return;
    };
    let Some(target) = world.actors.get(dest).cloned() else {
        return;
    };
    let src_z = src.pos.z() + src.height * 0.5;
    let tgt_z = target.pos.z() + target.height * 0.5;
    let dx = target.pos.x() - src.pos.x();
    let dy = target.pos.y() - src.pos.y();
    let dz = tgt_z - src_z;
    let dist_xy = dmath::sqrt(dx * dx + dy * dy).max(1.0);
    let dist = dmath::sqrt(dx * dx + dy * dy + dz * dz).max(1.0);
    let ang = Angle::from_radians(dmath::atan2(dy, dx));
    let pitch = Angle::from_radians(dmath::atan2(dz, dist_xy));
    let rad = ang.radians();
    let pr = pitch.radians();
    let cp = dmath::cos(pr);
    let sp = dmath::sin(pr);
    let offset = src.radius + 8.0;
    let pos = DVec3::new(
        src.pos.x() + dmath::cos(rad) * cp * offset,
        src.pos.y() + dmath::sin(rad) * cp * offset,
        src_z + sp * offset,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return;
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(default_speed);
    // Arrival delay in tics ≈ distance / speed (vanilla `special2` countdown).
    let flight_tics = (dist / speed.max(1.0)).round() as i32;
    if let Some(m) = world.actors.get_mut(id) {
        m.pitch = pitch;
        m.vel = DVec3::new(
            dmath::cos(rad) * cp * speed,
            dmath::sin(rad) * cp * speed,
            sp * speed,
        );
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.health = 1000;
        // Cube destination (unlike normal missiles where target is the shooter).
        m.target = Some(dest);
        m.master = Some(self_id);
        m.reaction_time = flight_tics.max(1);
    }
    check_missile_spawn(world, id);
    let see = world
        .classes
        .get(missile_class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = see {
        if let Some(m) = world.actors.get(id) {
            world.play_sound(&snd, m.pos.x(), m.pos.y(), m.pos.z(), 1.0);
        }
    }
}

/// Pick a default Icon-of-Sin spawn class from `pr_spawnfly`.
fn brain_spawn_class(world: &mut World) -> &'static str {
    let r = world.p_random("pr_spawnfly");
    if r < 50 {
        "DoomImp"
    } else if r < 90 {
        "Demon"
    } else if r < 120 {
        "Spectre"
    } else if r < 130 {
        "PainElemental"
    } else if r < 160 {
        "Cacodemon"
    } else if r < 162 {
        "Archvile"
    } else if r < 172 {
        "Revenant"
    } else if r < 192 {
        "Arachnotron"
    } else if r < 222 {
        "Fatso"
    } else if r < 246 {
        "HellKnight"
    } else {
        "BaronOfHell"
    }
}

/// Shared cube arrival: spawn fog + monster at destination, then remove cube.
fn spawn_fly_at(
    world: &mut World,
    self_id: ActorId,
    fog_class: &str,
    spawn_sound: &str,
) -> ActionResult {
    let Some(cube) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let Some(targ) = cube.target else {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.destroyed = true;
        }
        return ActionResult::Continue;
    };
    if cube.reaction_time > 0 {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.reaction_time -= 1;
            if a.reaction_time > 0 {
                return ActionResult::Continue;
            }
        }
    }
    let Some(dest) = world.actors.get(targ).map(|a| a.pos) else {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.destroyed = true;
        }
        return ActionResult::Continue;
    };
    if let Some(fog) = world.classes.find_name(fog_class) {
        let _ = world.spawn_at(fog, dest, Angle::ZERO);
    }
    world.play_sound(spawn_sound, dest.x(), dest.y(), dest.z(), 1.0);
    let spawn_name = brain_spawn_class(world);
    if let Some(cls) = world.classes.find_name(spawn_name) {
        if let Ok(id) = world.spawn_at(cls, dest, Angle::ZERO) {
            // Hate what the eye hates (eye is cube master).
            if let Some(eye) = cube.master.and_then(|m| world.actors.get(m)) {
                let hate = eye.target;
                if let Some(a) = world.actors.get_mut(id) {
                    a.target = hate;
                }
            }
            if let Some(see) = world.states.find_label(cls, "See") {
                world.set_state(id, Some(see), true);
            }
        }
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.destroyed = true;
    }
    ActionResult::Continue
}

/// `A_SpawnFly([fogtype])`: land cube and spawn a monster.
fn a_spawn_fly(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let fog = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "SpawnFire".into());
    spawn_fly_at(world, self_id, &fog, "brain/spawn")
}

/// `A_SpawnSound`: cube travel sound + spawn-fly.
fn a_spawn_sound(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("brain/cube", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    spawn_fly_at(world, self_id, "SpawnFire", "brain/spawn")
}

/// `A_BossDeath` / `A_KeenDie`: when the last peer of this class dies, fire the
/// vanilla map-specific effect (episode bosses, MAP07 tag floors, Keen door 666,
/// else exit).
fn a_boss_death(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(class) = world.actors.get(self_id).map(|a| a.class) else {
        return ActionResult::Continue;
    };
    let others_alive = world.thinkers.default.iter().any(|&id| {
        if id == self_id {
            return false;
        }
        world
            .actors
            .get(id)
            .is_some_and(|a| !a.destroyed && a.class == class && a.health > 0)
    });
    if others_alive {
        return ActionResult::Continue;
    }
    let class_name = world
        .classes
        .get(class)
        .map(|c| c.name.to_ascii_lowercase())
        .unwrap_or_default();
    // UMAPINFO bossaction overrides vanilla map checks when present for this class.
    let umap_actions: Vec<_> = world
        .boss_actions
        .iter()
        .filter(|ba| ba.class == class_name)
        .cloned()
        .collect();
    if !umap_actions.is_empty() {
        for ba in umap_actions {
            let args = [ba.tag, 0, 0, 0, 0];
            let special = ba.special.clamp(0, i32::from(u16::MAX)) as u16;
            let _ = crate::specials::execute_special(world, special, &args, None, None);
        }
        world.push_message("Boss defeated!");
        return ActionResult::Continue;
    }
    let map = world.map.name.to_ascii_uppercase();
    match class_name.as_str() {
        // Doom II MAP07: all Mancubi → lower tag 666; all Arachnotrons → raise tag 667.
        "fatso" if map == "MAP07" => {
            let args = [666, 8, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::FLOOR_LOWER_TO_LOWEST,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        "arachnotron" if map == "MAP07" => {
            let args = [667, 8, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::FLOOR_RAISE_TO_NEAREST,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        // Ultimate Doom episode bosses (vanilla A_BossDeath map checks).
        "baronofhell" if map == "E1M8" => {
            let args = [666, 8, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::FLOOR_LOWER_TO_LOWEST,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        "cyberdemon" if map == "E4M6" => {
            let args = [666, 8, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::FLOOR_RAISE_TO_NEAREST,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        "spidermastermind" if map == "E4M8" => {
            let args = [666, 8, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::FLOOR_LOWER_TO_LOWEST,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        // Commander Keen: open all tagged-666 doors (does not exit the map).
        "commanderkeen" => {
            let args = [666, 16, 0, 0, 0];
            let _ = crate::specials::execute_special(
                world,
                crate::specials::num::DOOR_OPEN,
                &args,
                None,
                None,
            );
            world.push_message("Boss defeated!");
        }
        // E2M8 Cyber / E3M8 Spider / other last-of-class bosses → exit.
        _ => {
            world.level.exiting = true;
            world.push_message("Boss defeated!");
        }
    }
    ActionResult::Continue
}

/// `A_SpidRefire` / `A_CPosRefire`: face target; sometimes keep firing; else abort to See
/// if the target is gone or out of sight.
///
/// Vanilla: `if (P_Random() < chance) return;` then check target/sight → `seestate`.
/// `chance` is 10 for spider, 40 for chaingunner. State loops via DECORATE `Goto`.
fn a_refire(world: &mut World, self_id: ActorId, chance: i32) -> ActionResult {
    a_refire_to_label(world, self_id, chance, "See")
}

/// `A_MonsterRefire(chance, "Label")` — like `A_CPosRefire` with a custom abort label.
fn a_monster_refire(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let chance = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(40);
    let label = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "See".to_owned());
    a_refire_to_label(world, self_id, chance, &label)
}

fn a_refire_to_label(
    world: &mut World,
    self_id: ActorId,
    chance: i32,
    label: &str,
) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let roll = i32::from(world.p_random("pr_refire"));
    if roll < chance {
        return ActionResult::Continue;
    }
    let tid = world
        .actors
        .get(self_id)
        .and_then(|a| a.target)
        .or_else(|| world.players.iter().flatten().find_map(|p| p.mo));
    let abort = match tid {
        None => true,
        Some(t) => {
            let dead = world
                .actors
                .get(t)
                .is_none_or(|a| a.destroyed || a.health <= 0);
            dead || !crate::sight::check_sight(world, self_id, t)
        }
    };
    if abort {
        if let Some(dest) = world
            .actors
            .get(self_id)
            .and_then(|a| world.states.find_label(a.class, label))
        {
            return ActionResult::Jump(dest);
        }
    }
    ActionResult::Continue
}

/// `A_VileStart`: arch-vile attack telegraph.
fn a_vile_start(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("vile/start", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_VileTarget`: spawn `ArchvileFire` on the chase target and wire pointers.
fn a_vile_target(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    let Some(fire_class) = world.classes.find_name("ArchvileFire") else {
        return ActionResult::Continue;
    };
    let Some(tgt) = world.actors.get(tid).cloned() else {
        return ActionResult::Continue;
    };
    let Ok(fire_id) = world.spawn_at(fire_class, tgt.pos, Angle::ZERO) else {
        return ActionResult::Continue;
    };
    if let Some(fire) = world.actors.get_mut(fire_id) {
        // Vanilla: fire.target = vile, fire.tracer = victim.
        fire.target = Some(self_id);
        fire.tracer = Some(tid);
    }
    if let Some(vile) = world.actors.get_mut(self_id) {
        vile.tracer = Some(fire_id);
    }
    let _ = a_fire(world, fire_id);
    ActionResult::Continue
}

/// `A_StartFire`: arch-vile flame spawn sound, then track like `A_Fire`.
fn a_start_fire(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("vile/firestrt", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    a_fire(world, self_id)
}

/// `A_FireCrackle`: crackle cue, then track like `A_Fire`.
fn a_fire_crackle(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("vile/firecrkl", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    a_fire(world, self_id)
}

/// `A_Fire`: keep the flame on the victim while the vile still has LOS.
fn a_fire(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(fire) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let Some(vile_id) = fire.target else {
        return ActionResult::Continue;
    };
    let Some(dest_id) = fire.tracer else {
        return ActionResult::Continue;
    };
    if !target_is_valid(world, dest_id) {
        return ActionResult::Continue;
    }
    if !crate::sight::check_sight(world, vile_id, dest_id) {
        return ActionResult::Continue;
    }
    let Some(dest) = world.actors.get(dest_id) else {
        return ActionResult::Continue;
    };
    let pos = dest.pos;
    if let Some(a) = world.actors.get_mut(self_id) {
        a.pos = pos;
    }
    ActionResult::Continue
}

/// `A_VileAttack`: burn the chase target (or nearest player) when in range.
fn a_vile_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    // Prefer the flame origin for the boom cue when `A_VileTarget` wired it.
    let (sx, sy, sz) = self_actor
        .tracer
        .and_then(|fid| world.actors.get(fid))
        .filter(|f| !f.destroyed)
        .map(|f| (f.pos.x(), f.pos.y(), f.pos.z()))
        .unwrap_or((self_actor.pos.x(), self_actor.pos.y(), self_actor.pos.z()));
    world.play_sound("vile/stop", sx, sy, sz, 1.0);
    let tid =
        resolve_target(world, self_id).or_else(|| player_in_melee_range(world, self_id, 768.0));
    let Some(tid) = tid else {
        return ActionResult::Continue;
    };
    // Vanilla checks LOS + melee-ish range against the victim.
    let in_range = {
        let Some(tgt) = world.actors.get(tid) else {
            return ActionResult::Continue;
        };
        let dx = tgt.pos.x() - self_actor.pos.x();
        let dy = tgt.pos.y() - self_actor.pos.y();
        dx * dx + dy * dy <= 768.0 * 768.0
    };
    if !in_range || !crate::sight::check_sight(world, self_id, tid) {
        return ActionResult::Continue;
    }
    crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), 20);
    if let Some(a) = world.actors.get_mut(tid) {
        // Vanilla: momz = 1000*FRACUNIT / mass.
        let lift = 1000.0 / f64::from(a.mass.max(1));
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), a.vel.z() + lift);
    }
    // Vanilla: `P_RadiusAttack(fire, actor, 70, 70)`.
    radius_attack(world, self_id, DVec3::new(sx, sy, sz), 70, 70.0);
    ActionResult::Continue
}

/// `A_SkullAttack`: charge toward the chase target (XY + Z aim, SKULLSPEED 20).
fn a_skull_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let ang = a.angle.radians();
    let speed = 20.0;
    let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
    let momz = a
        .target
        .and_then(|tid| world.actors.get(tid))
        .map(|t| {
            let dx = t.pos.x() - a.pos.x();
            let dy = t.pos.y() - a.pos.y();
            let dist = (dx * dx + dy * dy).sqrt().max(1.0) / speed;
            (t.pos.z() + t.height * 0.5 - a.pos.z()) / dist
        })
        .unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.vel = DVec3::new(dmath::cos(ang) * speed, dmath::sin(ang) * speed, momz);
    }
    world.play_sound("skull/melee", x, y, z, 1.0);
    ActionResult::Continue
}

/// `A_Saw`: chainsaw melee; hit → `weapons/sawhit` + face target, miss → `weapons/sawfull`.
fn a_saw(world: &mut World, self_id: ActorId) -> ActionResult {
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    // Vanilla: ((rnd%10)+1)*2
    let damage = melee_dice(world, 10, 2);
    let hit = line_attack(world, self_id, angle, 64.0, damage);
    let snd = if hit.actor.is_some() {
        "weapons/sawhit"
    } else {
        "weapons/sawfull"
    };
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    // Vanilla: turn toward the struck actor on a successful saw hit.
    if let Some(tid) = hit.actor {
        let face = match (world.actors.get(self_id), world.actors.get(tid)) {
            (Some(src), Some(tgt)) => {
                let dx = tgt.pos.x() - src.pos.x();
                let dy = tgt.pos.y() - src.pos.y();
                Some(Angle::from_radians(dmath::atan2(dy, dx)))
            }
            _ => None,
        };
        if let Some(face) = face {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.angle = face;
            }
        }
    }
    ActionResult::Continue
}

/// `A_CustomPunch(damage [, norandom [, flags [, pufftype [, range]]]])`.
fn a_custom_punch(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let base = parse_i32_arg(args, 0, 2).max(0);
    let norandom = args
        .get(1)
        .map(|s| {
            let t = s.trim_matches('"').to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes"
        })
        .unwrap_or(false);
    let range = args
        .get(4)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .filter(|r| *r > 0.0)
        .unwrap_or(64.0);
    let damage = if norandom {
        base
    } else if base > 0 {
        melee_dice(world, base.max(1), 1).max(1)
    } else {
        0
    };
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    if damage > 0 {
        let hit = line_attack(world, self_id, angle, range, damage);
        if let Some(tid) = hit.actor {
            let face = match (world.actors.get(self_id), world.actors.get(tid)) {
                (Some(src), Some(tgt)) => {
                    let dx = tgt.pos.x() - src.pos.x();
                    let dy = tgt.pos.y() - src.pos.y();
                    Some(Angle::from_radians(dmath::atan2(dy, dx)))
                }
                _ => None,
            };
            if let Some(face) = face {
                if let Some(a) = world.actors.get_mut(self_id) {
                    a.angle = face;
                }
            }
        }
    }
    ActionResult::Continue
}

/// `A_FireBullets(spread_horiz, spread_vert, numbullets, damage [, flags [, puff [, range]]])`.
fn a_fire_bullets(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let spread_h = parse_f64_arg(args, 0, 0.0);
    let _spread_v = parse_f64_arg(args, 1, 0.0);
    let num = parse_i32_arg(args, 2, 1).max(1);
    let damage = parse_i32_arg(args, 3, 5).max(0);
    let range = args
        .get(6)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .filter(|r| *r > 0.0)
        .unwrap_or(2048.0);
    let base_ang = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    for i in 0..num {
        let yaw = if i == 0 && num > 1 && spread_h == 0.0 {
            // Vanilla shotgun-style: first bullet accurate when spread is 0.
            0.0
        } else if spread_h > 0.0 {
            let r1 = i32::from(world.p_random("pr_wpnfire"));
            let r2 = i32::from(world.p_random("pr_wpnfire"));
            f64::from(r1 - r2) * (spread_h / 255.0)
        } else {
            player_shotgun_spread_deg(world)
        };
        let ang = Angle::from_degrees(base_ang.degrees() + yaw);
        if damage > 0 {
            let _ = line_attack(world, self_id, ang, range, damage);
        }
    }
    ActionResult::Continue
}

/// `A_Punch`: short-range melee; ×10 damage when berserk; face target on hit.
fn a_punch(world: &mut World, self_id: ActorId) -> ActionResult {
    let berserk = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(self_id) && p.berserk);
    // Vanilla: ((rnd%10)+1)*2, ×10 with berserk.
    let mut damage = melee_dice(world, 10, 2);
    if berserk {
        damage *= 10;
    }
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("player/male/fist", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    let hit = line_attack(world, self_id, angle, 64.0, damage);
    if let Some(tid) = hit.actor {
        let face = match (world.actors.get(self_id), world.actors.get(tid)) {
            (Some(src), Some(tgt)) => {
                let dx = tgt.pos.x() - src.pos.x();
                let dy = tgt.pos.y() - src.pos.y();
                Some(Angle::from_radians(dmath::atan2(dy, dx)))
            }
            _ => None,
        };
        if let Some(face) = face {
            if let Some(a) = world.actors.get_mut(self_id) {
                a.angle = face;
            }
        }
    }
    ActionResult::Continue
}

/// Player bullet damage: `((P_Random()%3)+1)*5` via `pr_wpnfire`.
fn player_bullet_damage(world: &mut World) -> i32 {
    let roll = i32::from(world.p_random("pr_wpnfire")) % 3 + 1;
    roll * 5
}

/// Player shotgun/SSG yaw spread: BAM `(rnd-rnd)<<18` → degrees.
fn player_shotgun_spread_deg(world: &mut World) -> f64 {
    let r1 = i32::from(world.p_random("pr_wpnfire"));
    let r2 = i32::from(world.p_random("pr_wpnfire"));
    f64::from(r1 - r2) * (360.0 / 16_384.0)
}

/// Doom `P_GunShot`: damage, then `P_SubRandom()<<18` yaw if `!accurate`.
fn player_gun_shot(world: &mut World, self_id: ActorId, accurate: bool) {
    let mut angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    let slope = crate::hitscan::bullet_slope(world, self_id);
    let dmg = player_bullet_damage(world);
    if !accurate {
        angle = Angle::from_degrees(angle.degrees() + player_shotgun_spread_deg(world));
    }
    let _ = crate::hitscan::line_attack_with_slope(world, self_id, angle, 2048.0, dmg, slope);
}

fn player_refire(world: &World, self_id: ActorId) -> i32 {
    world
        .players
        .iter()
        .flatten()
        .find(|p| p.mo == Some(self_id))
        .map(|p| p.refire)
        .unwrap_or(world.thinking_refire)
}

/// `A_FirePistol`: single hitscan (`P_GunShot` with `!refire` accuracy).
fn a_fire_pistol(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/pistol", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    player_gun_shot(world, self_id, player_refire(world, self_id) == 0);
    ActionResult::Continue
}

/// `A_FireCGun`: single hitscan with chaingun sound (`P_GunShot` `!refire`).
fn a_fire_cgun(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/chngun", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    player_gun_shot(world, self_id, player_refire(world, self_id) == 0);
    ActionResult::Continue
}

/// `A_FireShotgun`: seven pellets with horizontal spread.
fn a_fire_shotgun(world: &mut World, self_id: ActorId) -> ActionResult {
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/shotgf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    // Doom `P_BulletSlope` once, then seven `P_GunShot` with that aimslope.
    let slope = crate::hitscan::bullet_slope(world, self_id);
    if std::env::var_os("ROOM_TRACE_WALL").is_some() && matches!(world.level.time, 410 | 456) {
        eprintln!(
            "BULLETSLOPE@{} slope_f={:.6} (~fixed {})",
            world.level.time,
            slope,
            (slope * 65536.0).round() as i32
        );
    }
    // Doom `P_GunShot`: damage dice first, then `P_SubRandom()<<18` yaw jitter.
    let trace_sg = std::env::var_os("ROOM_TRACE_WALL").is_some()
        && matches!(world.level.time, 410 | 456 | 1332 | 1333);
    if trace_sg {
        eprintln!(
            "FIRESHOTGUN@{} before_idx={} slope={:.6}",
            world.level.time, world.vanilla_rng.prndindex, slope
        );
    }
    for _ in 0..7 {
        let dmg = player_bullet_damage(world);
        let a = Angle::from_degrees(angle.degrees() + player_shotgun_spread_deg(world));
        if trace_sg {
            eprintln!(
                "GUNSHOT@{} dmg={dmg} ang={:.1} idx={}",
                world.level.time,
                a.degrees(),
                world.vanilla_rng.prndindex
            );
        }
        let _ = crate::hitscan::line_attack_with_slope(world, self_id, a, 2048.0, dmg, slope);
    }
    if trace_sg {
        eprintln!(
            "FIRESHOTGUN@{} after_idx={}",
            world.level.time, world.vanilla_rng.prndindex
        );
    }
    ActionResult::Continue
}

/// `A_FireShotgun2`: super shotgun — 20 pellets, wider spread.
fn a_fire_shotgun2(world: &mut World, self_id: ActorId) -> ActionResult {
    let angle = world
        .actors
        .get(self_id)
        .map(|a| a.angle)
        .unwrap_or(Angle::ZERO);
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/sshotf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    // Doom SSG: `P_BulletSlope`, then pellets with yaw + slope jitter.
    let base_slope = crate::hitscan::bullet_slope(world, self_id);
    for _ in 0..20 {
        let dmg = player_bullet_damage(world);
        let a = Angle::from_degrees(angle.degrees() + player_shotgun_spread_deg(world) * 2.0);
        // `(P_SubRandom()<<5)` aimslope jitter ≈ ±4 map units / 256 run.
        let r1 = i32::from(world.p_random("pr_wpnfire"));
        let r2 = i32::from(world.p_random("pr_wpnfire"));
        let slope = base_slope + f64::from(r1 - r2) * (32.0 / 65536.0);
        let _ = crate::hitscan::line_attack_with_slope(world, self_id, a, 2048.0, dmg, slope);
    }
    ActionResult::Continue
}

/// `A_FireProjectile` options on top of plain `P_SpawnPlayerMissile`.
#[derive(Clone, Copy, Default)]
struct PlayerMissileOpts {
    /// Degrees added to the launch direction (left positive).
    angle: f64,
    /// Sideways spawn offset (right positive, GZDoom `spawnofs_xy`).
    ofs_xy: f64,
    /// Added to the `z + 32` spawn height.
    height: f64,
    /// Freelook pitch offset, GZDoom sign (positive aims down).
    pitch: f64,
    /// `FPF_AIMATANGLE`: autoaim along `angle` instead of turning afterwards.
    aim_at_angle: bool,
    /// `FPF_NOAUTOAIM`: follow the view pitch only.
    no_autoaim: bool,
}

fn spawn_player_missile(world: &mut World, self_id: ActorId, class_name: &str, speed: f64) {
    if let Some(cid) = world.classes.find_name(class_name) {
        let _ = spawn_player_missile_ex(world, self_id, cid, speed, PlayerMissileOpts::default());
    }
}

/// `P_SpawnPlayerMissile`: launch from `z + 32` along the autoaim angle /
/// slope (else freelook pitch). Ignores the shooter's `target`, which
/// `P_DamageMobj` sets to whoever last hurt the player.
fn spawn_player_missile_ex(
    world: &mut World,
    self_id: ActorId,
    missile_class: room_core::ClassId,
    speed: f64,
    opts: PlayerMissileOpts,
) -> Option<ActorId> {
    let src = world.actors.get(self_id).cloned()?;
    let shoot_angle = if opts.aim_at_angle {
        Angle::from_degrees(src.angle.degrees() + opts.angle)
    } else {
        src.angle
    };
    let (mut ang, aim) = if opts.no_autoaim {
        (shoot_angle, None)
    } else {
        crate::hitscan::player_missile_aim(world, self_id, shoot_angle)
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(speed);
    // Autoaim: vanilla `momz = speed * slope` on top of full XY speed.
    // Freelook: split `speed` along the view pitch (GZDoom).
    let (xy, z) = match aim {
        Some((s, _)) => (speed, speed * s),
        None => {
            let pr = (src.pitch.radians() - opts.pitch.to_radians()).clamp(-1.4, 1.4);
            (speed * dmath::cos(pr), speed * dmath::sin(pr))
        }
    };
    // Spawn just clear of the player in XY so `P_CheckMissileSpawn` does not
    // explode it against the shooter; `ofs_xy` is relative to the view angle.
    let rad = ang.radians();
    let view = src.angle.radians();
    let offset = src.radius + 8.0;
    let pos = DVec3::new(
        src.pos.x() + dmath::cos(rad) * offset + dmath::sin(view) * opts.ofs_xy,
        src.pos.y() + dmath::sin(rad) * offset - dmath::cos(view) * opts.ofs_xy,
        src.pos.z() + 32.0 + opts.height,
    );
    if !opts.aim_at_angle {
        ang = Angle::from_degrees(ang.degrees() + opts.angle);
    }
    let rad = ang.radians();
    let id = world.spawn_at(missile_class, pos, ang).ok()?;
    if let Some(m) = world.actors.get_mut(id) {
        m.angle = ang;
        m.pitch = src.pitch;
        m.vel = DVec3::new(dmath::cos(rad) * xy, dmath::sin(rad) * xy, z);
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.health = 1000;
        m.target = Some(self_id);
        m.master = Some(self_id);
        // GZDoom hands seekers their autoaim `linetarget`.
        m.tracer = aim.map(|(_, t)| t);
    }
    check_missile_spawn(world, id);
    let class_name = world
        .classes
        .get(missile_class)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    record_missile_shot(world, self_id, self_id, &class_name, pos);
    let see = world
        .classes
        .get(missile_class)
        .map(|c| c.see_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = see {
        if let Some(m) = world.actors.get(id) {
            world.play_sound(&snd, m.pos.x(), m.pos.y(), m.pos.z(), 1.0);
        }
    }
    Some(id)
}

const FPF_AIMATANGLE: i32 = 1;
const FPF_NOAUTOAIM: i32 = 4;

/// `FPF_*` flags as an int or `A|B` names.
fn parse_fpf_flags(raw: Option<&String>) -> i32 {
    let Some(raw) = raw.map(|s| s.trim_matches('"').trim()) else {
        return 0;
    };
    if let Ok(n) = raw.parse::<i32>() {
        return n;
    }
    raw.split('|')
        .map(|f| match f.trim().to_ascii_uppercase().as_str() {
            "FPF_AIMATANGLE" => FPF_AIMATANGLE,
            "FPF_TRANSFERTRANSLATION" => 2,
            "FPF_NOAUTOAIM" => FPF_NOAUTOAIM,
            "TRUE" => FPF_AIMATANGLE,
            _ => 0,
        })
        .fold(0, |a, b| a | b)
}

/// `A_FireProjectile` / `A_FireCustomMissile(missiletype, angle, useammo,
/// spawnofs_xy, spawnheight, flags, pitch)`. Ammo is spent when the Fire
/// state starts, so `useammo` is ignored. For `A_FireCustomMissile` a bool
/// `flags` is the old `aimatangle`.
fn a_fire_projectile(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some(cid) = args.first().and_then(|s| resolve_type_arg(world, s)) else {
        return ActionResult::Continue;
    };
    let flags = parse_fpf_flags(args.get(5));
    let opts = PlayerMissileOpts {
        angle: parse_f64_arg(args, 1, 0.0),
        ofs_xy: parse_f64_arg(args, 3, 0.0),
        height: parse_f64_arg(args, 4, 0.0),
        pitch: parse_f64_arg(args, 6, 0.0),
        aim_at_angle: flags & FPF_AIMATANGLE != 0,
        no_autoaim: flags & FPF_NOAUTOAIM != 0,
    };
    let _ = spawn_player_missile_ex(world, self_id, cid, 20.0, opts);
    ActionResult::Continue
}

/// `A_FireMissile`: spawn a `Rocket` from the shooter.
fn a_fire_missile(world: &mut World, self_id: ActorId) -> ActionResult {
    if world.classes.find_name("Rocket").is_some() {
        spawn_player_missile(world, self_id, "Rocket", 20.0);
    } else if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/rocklf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_FirePlasma`: spawn a `PlasmaBall`.
fn a_fire_plasma(world: &mut World, self_id: ActorId) -> ActionResult {
    if world.classes.find_name("PlasmaBall").is_some() {
        spawn_player_missile(world, self_id, "PlasmaBall", 25.0);
    } else if let Some(a) = world.actors.get(self_id) {
        world.play_sound("weapons/plasmaf", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_FireBFG`: spawn a `BFGBall` (spray extras spawn from `A_BFGSpray` on death).
fn a_fire_bfg(world: &mut World, self_id: ActorId) -> ActionResult {
    if world.classes.find_name("BFGBall").is_some() {
        spawn_player_missile(world, self_id, "BFGBall", 25.0);
    }
    ActionResult::Continue
}

/// Doom `P_NoiseAlert` — flood `soundtarget` onto reachable sectors from `emitter`.
/// Called from `P_FireWeapon` / `A_ReFire`, not from `A_Fire*`. Wake happens later
/// in `A_Look` when the monster next thinks.
pub(crate) fn noise_alert(world: &mut World, source: ActorId) {
    noise_alert_ex(world, source, source);
}

/// Like [`noise_alert`], but `target` becomes `soundtarget` while flooding from `emitter`.
pub(crate) fn noise_alert_ex(world: &mut World, target: ActorId, emitter: ActorId) {
    let Some(src) = world.actors.get(emitter).cloned() else {
        return;
    };
    if world.actors.get(target).is_none() {
        return;
    }
    // Doom `P_RecursiveSound`: flood through open 2S lines from the emitter's
    // sector. `ML_SOUNDBLOCK` allows one extra hop then stops.
    const ML_SOUNDBLOCK: i16 = 0x0040;
    let Some(start) = room_nodebuild::point_in_sector(&world.map, src.pos.x(), src.pos.y()) else {
        return;
    };
    let nsec = world.map.sectors.len();
    // `sound_traversed` equivalent: 0 = unvisited; else blocks+1.
    let mut traversed = vec![0u8; nsec];
    let mut stack: Vec<(usize, u8)> = vec![(start.raw() as usize, 0)];
    while let Some((si, blocks)) = stack.pop() {
        if si >= nsec {
            continue;
        }
        let mark = blocks + 1;
        if traversed[si] != 0 && traversed[si] <= mark {
            continue;
        }
        traversed[si] = mark;
        if let Some(sec) = world.sectors.get_mut(si) {
            sec.soundtarget = Some(target);
        }
        let Some(sec) = world.map.sectors.get(si) else {
            continue;
        };
        for lid in &sec.lines {
            let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let (Some(front), Some(back)) = (line.front_sector, line.back_sector) else {
                continue;
            };
            let fi = front.raw() as usize;
            let bi = back.raw() as usize;
            let (Some(fd), Some(bd)) = (world.sectors.get(fi), world.sectors.get(bi)) else {
                continue;
            };
            let open_bottom = fd.floor_z.max(bd.floor_z);
            let open_top = fd.ceiling_z.min(bd.ceiling_z);
            if open_top - open_bottom <= 0.0 {
                continue;
            }
            let other = if fi == si { bi } else { fi };
            if (line.flags & ML_SOUNDBLOCK) != 0 {
                if blocks == 0 {
                    stack.push((other, 1));
                }
            } else {
                stack.push((other, blocks));
            }
        }
    }
}

/// `A_BFGSpray`: 40 hitscan rays in a fan from the shooter (M5: first player).
fn a_bfg_spray(world: &mut World, self_id: ActorId) -> ActionResult {
    // Prefer the ball's `target` (shooter); fall back to first player, then self.
    let source = world
        .actors
        .get(self_id)
        .and_then(|a| a.target)
        .filter(|&tid| {
            world
                .actors
                .get(tid)
                .is_some_and(|a| !a.destroyed && a.health > 0)
        })
        .or_else(|| world.players.iter().flatten().find_map(|p| p.mo))
        .unwrap_or(self_id);
    let Some(origin) = world.actors.get(source).cloned() else {
        return ActionResult::Continue;
    };
    let base = origin.angle.degrees();
    // Vanilla: 40 rays across ~90° (≈2.25° steps) centered on facing.
    const RAYS: i32 = 40;
    let step = 90.0 / f64::from(RAYS);
    let start = base - 45.0 + step * 0.5;
    let extra = world.classes.find_name("BFGExtra");
    for i in 0..RAYS {
        let ang = Angle::from_degrees(start + step * f64::from(i));
        // Vanilla: damage = 15 * (1..8).
        let roll = i32::from(world.p_random("pr_bfg")) % 8 + 1;
        let damage = 15 * roll;
        let hit = crate::hitscan::line_attack(world, source, ang, 1024.0, damage);
        if let (Some(_), Some(extra_class)) = (hit.actor, extra) {
            let _ = world.spawn_at(extra_class, hit.pos, Angle::ZERO);
        }
    }
    ActionResult::Continue
}

/// Class `MeleeRange` (default 64).
fn class_melee_range(world: &World, class: room_core::ClassId) -> f64 {
    world
        .classes
        .get(class)
        .map(|c| c.melee_range)
        .filter(|&r| r > 0.0)
        .unwrap_or(64.0)
}

/// Doom / GZDoom `P_CheckMeleeRange`: `MeleeRange - 20 + target.radius`, approx dist, sight.
fn check_melee_range(world: &World, self_id: ActorId, tid: ActorId) -> bool {
    use crate::sight::check_sight;
    use crate::vanilla::approx_distance;
    use room_core::{FRACUNIT, Fixed};

    let Some(s) = world.actors.get(self_id) else {
        return false;
    };
    let Some(t) = world.actors.get(tid) else {
        return false;
    };
    let dist = approx_distance(
        Fixed::from_f64(t.pos.x() - s.pos.x()),
        Fixed::from_f64(t.pos.y() - s.pos.y()),
    );
    let melee_mu = if s.melee_range > 0.0 {
        s.melee_range
    } else {
        class_melee_range(world, s.class)
    };
    let melee = Fixed::from_f64(melee_mu);
    // Doom 1.5+: `MELEERANGE - 20*FRACUNIT + pl->info->radius`.
    let range = Fixed(melee.0 - 20 * FRACUNIT) + Fixed::from_f64(t.radius);
    if dist.0 >= range.0 {
        return false;
    }
    check_sight(world, self_id, tid)
}

/// First player pawn within `range` map units of `self_id`, if any.
fn player_in_melee_range(world: &World, self_id: ActorId, range: f64) -> Option<ActorId> {
    let self_actor = world.actors.get(self_id)?;
    let sx = self_actor.pos.x();
    let sy = self_actor.pos.y();
    for slot in &world.players {
        let Some(p) = slot else {
            continue;
        };
        let Some(mo) = p.mo else {
            continue;
        };
        if mo == self_id {
            continue;
        }
        let Some(other) = world.actors.get(mo) else {
            continue;
        };
        let dx = other.pos.x() - sx;
        let dy = other.pos.y() - sy;
        if dx * dx + dy * dy <= range * range {
            return Some(mo);
        }
    }
    None
}

/// `A_SargAttack`: melee if the first player pawn is within reach.
fn a_sarg_attack(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let range = world
        .actors
        .get(self_id)
        .map(|a| class_melee_range(world, a.class))
        .unwrap_or(64.0);
    if let Some(tid) = player_in_melee_range(world, self_id, range) {
        let (sound, x, y, z) = {
            let Some(a) = world.actors.get(self_id) else {
                return ActionResult::Continue;
            };
            let sound = world
                .classes
                .get(a.class)
                .map(|c| c.attack_sound.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "demon/melee".into());
            (sound, a.pos.x(), a.pos.y(), a.pos.z())
        };
        world.play_sound(&sound, x, y, z, 1.0);
        // Vanilla: ((rnd%10)+1)*4
        let dmg = melee_dice(world, 10, 4);
        crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), dmg);
    }
    ActionResult::Continue
}

/// `A_SkelWhoosh`: swing telegraph sound.
fn a_skel_whoosh(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    if let Some(a) = world.actors.get(self_id) {
        world.play_sound("skeleton/swing", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    ActionResult::Continue
}

/// `A_SkelFist`: Revenant melee punch.
fn a_skel_fist(world: &mut World, self_id: ActorId) -> ActionResult {
    let _ = a_face_target(world, self_id);
    let range = world
        .actors
        .get(self_id)
        .map(|a| class_melee_range(world, a.class))
        .unwrap_or(64.0);
    if let Some(tid) = player_in_melee_range(world, self_id, range) {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound("skeleton/melee", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        // Vanilla: ((rnd%10)+1)*6
        let dmg = melee_dice(world, 10, 6);
        crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), dmg);
    }
    ActionResult::Continue
}

/// Vanilla melee damage: `(P_Random() % sides + 1) * mult` via `pr_melesit`.
fn melee_dice(world: &mut World, sides: i32, mult: i32) -> i32 {
    let sides = sides.max(1);
    let roll = i32::from(world.p_random("pr_melesit")) % sides + 1;
    roll * mult
}

/// Radial blast around `origin` attributed to `source` (vanilla `P_RadiusAttack`).
fn radius_attack(
    world: &mut World,
    source: ActorId,
    origin: DVec3,
    max_damage: i32,
    distance: f64,
) {
    if max_damage <= 0 || distance <= 0.0 {
        return;
    }
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == source {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        if other.destroyed {
            continue;
        }
        let other_flags =
            other.flags | world.classes.get(other.class).map(|c| c.flags).unwrap_or(0);
        if (other_flags & crate::class::FLAG_NORADIUSDMG) != 0 {
            continue;
        }
        let dx = other.pos.x() - origin.x();
        let dy = other.pos.y() - origin.y();
        let dist = dmath::sqrt(dx * dx + dy * dy);
        if dist >= distance {
            continue;
        }
        let dmg = ((1.0 - dist / distance) * f64::from(max_damage)) as i32;
        if dmg <= 0 {
            continue;
        }
        let mass = if other.mass > 0 {
            other.mass
        } else {
            world
                .classes
                .get(other.class)
                .map(|c| c.mass.max(1))
                .unwrap_or(100)
        };
        crate::damage::damage_mobj(world, id, Some(source), Some(source), dmg);
        // Vanilla-ish thrust: damage*(FRACUNIT>>3)*100/mass → map units ≈ dmg*12.5/mass.
        if dist > 0.0 && (other_flags & crate::class::FLAG_DONTTHRUST) == 0 {
            if let Some(a) = world.actors.get_mut(id) {
                if !a.destroyed {
                    let thrust = f64::from(dmg) * 12.5 / f64::from(mass);
                    a.vel = DVec3::new(
                        a.vel.x() + dx / dist * thrust,
                        a.vel.y() + dy / dist * thrust,
                        a.vel.z(),
                    );
                }
            }
        }
    }
}

/// `A_Explode`: radius damage around self (Doom barrel defaults).
fn a_explode(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    // Cue DeathSound at the blast (rocket / barrel / …).
    let death_snd = world
        .classes
        .get(self_actor.class)
        .map(|c| c.death_sound.clone())
        .filter(|s| !s.is_empty());
    if let Some(snd) = death_snd {
        world.play_sound(
            &snd,
            self_actor.pos.x(),
            self_actor.pos.y(),
            self_actor.pos.z(),
            1.0,
        );
    }
    radius_attack(world, self_id, self_actor.pos, 128, 128.0);
    ActionResult::Continue
}

/// `A_Detonate` (MBF): `A_Explode` using the actor's `Damage` for blast + radius.
fn a_detonate(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let dmg = world
        .classes
        .get(self_actor.class)
        .map(|c| c.damage)
        .unwrap_or(0)
        .max(0);
    if dmg == 0 {
        return ActionResult::Continue;
    }
    radius_attack(world, self_id, self_actor.pos, dmg, f64::from(dmg));
    ActionResult::Continue
}

/// `A_Mushroom` (MBF): detonate + spawn fireballs (`FatShot` if present).
/// Optional args: slope (default 4.0), speed_mul (default 0.5).
fn a_mushroom(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let _ = a_detonate(world, self_id);
    let Some(self_actor) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let damage = world
        .classes
        .get(self_actor.class)
        .map(|c| c.damage)
        .unwrap_or(0)
        .max(0);
    let n = {
        let q = damage / 4 + 1;
        (q * q).clamp(0, 64)
    };
    if n == 0 {
        return ActionResult::Continue;
    }
    let slope = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(4.0);
    let speed_mul = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.5)
        .abs()
        .max(0.01);
    let shot = world
        .classes
        .find_name("FatShot")
        .or_else(|| world.classes.find_name("DoomImpBall"))
        .or_else(|| world.classes.find_name("Rocket"));
    let Some(shot) = shot else {
        return ActionResult::Continue;
    };
    let base_speed = world
        .classes
        .get(shot)
        .map(|c| class_move_speed(world, c))
        .filter(|&s| s > 0.0)
        .unwrap_or(20.0)
        * speed_mul;
    for i in 0..n {
        let ang = Angle::from_degrees(f64::from(i) * (360.0 / f64::from(n)));
        let rad = ang.radians();
        let c = dmath::cos(rad);
        let s = dmath::sin(rad);
        let pos = self_actor.pos;
        if let Ok(id) = world.spawn_at(shot, pos, ang) {
            if let Some(m) = world.actors.get_mut(id) {
                m.vel = DVec3::new(c * base_speed, s * base_speed, slope * base_speed * 0.25);
                m.target = self_actor.target;
                m.master = Some(self_id);
                // MBF: fireballs are gravity-affected (clear NOGRAVITY if set).
                m.flags &= !crate::class::FLAG_NOGRAVITY;
            }
        }
    }
    ActionResult::Continue
}

/// `A_Spawn` (MBF): spawn class at self + Z offset. Args: `"Class"` [, z].
fn a_mbf_spawn(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let class_name = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    if class_name.is_empty() {
        return ActionResult::Continue;
    }
    let Some(cid) = world.classes.find_name(&class_name) else {
        return ActionResult::Continue;
    };
    let z_off = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let pos = DVec3::new(src.pos.x(), src.pos.y(), src.pos.z() + z_off);
    let _ = world.spawn_at(cid, pos, src.angle);
    ActionResult::Continue
}

/// `A_Turn` (MBF): add degrees to yaw. Arg: degrees (misc1).
fn a_turn(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let deg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.angle = Angle::from_degrees(a.angle.degrees() + deg);
    }
    ActionResult::Continue
}

/// `A_Face` (MBF): set absolute yaw in degrees. Arg: degrees (misc1).
fn a_face(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let deg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(self_id) {
        a.angle = Angle::from_degrees(deg);
    }
    ActionResult::Continue
}

/// `A_Scratch` (MBF): fixed-damage melee. Args: damage [, "sound"].
fn a_scratch(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let dmg = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if dmg <= 0 {
        return ActionResult::Continue;
    }
    let sound = args
        .get(1)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let _ = a_face_target(world, self_id);
    let range = world
        .actors
        .get(self_id)
        .map(|a| class_melee_range(world, a.class))
        .unwrap_or(64.0);
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    let Some((_, dist)) = distance_to_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if dist > range {
        return ActionResult::Continue;
    }
    if let Some(ref snd) = sound {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
    }
    crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), dmg);
    ActionResult::Continue
}

/// `A_RandomJump` (MBF / ZDoom): `A_RandomJump("Label", chance)` — chance 0..=255.
fn a_random_jump(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let chance = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    if chance <= 0 || label.is_empty() {
        return ActionResult::Continue;
    }
    let roll = i32::from(world.p_random("pr_jump"));
    if roll >= chance {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

/// `A_LineEffect` (MBF): run a line special remotely. Args: special [, tag].
fn a_line_effect(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let special = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<u16>().ok())
        .unwrap_or(0);
    if special == 0 {
        return ActionResult::Continue;
    }
    let tag = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0);
    let _ =
        crate::specials::execute_special(world, special, &[tag, 0, 0, 0, 0], Some(self_id), None);
    ActionResult::Continue
}

/// Resolve a class by DECORATE name or DoomEdNum string.
fn resolve_type_arg(world: &World, raw: &str) -> Option<room_core::ClassId> {
    let name = raw.trim_matches('"');
    if name.is_empty() {
        return None;
    }
    if let Ok(num) = name.parse::<i32>() {
        if let Some(id) = world.classes.find_doomednum(num) {
            return Some(id);
        }
    }
    world.classes.find_name(name)
}

fn parse_f64_arg(args: &[String], i: usize, default: f64) -> f64 {
    args.get(i)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(default)
}

fn parse_i32_arg(args: &[String], i: usize, default: i32) -> i32 {
    args.get(i)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(default)
}

/// `A_SpawnObject` (MBF21): spawn type with pose/vel offsets (map units / degrees).
/// Args: type, angle, pitch, xofs, yofs, zofs [, xvel, yvel, zvel]
fn a_spawn_object(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some(cid) = args.first().and_then(|s| resolve_type_arg(world, s)) else {
        return ActionResult::Continue;
    };
    let ang_off = parse_f64_arg(args, 1, 0.0);
    let _pitch = parse_f64_arg(args, 2, 0.0);
    let xofs = parse_f64_arg(args, 3, 0.0);
    let yofs = parse_f64_arg(args, 4, 0.0);
    let zofs = parse_f64_arg(args, 5, 0.0);
    let xvel = parse_f64_arg(args, 6, 0.0);
    let yvel = parse_f64_arg(args, 7, 0.0);
    let zvel = parse_f64_arg(args, 8, 0.0);
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let ang = Angle::from_degrees(src.angle.degrees() + ang_off);
    let rad = ang.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let pos = DVec3::new(
        src.pos.x() + xofs * c - yofs * s,
        src.pos.y() + xofs * s + yofs * c,
        src.pos.z() + zofs,
    );
    if let Ok(id) = world.spawn_at(cid, pos, ang) {
        if let Some(m) = world.actors.get_mut(id) {
            m.vel = DVec3::new(xvel * c - yvel * s, xvel * s + yvel * c, zvel);
            m.target = src.target;
            m.master = Some(self_id);
            m.tracer = src.target;
        }
    }
    ActionResult::Continue
}

/// `A_MonsterProjectile` (MBF21): type, angle, pitch, hoffset, voffset.
fn a_monster_projectile(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let Some(missile_class) = args.first().and_then(|s| resolve_type_arg(world, s)) else {
        return ActionResult::Continue;
    };
    let ang_off = parse_f64_arg(args, 1, 0.0);
    let pitch = parse_f64_arg(args, 2, 0.0);
    let hoffset = parse_f64_arg(args, 3, 0.0);
    let voffset = parse_f64_arg(args, 4, 0.0);
    let _ = a_face_target(world, self_id);
    let Some(src) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let ang = Angle::from_degrees(src.angle.degrees() + ang_off);
    let rad = ang.radians();
    let c = dmath::cos(rad);
    let s = dmath::sin(rad);
    let spawnheight = 32.0 + voffset;
    let pos = DVec3::new(
        src.pos.x() + c * (src.radius + 8.0) - s * hoffset,
        src.pos.y() + s * (src.radius + 8.0) + c * hoffset,
        src.pos.z() + spawnheight,
    );
    let Ok(id) = world.spawn_at(missile_class, pos, ang) else {
        return ActionResult::Continue;
    };
    let speed = world
        .classes
        .get(missile_class)
        .map(|c| class_move_speed(world, c))
        .filter(|s| *s > 0.0)
        .unwrap_or(20.0);
    // Pitch: negative = up (MBF21).
    let pitch_rad = Angle::from_degrees(-pitch).radians();
    let horiz = dmath::cos(pitch_rad) * speed;
    let vz = dmath::sin(pitch_rad) * speed;
    if let Some(m) = world.actors.get_mut(id) {
        m.vel = DVec3::new(c * horiz, s * horiz, vz);
        m.flags |= crate::class::FLAG_MISSILE;
        m.flags &= !crate::class::FLAG_SOLID;
        m.target = Some(self_id);
        m.master = Some(self_id);
        m.tracer = src.target;
    }
    ActionResult::Continue
}

/// `A_MonsterBulletAttack` (MBF21): hspread, vspread, numbullets, damage [, dice].
fn a_monster_bullet_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let hspread = parse_f64_arg(args, 0, 0.0);
    let _vspread = parse_f64_arg(args, 1, 0.0);
    let num = parse_i32_arg(args, 2, 1).max(1);
    let damage = parse_i32_arg(args, 3, 3);
    let dice = parse_i32_arg(args, 4, 1).max(1);
    let _ = a_face_target(world, self_id);
    let Some(base) = world.actors.get(self_id).map(|a| a.angle) else {
        return ActionResult::Continue;
    };
    for i in 0..num {
        let t = if num == 1 {
            0.0
        } else {
            (f64::from(i) / f64::from(num - 1)) * 2.0 - 1.0
        };
        let ang = Angle::from_degrees(base.degrees() + t * hspread);
        let dmg = if dice <= 1 {
            damage
        } else {
            melee_dice(world, dice, damage)
        };
        let _ = line_attack(world, self_id, ang, 2048.0, dmg);
    }
    ActionResult::Continue
}

/// `A_MonsterMeleeAttack` (MBF21): damage_base, damage_dice [, sound, range].
fn a_monster_melee_attack(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let base = parse_i32_arg(args, 0, 3);
    let dice = parse_i32_arg(args, 1, 8).max(1);
    let sound = args
        .get(2)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty());
    let range = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or_else(|| {
            world
                .actors
                .get(self_id)
                .map(|a| class_melee_range(world, a.class))
                .unwrap_or(64.0)
        });
    let _ = a_face_target(world, self_id);
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    let Some((_, dist)) = distance_to_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if dist > range {
        return ActionResult::Continue;
    }
    if let Some(ref snd) = sound {
        if let Some(a) = world.actors.get(self_id) {
            world.play_sound(snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
    }
    let dmg = melee_dice(world, dice, base);
    crate::damage::damage_mobj(world, tid, Some(self_id), Some(self_id), dmg);
    ActionResult::Continue
}

/// `A_RadiusDamage` (MBF21): damage, radius.
fn a_radius_damage(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let damage = parse_i32_arg(args, 0, 128).max(0);
    let radius = parse_f64_arg(args, 1, 128.0).abs();
    let Some(pos) = world.actors.get(self_id).map(|a| a.pos) else {
        return ActionResult::Continue;
    };
    radius_attack(world, self_id, pos, damage, radius);
    ActionResult::Continue
}

/// `A_NoiseAlert` (MBF21): wake nearby hostiles onto our target.
fn a_noise_alert(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(tid) = world.actors.get(self_id).and_then(|a| a.target) else {
        return ActionResult::Continue;
    };
    if !target_is_valid(world, tid) {
        return ActionResult::Continue;
    }
    let origin = world
        .actors
        .get(self_id)
        .map(|a| a.pos)
        .unwrap_or(DVec3::ZERO);
    let alert_r2 = 1024.0 * 1024.0;
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == self_id || id == tid {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        if (a.flags & crate::class::FLAG_FRIENDLY) != 0 {
            continue;
        }
        if (a.flags & crate::class::FLAG_AMBUSH) != 0 {
            continue;
        }
        let dx = a.pos.x() - origin.x();
        let dy = a.pos.y() - origin.y();
        if dx * dx + dy * dy > alert_r2 {
            continue;
        }
        if let Some(m) = world.actors.get_mut(id) {
            m.target = Some(tid);
            m.flags &= !crate::class::FLAG_DORMANT;
        }
    }
    ActionResult::Continue
}

fn a_clear_tracer(world: &mut World, self_id: ActorId) -> ActionResult {
    if let Some(a) = world.actors.get_mut(self_id) {
        a.tracer = None;
    }
    ActionResult::Continue
}

/// `A_FindTracer` (MBF21): fov degrees [, range map units]. Sets `tracer`.
fn a_find_tracer(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let fov = parse_f64_arg(args, 0, 180.0).abs().max(1.0);
    let range = parse_f64_arg(args, 1, 2048.0).abs();
    let Some(self_a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let friendly = (self_a.flags & crate::class::FLAG_FRIENDLY) != 0;
    let mut best: Option<(ActorId, f64)> = None;
    for &id in &world.thinkers.default {
        if id == self_id {
            continue;
        }
        // Hostiles seek players; friends seek enemy monsters.
        let ok = if friendly {
            is_enemy_monster(world, id)
        } else {
            is_player_mo(world, id) && target_is_valid(world, id)
        };
        if !ok {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        let dx = other.pos.x() - self_a.pos.x();
        let dy = other.pos.y() - self_a.pos.y();
        let d2 = dx * dx + dy * dy;
        if d2 > range * range {
            continue;
        }
        let bearing = Angle::from_radians(dmath::atan2(dy, dx)).degrees();
        let mut delta = (bearing - self_a.angle.degrees()).rem_euclid(360.0);
        if delta > 180.0 {
            delta -= 360.0;
        }
        if delta.abs() > fov * 0.5 {
            continue;
        }
        if !crate::sight::check_sight(world, self_id, id) {
            continue;
        }
        if best.as_ref().is_none_or(|(_, bd)| d2 < *bd) {
            best = Some((id, d2));
        }
    }
    if let Some((id, _)) = best {
        if let Some(a) = world.actors.get_mut(self_id) {
            a.tracer = Some(id);
        }
    }
    ActionResult::Continue
}

/// `A_SeekTracer` (MBF21): threshold_deg, maxturn_deg — home missile toward tracer.
fn a_seek_tracer(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let threshold = parse_f64_arg(args, 0, 90.0).abs();
    let max_turn = parse_f64_arg(args, 1, 10.0).abs();
    let Some(aim) = world.actors.get(self_id).and_then(|a| a.tracer) else {
        return ActionResult::Continue;
    };
    if !target_is_valid(world, aim) {
        return ActionResult::Continue;
    }
    let Some(delta) = angle_to_aim(world, self_id, aim) else {
        return ActionResult::Continue;
    };
    if delta.abs() > threshold {
        return ActionResult::Continue;
    }
    home_toward(world, self_id, aim, max_turn);
    ActionResult::Continue
}

/// `A_HealChase` (MBF21): ["HealState", chance] — heal a nearby friend, else `A_Chase`.
fn a_heal_chase(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let chance = parse_i32_arg(args, 1, 128).clamp(0, 256);
    let roll = i32::from(world.p_random("pr_healchase"));
    if chance > 0 && roll < chance {
        let origin = world.actors.get(self_id).map(|a| a.pos);
        if let Some(origin) = origin {
            let ids: Vec<ActorId> = world.thinkers.default.clone();
            for id in ids {
                if id == self_id {
                    continue;
                }
                let Some(a) = world.actors.get(id) else {
                    continue;
                };
                if (a.flags & crate::class::FLAG_FRIENDLY) == 0 || a.destroyed || a.health <= 0 {
                    continue;
                }
                let max_hp = world
                    .classes
                    .get(a.class)
                    .map(|c| c.health)
                    .unwrap_or(a.health);
                if a.health >= max_hp {
                    continue;
                }
                let dx = a.pos.x() - origin.x();
                let dy = a.pos.y() - origin.y();
                if dx * dx + dy * dy > 64.0 * 64.0 {
                    continue;
                }
                if let Some(m) = world.actors.get_mut(id) {
                    m.health = max_hp;
                }
                if !label.is_empty() {
                    if let Some(class) = world.actors.get(self_id).map(|a| a.class) {
                        if let Some(sid) = world.states.find_label(class, &label) {
                            return ActionResult::Jump(sid);
                        }
                    }
                }
                return ActionResult::Continue;
            }
        }
    }
    a_chase(world, self_id)
}

/// `A_JumpIfHealthBelow` (MBF21): "Label", health — jump when health < health.
fn a_jump_if_health_below(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let thresh = parse_i32_arg(args, 1, 0);
    let health = world.actors.get(self_id).map(|a| a.health).unwrap_or(0);
    if health >= thresh || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

fn a_jump_if_target_in_sight(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some(tid) = resolve_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if !crate::sight::check_sight(world, self_id, tid) || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

/// MBF21 arg order: state, distance (unlike DECORATE `A_JumpIfCloser(dist, state)`).
fn a_jump_if_target_closer_mbf21(
    world: &mut World,
    self_id: ActorId,
    args: &[String],
) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let dist = parse_f64_arg(args, 1, 0.0);
    let Some((_, d)) = distance_to_target(world, self_id) else {
        return ActionResult::Continue;
    };
    if d >= dist || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

fn a_jump_if_tracer_in_sight(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let Some(tid) = world.actors.get(self_id).and_then(|a| a.tracer) else {
        return ActionResult::Continue;
    };
    if !target_is_valid(world, tid)
        || !crate::sight::check_sight(world, self_id, tid)
        || label.is_empty()
    {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

fn a_jump_if_tracer_closer(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let dist = parse_f64_arg(args, 1, 0.0);
    let Some((sx, sy, tid)) = world.actors.get(self_id).and_then(|a| {
        let t = a.tracer?;
        Some((a.pos.x(), a.pos.y(), t))
    }) else {
        return ActionResult::Continue;
    };
    let Some(t) = world.actors.get(tid) else {
        return ActionResult::Continue;
    };
    let dx = t.pos.x() - sx;
    let dy = t.pos.y() - sy;
    let d = dmath::sqrt(dx * dx + dy * dy);
    if d >= dist || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

fn a_add_flags(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let mut bits = 0u64;
    for a in args {
        if let Some(b) = flag_name_bit(a.trim_matches('"')) {
            bits |= b;
        }
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.flags |= bits;
    }
    ActionResult::Continue
}

fn a_remove_flags(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let mut bits = 0u64;
    for a in args {
        if let Some(b) = flag_name_bit(a.trim_matches('"')) {
            bits |= b;
        }
    }
    if let Some(a) = world.actors.get_mut(self_id) {
        a.flags &= !bits;
    }
    ActionResult::Continue
}

fn a_jump_if_flags_set(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let label = args
        .first()
        .map(|s| s.trim_matches('"').to_owned())
        .unwrap_or_default();
    let mut bits = 0u64;
    for a in args.iter().skip(1) {
        if let Some(b) = flag_name_bit(a.trim_matches('"')) {
            bits |= b;
        }
    }
    let flags = world.actors.get(self_id).map(|a| a.flags).unwrap_or(0);
    if bits == 0 || (flags & bits) != bits || label.is_empty() {
        return ActionResult::Continue;
    }
    let class = world.actors.get(self_id).map(|a| a.class);
    if let Some(class) = class {
        if let Some(sid) = world.states.find_label(class, &label) {
            return ActionResult::Jump(sid);
        }
    }
    ActionResult::Continue
}

fn flag_name_bit(name: &str) -> Option<u64> {
    Some(match name.to_ascii_uppercase().as_str() {
        "SOLID" => crate::class::FLAG_SOLID,
        "SHOOTABLE" => crate::class::FLAG_SHOOTABLE,
        "NOGRAVITY" => crate::class::FLAG_NOGRAVITY,
        "FLOAT" => crate::class::FLAG_FLOAT,
        "DROPOFF" => crate::class::FLAG_DROPOFF,
        "MISSILE" => crate::class::FLAG_MISSILE,
        "FRIENDLY" => crate::class::FLAG_FRIENDLY,
        "AMBUSH" => crate::class::FLAG_AMBUSH,
        "DORMANT" => crate::class::FLAG_DORMANT,
        "BOSS" => crate::class::FLAG_BOSS,
        "NORADIUSDMG" => crate::class::FLAG_NORADIUSDMG,
        "COUNTKILL" => crate::class::FLAG_COUNTKILL,
        _ => return None,
    })
}

/// `A_RadiusThrust(force, distance [, affectsource])` — push nearby actors (no damage).
fn a_radius_thrust(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let force = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(128.0)
        .abs();
    let distance = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(128.0)
        .abs()
        .max(1.0);
    let affect_source = args.get(2).is_some_and(|s| {
        let t = s.trim_matches('"').to_ascii_lowercase();
        t == "1" || t == "true" || t == "yes"
    });
    let Some(origin) = world.actors.get(self_id).map(|a| a.pos) else {
        return ActionResult::Continue;
    };
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == self_id && !affect_source {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        if other.destroyed {
            continue;
        }
        let other_flags =
            other.flags | world.classes.get(other.class).map(|c| c.flags).unwrap_or(0);
        if (other_flags & crate::class::FLAG_DONTTHRUST) != 0 {
            continue;
        }
        let dx = other.pos.x() - origin.x();
        let dy = other.pos.y() - origin.y();
        let dist = dmath::sqrt(dx * dx + dy * dy);
        if dist >= distance || dist < 1e-6 {
            continue;
        }
        let mass = if other.mass > 0 {
            other.mass
        } else {
            world
                .classes
                .get(other.class)
                .map(|c| c.mass.max(1))
                .unwrap_or(100)
        };
        let thrust = force * (1.0 - dist / distance) * 12.5 / f64::from(mass.max(1));
        if let Some(a) = world.actors.get_mut(id) {
            a.vel = DVec3::new(
                a.vel.x() + dx / dist * thrust,
                a.vel.y() + dy / dist * thrust,
                a.vel.z(),
            );
        }
    }
    ActionResult::Continue
}

/// `A_Quake(intensity, duration, damrad, tremrad [, sound])` — start an earthquake.
fn a_quake(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let intensity = args
        .first()
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 9.0);
    let duration = args
        .get(1)
        .and_then(|s| s.trim_matches('"').parse::<i32>().ok())
        .unwrap_or(0)
        .max(0);
    let damage_radius = args
        .get(2)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0)
        .abs();
    let tremor_radius = args
        .get(3)
        .and_then(|s| s.trim_matches('"').parse::<f64>().ok())
        .unwrap_or(0.0)
        .abs();
    let sound = args
        .get(4)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "world/quake".into());
    push_quake(
        world,
        self_id,
        intensity,
        duration,
        damage_radius,
        tremor_radius,
        sound,
    )
}

/// `A_QuakeEx(ix, iy, iz, duration, damrad, tremrad [, sound])` — max(|ix|,|iy|,|iz|).
fn a_quake_ex(world: &mut World, self_id: ActorId, args: &[String]) -> ActionResult {
    let ix = parse_f64_arg(args, 0, 0.0).abs();
    let iy = parse_f64_arg(args, 1, 0.0).abs();
    let iz = parse_f64_arg(args, 2, 0.0).abs();
    let intensity = ix.max(iy).max(iz).clamp(0.0, 9.0);
    let duration = parse_i32_arg(args, 3, 0).max(0);
    let damage_radius = parse_f64_arg(args, 4, 0.0).abs();
    let tremor_radius = parse_f64_arg(args, 5, 0.0).abs();
    let sound = args
        .get(6)
        .map(|s| s.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "world/quake".into());
    push_quake(
        world,
        self_id,
        intensity,
        duration,
        damage_radius,
        tremor_radius,
        sound,
    )
}

fn push_quake(
    world: &mut World,
    self_id: ActorId,
    intensity: f64,
    duration: i32,
    damage_radius: f64,
    tremor_radius: f64,
    sound: String,
) -> ActionResult {
    if duration == 0 || world.actors.get(self_id).is_none() {
        return ActionResult::Continue;
    }
    world.quakes.push(crate::world::Quake {
        spot: self_id,
        intensity,
        countdown: duration,
        damage_radius,
        tremor_radius,
        sound,
        thrust_multiplier: 0.5,
    });
    ActionResult::Continue
}

/// Parse `"RRGGBB"` / named colour → RGB 0..=1.
fn parse_blend_color(s: &str) -> Option<(f32, f32, f32)> {
    let t = s.trim_matches('"').trim();
    match t.to_ascii_lowercase().as_str() {
        "red" => return Some((1.0, 0.0, 0.0)),
        "green" => return Some((0.0, 1.0, 0.0)),
        "blue" => return Some((0.0, 0.0, 1.0)),
        "white" => return Some((1.0, 1.0, 1.0)),
        "black" => return Some((0.0, 0.0, 0.0)),
        _ => {}
    }
    let hex = t.strip_prefix('#').unwrap_or(t);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ))
}

/// `A_SetBlend(color, alpha, duration)` — full-screen fade (map units: duration in tics).
fn a_set_blend(world: &mut World, args: &[String]) -> ActionResult {
    let color = args
        .first()
        .map(|s| s.as_str())
        .and_then(parse_blend_color)
        .unwrap_or((0.0, 0.0, 0.0));
    let alpha = parse_f64_arg(args, 1, 0.0).clamp(0.0, 1.0) as f32;
    let duration = parse_i32_arg(args, 2, 0).max(0);
    let fade = &mut world.screen_fade;
    fade.target_r = color.0;
    fade.target_g = color.1;
    fade.target_b = color.2;
    fade.target_a = alpha;
    if duration <= 0 {
        fade.r = fade.target_r;
        fade.g = fade.target_g;
        fade.b = fade.target_b;
        fade.a = fade.target_a;
        fade.tics_left = 0;
    } else {
        fade.tics_left = duration;
    }
    ActionResult::Continue
}

/// `A_Wander`: walk along facing; on block pick a new 45° facing (`pr_wander`).
fn a_wander(world: &mut World, self_id: ActorId) -> ActionResult {
    let Some(a) = world.actors.get(self_id).cloned() else {
        return ActionResult::Continue;
    };
    let speed = {
        let actor_speed = a.speed;
        if actor_speed > 0.0 {
            actor_speed
        } else {
            world
                .classes
                .get(a.class)
                .map(|c| c.speed)
                .filter(|&s| s > 0.0)
                .unwrap_or(4.0)
        }
    };
    let mut ang = a.angle;
    let mut found = false;
    for attempt in 0..8 {
        if attempt > 0 {
            let dir = i32::from(world.p_random("pr_wander")) & 7;
            ang = Angle::from_degrees(f64::from(dir) * 45.0);
        }
        let rad = ang.radians();
        let dest = DVec3::new(
            a.pos.x() + dmath::cos(rad) * speed,
            a.pos.y() + dmath::sin(rad) * speed,
            a.pos.z(),
        );
        let check = crate::physics::check_position(world, &a, dest, Some(self_id));
        if check.ok {
            found = true;
            break;
        }
        if attempt == 0 {
            // First attempt used current facing; force a re-roll next.
            continue;
        }
    }
    if let Some(actor) = world.actors.get_mut(self_id) {
        actor.angle = ang;
        if found {
            let rad = ang.radians();
            actor.vel = DVec3::new(
                dmath::cos(rad) * speed,
                dmath::sin(rad) * speed,
                actor.vel.z(),
            );
        } else {
            actor.vel = DVec3::new(0.0, 0.0, actor.vel.z());
        }
    }
    ActionResult::Continue
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn noise_alert_wakes_non_ambush_only() {
        let ir = parse_decorate(
            r#"
actor Guard : Actor 9501
{
  Health 20
  Monster
  SeeSound "grunt/sight"
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS B -1
    Stop
  }
}
actor DeafGuard : Actor 9502
{
  Health 20
  Monster
  +AMBUSH
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let guard = classes.find_name("Guard").unwrap();
        let deaf = classes.find_name("DeafGuard").unwrap();
        assert_eq!(
            classes.get(deaf).unwrap().flags & crate::class::FLAG_AMBUSH,
            crate::class::FLAG_AMBUSH
        );
        let g = w
            .spawn_at(guard, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let d = w
            .spawn_at(deaf, DVec3::new(220.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let see_g = w.states.find_label(guard, "See").unwrap();
        let see_d = w.states.find_label(deaf, "See").unwrap();
        noise_alert(&mut w, player);
        // Doom: alert only stamps `soundtarget`; wake on next `A_Look`.
        assert!(
            w.sectors.iter().any(|s| s.soundtarget == Some(player)),
            "expected soundtarget flood"
        );
        assert_ne!(w.actors.get(g).unwrap().state, Some(see_g));
        assert_eq!(call_action(&mut w, g, "A_Look"), ActionResult::Jump(see_g));
        assert_eq!(w.actors.get(g).unwrap().target, Some(player));
        assert!(
            w.sound_events.iter().any(|e| e.name == "grunt/sight"),
            "expected SeeSound on wake"
        );
        // Ambush + LOS of soundtarget also wakes (Doom); alert alone did not.
        assert_ne!(w.actors.get(d).unwrap().state, Some(see_d));
        assert_eq!(call_action(&mut w, d, "A_Look"), ActionResult::Jump(see_d));
        assert_eq!(w.actors.get(d).unwrap().target, Some(player));
    }

    #[test]
    fn a_chase_respects_max_target_range() {
        let ir = parse_decorate(
            r#"
actor ShortSight : Actor 9401
{
  Health 100
  Speed 0
  MaxTargetRange 100
  MinMissileChance 0
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Missile:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("ShortSight").unwrap();
        assert!((classes.get(cls).unwrap().max_target_range - 100.0).abs() < 1e-9);
        // 200 units away: MaxTargetRange 100 walks (speed 0 → no vel).
        let id = w
            .spawn_at(cls, DVec3::new(264.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
            a.reaction_time = 0;
            a.move_count = 0;
        }
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_ne!(
            w.actors.get(id).unwrap().state,
            w.states.find_label(cls, "Missile")
        );
        // Inside range → Missile (movecount 0 so Doom allows the check).
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(120.0, 64.0, 0.0);
            a.move_count = 0;
            a.flags &= !crate::class::FLAG_JUSTATTACKED;
        }
        assert_eq!(
            a_chase(&mut w, id),
            ActionResult::Jump(w.states.find_label(cls, "Missile").unwrap())
        );
    }

    #[test]
    fn a_chase_enters_missile_in_range() {
        let ir = parse_decorate(
            r#"
actor AlwaysShoot : Actor 9402
{
  Health 100
  Speed 0
  MinMissileChance 0
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Missile:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("AlwaysShoot").unwrap();
        let id = w
            .spawn_at(zombie, room_core::DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.level.time = 16;
        assert_eq!(
            a_chase(&mut w, id),
            ActionResult::Jump(w.states.find_label(zombie, "Missile").unwrap())
        );
    }

    #[test]
    fn a_chase_min_missile_chance_can_block() {
        let ir = parse_decorate(
            r#"
actor NeverShoot : Actor 9403
{
  Health 100
  Speed 0
  MinMissileChance 256
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Missile:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("NeverShoot").unwrap();
        assert_eq!(classes.get(cls).unwrap().min_missile_chance, 256);
        let id = w
            .spawn_at(cls, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.level.time = 16;
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
    }

    #[test]
    fn a_spos_attack_uses_attack_sound_and_three_pellets() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let shotgun = classes.find_name("ShotgunGuy").unwrap();
        assert_eq!(classes.get(shotgun).unwrap().attack_sound, "shotguy/attack");
        let id = w
            .spawn_at(shotgun, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let before = w.actors.get(mo).unwrap().health;
        let _ = w.drain_sound_events();
        assert_eq!(
            call_action(&mut w, id, "A_SPosAttack"),
            ActionResult::Continue
        );
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "shotguy/attack")
        );
        let after = w.actors.get(mo).unwrap().health;
        // Three pellets × at least 3 damage each when all connect.
        assert!(
            before - after >= 9,
            "expected ≥3 pellets of damage, before={before} after={after}"
        );
    }

    #[test]
    fn a_chase_uses_fast_speed_when_fast_monsters() {
        let ir = parse_decorate(
            r#"
actor Speedy : Actor 9801
{
  Health 100
  Speed 8
  FastSpeed 20
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("Speedy").unwrap();
        assert!((classes.get(cls).unwrap().fast_speed - 20.0).abs() < 1e-9);
        let id = w
            .spawn_at(cls, DVec3::new(600.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.level.time = 1;
        w.fast_monsters = true;
        let x0 = w.actors.get(id).unwrap().pos.x();
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        let stepped = dmath::sqrt((a.pos.x() - x0) * (a.pos.x() - x0) + a.pos.y() * a.pos.y());
        // Doom `P_Move` steps `FastSpeed` along an 8-way ray (here due west).
        assert!(
            (a.pos.x() - (x0 - 20.0)).abs() < 1e-6,
            "fast step x {} -> {}",
            x0,
            a.pos.x()
        );
        let _ = stepped;
    }

    #[test]
    fn a_chase_float_adjusts_z() {
        let ir = parse_decorate(
            r#"
actor Floater : Actor 9701
{
  Health 100
  Speed 0
  FloatSpeed 5
  ReactionTime 0
  +FLOAT
  +NOGRAVITY
  +SHOOTABLE
  States
  {
  Spawn:
    HEAD A -1
    Stop
  See:
    HEAD A 1 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("Floater").unwrap();
        assert_eq!(
            classes.get(cls).unwrap().flags & crate::class::FLAG_FLOAT,
            crate::class::FLAG_FLOAT
        );
        assert!((classes.get(cls).unwrap().float_speed - 5.0).abs() < 1e-9);
        let id = w
            .spawn_at(cls, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        // Player at z=0; floater starts at 0 — lift player so target is above.
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.pos = DVec3::new(64.0, 64.0, 40.0);
        }
        w.level.time = 1;
        let z0 = w.actors.get(id).unwrap().pos.z();
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert!((w.actors.get(id).unwrap().pos.z() - (z0 + 5.0)).abs() < 1e-9);
    }

    #[test]
    fn a_chase_uses_class_speed() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let demon = classes.find_name("Demon").unwrap();
        assert!((classes.get(demon).unwrap().speed - 10.0).abs() < 1e-9);
        let id = w
            .spawn_at(demon, DVec3::new(600.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0; // skip ReactionTime countdown
        }
        w.level.time = 1; // avoid active-sound / missile cadence
        let x0 = w.actors.get(id).unwrap().pos.x();
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        assert!(a.move_dir >= 0, "should pick an 8-way chase dir");
        let dist = dmath::sqrt(
            (a.pos.x() - x0) * (a.pos.x() - x0) + (a.pos.y() - 64.0) * (a.pos.y() - 64.0),
        );
        assert!(
            (dist - 10.0).abs() < 0.5,
            "Demon step distance {dist} (x {} -> {})",
            x0,
            a.pos.x()
        );
    }

    #[test]
    fn a_chase_opens_door_when_blocked() {
        use crate::specials::num;
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 240.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        for line in &mut map.lines {
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.y() - 256.0).abs() < 1.0 && (v2.y() - 256.0).abs() < 1.0 {
                line.special = num::DOOR_RAISE as i16;
                line.args = [1, 16, 150, 0, 0];
                break;
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        assert_ne!(
            classes.get(zombie).unwrap().flags & crate::class::FLAG_CANUSEWALLS,
            0
        );
        let id = w
            .spawn_at(
                zombie,
                DVec3::new(128.0, 200.0, 0.0),
                Angle::from_degrees(90.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
            a.target = w.players[0].as_ref().and_then(|p| p.mo);
            // Face / step north into the door line.
            a.move_dir = 2;
            a.move_count = 8;
            a.speed = 32.0;
            a.angle = Angle::from_degrees(90.0);
        }
        w.level.time = 1;
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert!(
            !w.doors.is_empty(),
            "blocked CANUSEWALLS chase should open the door"
        );
    }

    #[test]
    fn a_chase_picks_new_dir_when_blocked() {
        // Two rooms joined by a narrow north corridor; monster west of a wall,
        // player east — first west→east step is blocked, so NewChaseDir must turn.
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("west", rect(0.0, 0.0, 128.0, 256.0), 0.0, 128.0)
            .room("east", rect(192.0, 0.0, 320.0, 256.0), 0.0, 128.0)
            .room("hall", rect(128.0, 96.0, 192.0, 160.0), 0.0, 128.0)
            .player_start(256.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(120.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
            // Force an eastward step that hits the dividing wall at x=128.
            a.move_dir = 0;
            a.move_count = 8;
            a.speed = 16.0;
        }
        w.level.time = 1;
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        assert_ne!(a.move_dir, 0, "blocked east should re-pick chase dir");
        assert!(a.move_dir >= 0);
    }

    #[test]
    fn a_chase_counts_down_threshold() {
        let ir = parse_decorate(
            r#"
actor Clingy : Actor 9202
{
  Health 100
  Speed 0
  Threshold 3
  DefThreshold 3
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop See: POSS A 1 A_Chase Loop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("Clingy").unwrap();
        assert_eq!(classes.get(cls).unwrap().threshold, 3);
        let id = w
            .spawn_at(cls, DVec3::new(600.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.actors.get(id).unwrap().threshold, 3);
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        w.level.time = 1;
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().threshold, 2);
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().threshold, 1);
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().threshold, 0);
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().threshold, 0);
    }

    #[test]
    fn a_chase_respects_reaction_time() {
        let ir = parse_decorate(
            r#"
actor SlowWake : Actor 9201
{
  Health 100
  Speed 8
  ReactionTime 3
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("SlowWake").unwrap();
        assert_eq!(classes.get(cls).unwrap().reaction_time, 3);
        let id = w
            .spawn_at(cls, DVec3::new(600.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.actors.get(id).unwrap().reaction_time, 3);
        w.level.time = 1;
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().reaction_time, 2);
        let vel = w.actors.get(id).unwrap().vel;
        assert_eq!(vel.x(), 0.0);
        assert_eq!(vel.y(), 0.0);
        // Drain remaining countdown.
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().reaction_time, 0);
        let x0 = w.actors.get(id).unwrap().pos.x();
        assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        assert!(
            (a.pos.x() - x0).abs() > 1.0 || a.move_dir >= 0,
            "should begin walking after ReactionTime"
        );
    }

    #[test]
    fn a_vile_chase_raises_corpse() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(400.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let vile = classes.find_name("Archvile").unwrap();
        let corpse = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = crate::damage::damage_mobj(&mut w, corpse, None, None, 999);
        assert!(w.actors.get(corpse).unwrap().health <= 0);
        let kills = w.level.found_kills;
        let arch = w
            .spawn_at(vile, DVec3::new(120.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let heal = w.states.find_label(vile, "Heal").unwrap();
        let raise = w.states.find_label(zombie, "Raise").unwrap();
        assert_eq!(a_vile_chase(&mut w, arch), ActionResult::Jump(heal));
        let raised = w.actors.get(corpse).unwrap();
        assert_eq!(raised.health, classes.get(zombie).unwrap().health);
        assert_eq!(raised.state, Some(raise));
        assert_ne!(raised.flags & crate::class::FLAG_SOLID, 0);
        assert_eq!(w.level.found_kills, kills.saturating_sub(1));
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "vile/raise")
        );
    }

    #[test]
    fn a_no_blocking_clears_solid() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_ne!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_SOLID,
            0
        );
        assert_eq!(a_no_blocking(&mut w, id), ActionResult::Continue);
        assert_eq!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_SOLID,
            0
        );
    }

    #[test]
    fn a_chase_uses_class_melee_range() {
        let ir = parse_decorate(
            r#"
actor Reachy : Actor 9001
{
  Health 100
  Speed 0
  MeleeRange 200
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Melee:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let reachy = classes.find_name("Reachy").unwrap();
        assert!((classes.get(reachy).unwrap().melee_range - 200.0).abs() < 1e-9);
        // 150 units away: default 64 would walk; MeleeRange 200 jumps to Melee.
        let id = w
            .spawn_at(reachy, DVec3::new(214.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        w.level.time = 1;
        let melee = w.states.find_label(reachy, "Melee").unwrap();
        assert_eq!(a_chase(&mut w, id), ActionResult::Jump(melee));
    }

    #[test]
    fn a_chase_melee_threshold_prefers_melee() {
        let ir = parse_decorate(
            r#"
actor CloseFighter : Actor 9002
{
  Health 100
  Speed 0
  MeleeRange 64
  MeleeThreshold 300
  MaxTargetRange 512
  MinMissileChance 0
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Melee:
    POSS B -1
    Stop
  Missile:
    POSS C -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("CloseFighter").unwrap();
        assert!((classes.get(cls).unwrap().melee_threshold - 300.0).abs() < 1e-9);
        // 200 units: past MeleeRange, inside MeleeThreshold → Melee not Missile.
        let id = w
            .spawn_at(cls, DVec3::new(264.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        w.level.time = 16;
        let melee = w.states.find_label(cls, "Melee").unwrap();
        assert_eq!(a_chase(&mut w, id), ActionResult::Jump(melee));
    }

    #[test]
    fn a_chase_plays_active_sound() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        assert_eq!(classes.get(zombie).unwrap().active_sound, "grunt/active");
        // Far enough to walk (not melee/missile this tic).
        let id = w
            .spawn_at(zombie, DVec3::new(600.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        // Vanilla: ActiveSound when `P_Random() < 3` — keep chasing until a roll hits.
        // Keep movecount > 0 so Doom skips missile checks this path.
        w.level.time = 1;
        let mut heard = false;
        for _ in 0..512 {
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(600.0, 64.0, 0.0);
                a.move_count = 8;
            }
            let _ = w.drain_sound_events();
            assert_eq!(a_chase(&mut w, id), ActionResult::Continue);
            if w.drain_sound_events()
                .iter()
                .any(|e| e.name == "grunt/active")
            {
                heard = true;
                break;
            }
        }
        assert!(heard, "expected grunt/active within 512 chase rolls");
    }

    #[test]
    fn a_chase_melee_plays_attack_sound() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  AttackSound "grunt/attack"
  MeleeRange 64
  States
  {
  Spawn:
    POSS A -1
    Stop
  See:
    POSS A 1 A_Chase
    Loop
  Melee:
    POSS E 8
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        let _ = w.drain_sound_events();
        let melee = w.states.find_label(zombie, "Melee").unwrap();
        assert_eq!(a_chase(&mut w, id), ActionResult::Jump(melee));
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "grunt/attack"),
            "melee entry should play AttackSound, got {sounds:?}"
        );
    }

    #[test]
    fn a_troop_attack_spawns_imp_ball() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let imp = classes.find_name("DoomImp").unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        // Far from player so melee does not trigger.
        let id = w
            .spawn_at(imp, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let count_balls = |w: &World| {
            w.thinkers
                .default
                .iter()
                .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == ball))
                .count()
        };
        let before = count_balls(&w);
        assert_eq!(a_troop_attack(&mut w, id), ActionResult::Continue);
        assert_eq!(count_balls(&w), before + 1);
        let missile = w
            .thinkers
            .default
            .iter()
            .find_map(|&aid| {
                let a = w.actors.get(aid)?;
                (a.class == ball).then_some(a)
            })
            .unwrap();
        assert!(missile.vel.x().abs() > 0.0 || missile.vel.y().abs() > 0.0);
    }

    #[test]
    fn a_troop_attack_from_ledge_aims_down_at_player() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // Default complevel uses Double math — must still pitch missiles at target.
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert_eq!(w.math_mode, crate::vanilla::SimMathMode::Double);
        let imp = classes.find_name("DoomImp").unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(p) = w.actors.get_mut(mo) {
            p.pos = DVec3::new(64.0, 64.0, 0.0);
        }
        let id = w
            .spawn_at(imp, DVec3::new(300.0, 64.0, 120.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(mo);
            a.pitch = Angle::ZERO;
        }
        assert_eq!(a_troop_attack(&mut w, id), ActionResult::Continue);
        let missile = w
            .thinkers
            .default
            .iter()
            .find_map(|&aid| {
                let a = w.actors.get(aid)?;
                (a.class == ball).then_some(a)
            })
            .expect("DoomImpBall");
        assert!(
            missile.vel.z() < -0.5,
            "elevated imp fireball must aim down, vel.z={}",
            missile.vel.z()
        );
    }

    #[test]
    fn a_head_attack_spawns_caco_ball() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let caco = classes.find_name("Cacodemon").unwrap();
        let ball = classes.find_name("CacodemonBall").unwrap();
        let id = w
            .spawn_at(caco, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_head_attack(&mut w, id), ActionResult::Continue);
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == ball))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_pain_attack_spawns_lost_soul() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pe = classes.find_name("PainElemental").unwrap();
        let soul = classes.find_name("LostSoul").unwrap();
        let id = w
            .spawn_at(pe, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_pain_attack(&mut w, id), ActionResult::Continue);
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == soul && !a.destroyed && a.health > 0)
            })
            .count();
        assert_eq!(n, 1);
        assert_eq!(a_pain_die(&mut w, id), ActionResult::Continue);
        let n2 = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == soul && !a.destroyed && a.health > 0)
            })
            .count();
        // Cardinal spawns can overlap and kill each other; expect a net increase.
        assert!(
            n2 > n,
            "A_PainDie should add Lost Souls (alive={n2}, before={n})"
        );
    }

    #[test]
    fn a_pain_attack_kills_soul_when_blocked() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        // Narrow east–west corridor: PE near the east wall, facing east into it.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pe = classes.find_name("PainElemental").unwrap();
        let soul = classes.find_name("LostSoul").unwrap();
        let id = w
            .spawn_at(pe, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_pain_attack(&mut w, id), ActionResult::Continue);
        let alive = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == soul && !a.destroyed && a.health > 0)
            })
            .count();
        assert_eq!(alive, 0, "blocked Lost Soul should be killed immediately");
    }

    #[test]
    fn a_pain_attack_respects_lost_soul_cap() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pe = classes.find_name("PainElemental").unwrap();
        let soul = classes.find_name("LostSoul").unwrap();
        for i in 0..20 {
            let x = 64.0 + f64::from(i) * 48.0;
            let _ = w
                .spawn_at(soul, DVec3::new(x, 200.0, 0.0), Angle::ZERO)
                .unwrap();
        }
        let id = w
            .spawn_at(pe, DVec3::new(100.0, 100.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_pain_attack(&mut w, id), ActionResult::Continue);
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == soul && !a.destroyed && a.health > 0)
            })
            .count();
        assert_eq!(n, 20, "should not spawn beyond the 20 Lost Soul cap");
    }

    #[test]
    fn a_skull_attack_aims_z_at_target() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let soul = classes.find_name("LostSoul").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(p) = w.actors.get_mut(player) {
            p.pos = DVec3::new(64.0, 64.0, 80.0);
        }
        let id = w
            .spawn_at(
                soul,
                DVec3::new(264.0, 64.0, 0.0),
                Angle::from_degrees(180.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        assert_eq!(a_skull_attack(&mut w, id), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        assert!(
            a.vel.z() > 0.0,
            "should climb toward elevated target, vz={}",
            a.vel.z()
        );
        assert!(a.vel.x() < 0.0, "should charge west toward player");
    }

    #[test]
    fn boss_missile_actions_spawn() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let skel = classes.find_name("Revenant").unwrap();
        let baby = classes.find_name("Arachnotron").unwrap();
        let fat = classes.find_name("Fatso").unwrap();
        let tracer = classes.find_name("RevenantTracer").unwrap();
        let plasma = classes.find_name("ArachnotronPlasma").unwrap();
        let shot = classes.find_name("FatShot").unwrap();
        let a = w
            .spawn_at(skel, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let b = w
            .spawn_at(baby, DVec3::new(300.0, 128.0, 0.0), Angle::ZERO)
            .unwrap();
        let c = w
            .spawn_at(fat, DVec3::new(300.0, 192.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_skel_missile(&mut w, a), ActionResult::Continue);
        assert_eq!(a_bspi_attack(&mut w, b), ActionResult::Continue);
        assert_eq!(a_fat_attack(&mut w, c, 11.25), ActionResult::Continue);
        let count = |cid| {
            w.thinkers
                .default
                .iter()
                .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == cid))
                .count()
        };
        assert_eq!(count(tracer), 1);
        assert_eq!(count(plasma), 1);
        assert_eq!(count(shot), 1);
    }

    #[test]
    fn a_fire_cgun_damages_ahead() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.angle = Angle::ZERO;
        }
        let id = w
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(id).unwrap().health;
        assert_eq!(a_fire_cgun(&mut w, player), ActionResult::Continue);
        assert!(w.actors.get(id).unwrap().health < hp0);
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "weapons/chngun")
        );
    }

    #[test]
    fn a_fire_shotgun2_runs() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert!(classes.find_name("SuperShotgun").is_some());
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(a_fire_shotgun2(&mut w, player), ActionResult::Continue);
        assert!(!w.drain_sound_events().is_empty());
    }

    #[test]
    fn a_saw_damages_nearby() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(id).unwrap().health;
        assert_eq!(a_saw(&mut w, player), ActionResult::Continue);
        assert!(w.actors.get(id).unwrap().health < hp0);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "weapons/sawhit"),
            "hit should play sawhit, got {sounds:?}"
        );
    }

    #[test]
    fn a_saw_miss_plays_sawfull() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(a_saw(&mut w, player), ActionResult::Continue);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "weapons/sawfull"),
            "miss should play sawfull, got {sounds:?}"
        );
    }

    #[test]
    fn a_wander_uses_class_speed_in_open_space() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let speed = classes.get(zombie).unwrap().speed;
        assert!(speed > 0.0);
        let id = w
            .spawn_at(zombie, DVec3::new(256.0, 256.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(call_action(&mut w, id, "A_Wander"), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        let expected = speed;
        assert!(
            (a.vel.x() - expected).abs() < 1e-9,
            "vel.x={} expected {expected}",
            a.vel.x()
        );
        assert!(a.vel.y().abs() < 1e-9);
    }

    #[test]
    fn a_wander_turns_when_facing_wall() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        // Face west into the west wall from near the edge.
        let id = w
            .spawn_at(
                zombie,
                DVec3::new(24.0, 128.0, 0.0),
                Angle::from_degrees(180.0),
            )
            .unwrap();
        assert_eq!(call_action(&mut w, id, "A_Wander"), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        let deg = a.angle.degrees();
        assert!(
            (deg - 180.0).abs() > 1.0,
            "expected new facing after block, got {deg}"
        );
    }

    #[test]
    fn a_tracer_homes_toward_player() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let tracer = classes.find_name("RevenantTracer").unwrap();
        let smoke = classes.find_name("RevenantTracerSmoke").unwrap();
        let puff = classes.find_name("BulletPuff").unwrap();
        // Spawn east of player, facing north; should turn toward west (player).
        let id = w
            .spawn_at(tracer, DVec3::new(200.0, 64.0, 0.0), Angle::DEG_90)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(0.0, 10.0, 0.0);
            a.flags |= crate::class::FLAG_MISSILE;
        }
        w.level.time = 0; // even ⇒ seek
        let ang0 = w.actors.get(id).unwrap().angle.degrees();
        let before = w.actors.len();
        assert_eq!(a_tracer(&mut w, id), ActionResult::Continue);
        let ang1 = w.actors.get(id).unwrap().angle.degrees();
        // Desired heading is ~180° (toward player at x=64); from 90 should decrease toward 180… wait
        // player at (64,64), tracer at (200,64) ⇒ atan2(0, -136) = 180°. From 90° turn toward 180 ⇒ increase.
        assert!(ang1 > ang0, "should turn toward player ({ang0} -> {ang1})");
        assert!(w.actors.len() > before, "tracer should spawn trail actors");
        assert!(
            w.thinkers.default.iter().any(|&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == smoke && !a.destroyed)
            }),
            "expected RevenantTracerSmoke trail"
        );
        assert!(
            w.thinkers.default.iter().any(|&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == puff && !a.destroyed)
            }),
            "expected BulletPuff at tracer"
        );
    }

    #[test]
    fn a_boss_death_umapinfo_bossaction_runs_special() {
        use crate::world::BossAction;
        use room_core::{Angle, DVec3};
        use room_decorate::parse_decorate;
        use room_nodebuild::apply_nodes;
        use room_testkit::{MapBuilder, rect};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Fatso : Actor 3002
{
  Health 1
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: FATT A -1 Stop  Death: FATT H 1 A_BossDeath Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(666);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.boss_actions.push(BossAction {
            class: "fatso".into(),
            special: i32::from(crate::specials::num::FLOOR_LOWER_TO_LOWEST),
            tag: 666,
        });
        let class = classes.find_name("Fatso").unwrap();
        let id = w
            .spawn_at(class, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_boss_death(&mut w, id), ActionResult::Continue);
        assert!(!w.level.exiting, "UMAPINFO bossaction should not auto-exit");
        assert!(
            !w.floors.is_empty(),
            "expected floor mover for UMAPINFO bossaction tag 666"
        );
    }

    #[test]
    fn a_boss_death_sets_exiting_when_alone() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cyber = classes.find_name("Cyberdemon").unwrap();
        let id = w
            .spawn_at(cyber, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(!w.level.exiting);
        assert_eq!(a_boss_death(&mut w, id), ActionResult::Continue);
        assert!(w.level.exiting);
    }

    #[test]
    fn a_keen_die_waits_for_all_keens() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(666);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let keen = classes.find_name("CommanderKeen").unwrap();
        let k1 = w
            .spawn_at(keen, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let k2 = w
            .spawn_at(keen, DVec3::new(140.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(call_action(&mut w, k1, "A_KeenDie"), ActionResult::Continue);
        assert!(!w.level.exiting, "second keen still alive");
        assert!(w.doors.is_empty(), "should not open doors yet");
        if let Some(a) = w.actors.get_mut(k2) {
            a.health = 0;
            a.destroyed = true;
        }
        assert_eq!(call_action(&mut w, k1, "A_KeenDie"), ActionResult::Continue);
        assert!(!w.level.exiting, "Keen opens tag 666, does not exit");
        assert!(!w.doors.is_empty(), "last Keen should Door_Open tag 666");
    }

    #[test]
    fn a_boss_death_map07_fatso_lowers_tag_666() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("pit", rect(0.0, 0.0, 256.0, 256.0), 64.0, 192.0)
            .room("low", rect(256.0, 0.0, 384.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP07")
            .unwrap();
        map.sectors[0].tags.push(666);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert_eq!(w.map.name, "MAP07");
        let fat = classes.find_name("Fatso").unwrap();
        let id = w
            .spawn_at(fat, DVec3::new(100.0, 100.0, 64.0), Angle::ZERO)
            .unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(a_boss_death(&mut w, id), ActionResult::Continue);
        assert!(!w.level.exiting);
        assert!(!w.floors.is_empty(), "should start floor movers on tag 666");
        for _ in 0..16 {
            crate::specials::tick_floors(&mut w);
        }
        assert!(
            w.sectors[0].floor_z < z0 - 1.0,
            "floor should drop, z0={z0} now={}",
            w.sectors[0].floor_z
        );
    }

    #[test]
    fn a_boss_death_map07_arachnotron_raises_tag_667() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("pit", rect(0.0, 0.0, 256.0, 256.0), 0.0, 192.0)
            .room("high", rect(256.0, 0.0, 384.0, 256.0), 64.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP07")
            .unwrap();
        map.sectors[0].tags.push(667);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let baby = classes.find_name("Arachnotron").unwrap();
        let id = w
            .spawn_at(baby, DVec3::new(100.0, 100.0, 0.0), Angle::ZERO)
            .unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(a_boss_death(&mut w, id), ActionResult::Continue);
        assert!(!w.level.exiting);
        assert!(!w.floors.is_empty(), "should start floor movers on tag 667");
        for _ in 0..16 {
            crate::specials::tick_floors(&mut w);
        }
        assert!(
            w.sectors[0].floor_z > z0 + 1.0,
            "floor should rise, z0={z0} now={}",
            w.sectors[0].floor_z
        );
    }

    #[test]
    fn a_boss_death_e1m8_baron_lowers_tag_666() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("pit", rect(0.0, 0.0, 256.0, 256.0), 64.0, 192.0)
            .room("low", rect(256.0, 0.0, 384.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("E1M8")
            .unwrap();
        map.sectors[0].tags.push(666);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let baron = classes.find_name("BaronOfHell").unwrap();
        let id = w
            .spawn_at(baron, DVec3::new(100.0, 100.0, 64.0), Angle::ZERO)
            .unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(a_boss_death(&mut w, id), ActionResult::Continue);
        assert!(!w.level.exiting);
        for _ in 0..16 {
            crate::specials::tick_floors(&mut w);
        }
        assert!(
            w.sectors[0].floor_z < z0 - 1.0,
            "E1M8 baron should drop tag 666, z0={z0} now={}",
            w.sectors[0].floor_z
        );
    }

    #[test]
    fn a_cpos_refire_aborts_to_see_when_target_dead() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let chainguy = classes.find_name("ChaingunGuy").unwrap();
        let id = w
            .spawn_at(chainguy, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        if let Some(a) = w.actors.get_mut(player) {
            a.health = 0;
        }
        // Force the sight-check path (skip the early random Continue).
        for _ in 0..64 {
            let _ = w.p_random("pr_refire");
        }
        // Burn RNG until roll >= 40 so we reach the abort check.
        let see = w.states.find_label(chainguy, "See").unwrap();
        let mut jumped = false;
        for _ in 0..64 {
            match call_action(&mut w, id, "A_CPosRefire") {
                ActionResult::Jump(s) => {
                    assert_eq!(s, see);
                    jumped = true;
                    break;
                }
                ActionResult::Continue => {}
            }
        }
        assert!(jumped, "dead target should eventually abort to See");
    }

    #[test]
    fn a_monster_refire_aborts_to_custom_label() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let chainguy = classes.find_name("ChaingunGuy").unwrap();
        let id = w
            .spawn_at(chainguy, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        if let Some(a) = w.actors.get_mut(player) {
            a.health = 0;
        }
        for _ in 0..64 {
            let _ = w.p_random("pr_refire");
        }
        let pain = w.states.find_label(chainguy, "Pain").unwrap();
        let mut jumped = false;
        for _ in 0..64 {
            match call_action_args(&mut w, id, "A_MonsterRefire", &["40".into(), "Pain".into()]) {
                ActionResult::Jump(s) => {
                    assert_eq!(s, pain);
                    jumped = true;
                    break;
                }
                ActionResult::Continue => {}
            }
        }
        assert!(jumped, "dead target should abort to Pain");
    }

    #[test]
    fn a_spid_refire_continues_while_player_visible() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let spider = classes.find_name("SpiderMastermind").unwrap();
        let id = w
            .spawn_at(spider, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        // Visible living target: never Jump (state machine loops via Goto).
        for _ in 0..32 {
            assert_eq!(
                call_action(&mut w, id, "A_SpidRefire"),
                ActionResult::Continue
            );
        }
    }

    #[test]
    fn a_vile_attack_damages_player() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let vile = classes.find_name("Archvile").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let hp0 = w.actors.get(player).unwrap().health;
        let id = w
            .spawn_at(vile, DVec3::new(128.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        assert_eq!(a_vile_start(&mut w, id), ActionResult::Continue);
        assert_eq!(a_vile_target(&mut w, id), ActionResult::Continue);
        let fire = w.actors.get(id).unwrap().tracer.unwrap();
        assert_eq!(
            w.classes
                .get(w.actors.get(fire).unwrap().class)
                .unwrap()
                .name,
            "ArchvileFire"
        );
        assert_eq!(w.actors.get(fire).unwrap().target, Some(id));
        assert_eq!(w.actors.get(fire).unwrap().tracer, Some(player));
        assert_eq!(a_vile_attack(&mut w, id), ActionResult::Continue);
        assert!(w.actors.get(player).unwrap().health < hp0);
        assert!(w.actors.get(player).unwrap().vel.z() > 0.0);
        assert!(w.sound_events.iter().any(|e| e.name == "vile/stop"));
    }

    #[test]
    fn a_vile_attack_radius_damages_nearby() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let vile = classes.find_name("Archvile").unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let zid = w
            .spawn_at(zombie, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(zid).unwrap().health;
        let id = w
            .spawn_at(vile, DVec3::new(128.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        assert_eq!(a_vile_target(&mut w, id), ActionResult::Continue);
        assert_eq!(a_vile_attack(&mut w, id), ActionResult::Continue);
        assert!(
            w.actors.get(zid).unwrap().health < hp0,
            "nearby zombie should take 70-unit blast"
        );
    }

    #[test]
    fn a_fire_tracks_victim_position() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let vile = classes.find_name("Archvile").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let id = w
            .spawn_at(vile, DVec3::new(128.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        assert_eq!(a_vile_target(&mut w, id), ActionResult::Continue);
        let fire = w.actors.get(id).unwrap().tracer.unwrap();
        if let Some(p) = w.actors.get_mut(player) {
            p.pos = DVec3::new(200.0, 100.0, 0.0);
        }
        assert_eq!(a_fire(&mut w, fire), ActionResult::Continue);
        let fp = w.actors.get(fire).unwrap().pos;
        assert!((fp.x() - 200.0).abs() < 1e-9);
        assert!((fp.y() - 100.0).abs() < 1e-9);
        assert_eq!(a_start_fire(&mut w, fire), ActionResult::Continue);
        assert!(w.sound_events.iter().any(|e| e.name == "vile/firestrt"));
        assert_eq!(a_fire_crackle(&mut w, fire), ActionResult::Continue);
        assert!(w.sound_events.iter().any(|e| e.name == "vile/firecrkl"));
    }

    #[test]
    fn a_bfg_spray_damages_monster_ahead() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.angle = Angle::ZERO; // face +X
        }
        let zombie = classes.find_name("ZombieMan").unwrap();
        let bfg = classes.find_name("BFGBall").unwrap();
        let extra = classes.find_name("BFGExtra").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let ball = w
            .spawn_at(bfg, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(ball) {
            a.target = Some(player);
        }
        let hp0 = w.actors.get(id).unwrap().health;
        assert_eq!(a_bfg_spray(&mut w, ball), ActionResult::Continue);
        assert!(w.actors.get(id).unwrap().health < hp0);
        assert!(
            w.thinkers.default.iter().any(|&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == extra && !a.destroyed)
            }),
            "hit should spawn BFGExtra glow"
        );
    }

    #[test]
    fn a_explode_damages_nearby_actors() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let barrel = classes.find_name("ExplosiveBarrel").unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let hp0 = w.actors.get(player).unwrap().health;
        let bar = w
            .spawn_at(barrel, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let zid = w
            .spawn_at(zombie, DVec3::new(96.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let zhp0 = w.actors.get(zid).unwrap().health;
        assert_eq!(a_explode(&mut w, bar), ActionResult::Continue);
        assert!(w.actors.get(player).unwrap().health < hp0);
        assert!(w.actors.get(zid).unwrap().health < zhp0);
        assert!(
            w.sound_events.iter().any(|e| e.name == "world/barrelx"),
            "A_Explode should play DeathSound, events={:?}",
            w.sound_events
        );
    }

    #[test]
    fn a_explode_skips_noradiusdmg() {
        let ir = parse_decorate(
            r#"
actor Boom : Actor 9100
{
  States { Spawn: BAR1 A -1 Stop }
}
actor Shielded : Actor 9105
{
  Health 100
  +SHOOTABLE
  +NORADIUSDMG
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let boom = classes.find_name("Boom").unwrap();
        let shielded = classes.find_name("Shielded").unwrap();
        assert_eq!(
            classes.get(shielded).unwrap().flags & crate::class::FLAG_NORADIUSDMG,
            crate::class::FLAG_NORADIUSDMG
        );
        let bar = w
            .spawn_at(boom, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let id = w
            .spawn_at(shielded, DVec3::new(96.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_explode(&mut w, bar), ActionResult::Continue);
        assert_eq!(w.actors.get(id).unwrap().health, 100);
    }

    #[test]
    fn a_explode_dontthrust_skips_knockback() {
        let ir = parse_decorate(
            r#"
actor Boom : Actor 9100
{
  States { Spawn: BAR1 A -1 Stop }
}
actor Anchored : Actor 9106
{
  Health 10000
  Mass 50
  +SHOOTABLE
  +DONTTHRUST
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let boom = classes.find_name("Boom").unwrap();
        let anchored = classes.find_name("Anchored").unwrap();
        assert_eq!(
            classes.get(anchored).unwrap().flags & crate::class::FLAG_DONTTHRUST,
            crate::class::FLAG_DONTTHRUST
        );
        let bar = w
            .spawn_at(boom, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let id = w
            .spawn_at(anchored, DVec3::new(96.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(id).unwrap().health;
        assert_eq!(a_explode(&mut w, bar), ActionResult::Continue);
        let a = w.actors.get(id).unwrap();
        assert!(a.health < hp0, "still takes splash damage");
        assert_eq!(a.vel.x(), 0.0);
        assert_eq!(a.vel.y(), 0.0);
    }

    #[test]
    fn a_explode_thrust_scales_with_mass() {
        let ir = parse_decorate(
            r#"
actor LightDummy : Actor 9101
{
  Health 10000
  Mass 50
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor HeavyDummy : Actor 9102
{
  Health 10000
  Mass 1000
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Boom : Actor 9103
{
  Health 1
  States { Spawn: BAR1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let light = classes.find_name("LightDummy").unwrap();
        let heavy = classes.find_name("HeavyDummy").unwrap();
        let boom = classes.find_name("Boom").unwrap();
        assert_eq!(classes.get(light).unwrap().mass, 50);
        assert_eq!(classes.get(heavy).unwrap().mass, 1000);
        let bar = w
            .spawn_at(boom, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let lid = w
            .spawn_at(light, DVec3::new(232.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let hid = w
            .spawn_at(heavy, DVec3::new(232.0, 232.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_explode(&mut w, bar), ActionResult::Continue);
        let lv = w.actors.get(lid).unwrap().vel;
        let hv = w.actors.get(hid).unwrap().vel;
        let lspd = dmath::sqrt(lv.x() * lv.x() + lv.y() * lv.y());
        let hspd = dmath::sqrt(hv.x() * hv.x() + hv.y() * hv.y());
        assert!(lspd > hspd * 5.0, "light {lspd} vs heavy {hspd}");
        assert!(lspd > 1.0);
    }

    #[test]
    fn a_sarg_and_skel_fist_damage_player() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let demon = classes.find_name("Demon").unwrap();
        let skel = classes.find_name("Revenant").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let hp0 = w.actors.get(player).unwrap().health;
        let dog = w
            .spawn_at(demon, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_sarg_attack(&mut w, dog), ActionResult::Continue);
        let hp1 = w.actors.get(player).unwrap().health;
        assert!(hp1 < hp0, "sarg melee ({hp1} vs {hp0})");
        let rev = w
            .spawn_at(skel, DVec3::new(72.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_skel_whoosh(&mut w, rev), ActionResult::Continue);
        assert_eq!(a_skel_fist(&mut w, rev), ActionResult::Continue);
        let hp2 = w.actors.get(player).unwrap().health;
        assert!(hp2 < hp1, "skel fist ({hp2} vs {hp1})");
    }

    #[test]
    fn player_bullet_damage_matches_vanilla_formula() {
        let mut w = World::from_map(
            MapBuilder::new()
                .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
                .player_start(32.0, 32.0, 0)
                .build_udmf("MAP01")
                .unwrap(),
        )
        .unwrap();
        w.rng.set_seed(7);
        for _ in 0..24 {
            let d = player_bullet_damage(&mut w);
            assert!([5, 10, 15].contains(&d), "got {d}");
        }
    }

    #[test]
    fn melee_dice_matches_vanilla_formula() {
        let mut w = World::from_map(
            MapBuilder::new()
                .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
                .player_start(32.0, 32.0, 0)
                .build_udmf("MAP01")
                .unwrap(),
        )
        .unwrap();
        w.rng.set_seed(42);
        for _ in 0..32 {
            let skel = melee_dice(&mut w, 10, 6);
            assert!((6..=60).contains(&skel));
            assert_eq!(skel % 6, 0);
            let sarg = melee_dice(&mut w, 10, 4);
            assert!((4..=40).contains(&sarg));
            assert_eq!(sarg % 4, 0);
            let troop = melee_dice(&mut w, 8, 3);
            assert!((3..=24).contains(&troop));
            assert_eq!(troop % 3, 0);
        }
    }

    #[test]
    fn a_punch_berserk_hits_harder() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        // Face east toward the zombie.
        if let Some(a) = w.actors.get_mut(player) {
            a.angle = Angle::ZERO;
        }
        let id = w
            .spawn_at(zombie, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(id).unwrap().health;
        assert_eq!(a_punch(&mut w, player), ActionResult::Continue);
        let hp1 = w.actors.get(id).unwrap().health;
        assert!(hp1 < hp0);
        if let Some(p) = w.players[0].as_mut() {
            p.berserk = true;
        }
        // Reset health and punch again.
        if let Some(a) = w.actors.get_mut(id) {
            a.health = hp0;
        }
        assert_eq!(a_punch(&mut w, player), ActionResult::Continue);
        let hp2 = w.actors.get(id).unwrap().health;
        let normal = hp0 - hp1;
        let powered = hp0 - hp2;
        assert!(
            powered > normal,
            "berserk punch should deal more: normal={normal} powered={powered}"
        );
    }

    #[test]
    fn a_fire_plasma_and_bfg_spawn_balls() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let plasma = classes.find_name("PlasmaBall").unwrap();
        let bfg = classes.find_name("BFGBall").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(a_fire_plasma(&mut w, player), ActionResult::Continue);
        assert_eq!(a_fire_bfg(&mut w, player), ActionResult::Continue);
        let count = |cid| {
            w.thinkers
                .default
                .iter()
                .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == cid))
                .count()
        };
        assert_eq!(count(plasma), 1);
        assert_eq!(count(bfg), 1);
    }

    #[test]
    fn player_rocket_launches_from_32_and_ignores_last_attacker() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 256.0)
            .player_start(64.0, 512.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let barrel = classes.find_name("ExplosiveBarrel").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        // Last attacker: high up and off to the side, out of the autoaim cone.
        let attacker = w
            .spawn_at(
                barrel,
                DVec3::new(600.0, 1000.0, 200.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.target = Some(attacker);
            a.angle = Angle::from_degrees(0.0);
        }
        assert_eq!(a_fire_missile(&mut w, player), ActionResult::Continue);
        let m = w
            .thinkers
            .default
            .iter()
            .find_map(|&aid| w.actors.get(aid).filter(|a| a.class == rocket))
            .unwrap();
        let feet = w.actors.get(player).unwrap().pos.z();
        assert!(
            (m.pos.z() - (feet + 32.0)).abs() < 1e-6,
            "spawn z {}",
            m.pos.z()
        );
        assert!(m.vel.z().abs() < 1e-9, "level shot, vel {:?}", m.vel);
        assert!(
            m.vel.y().abs() < 1e-9 && m.vel.x() > 19.0,
            "straight ahead {:?}",
            m.vel
        );
    }

    #[test]
    fn a_fire_missile_spawns_rocket() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(a_fire_missile(&mut w, player), ActionResult::Continue);
        let missile = w.thinkers.default.iter().find_map(|&aid| {
            let a = w.actors.get(aid)?;
            (a.class == rocket).then_some(a)
        });
        let Some(missile) = missile else {
            panic!("rocket spawned");
        };
        let spd =
            dmath::sqrt(missile.vel.x() * missile.vel.x() + missile.vel.y() * missile.vel.y());
        assert!(
            (spd - 20.0).abs() < 1e-6,
            "rocket speed from ClassDef, got {spd}"
        );
        assert!(
            w.sound_events.iter().any(|e| e.name == "weapons/rocklf"),
            "Rocket SeeSound on launch, events={:?}",
            w.sound_events
        );
    }

    #[test]
    fn missile_inherits_shooter_pitch() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.pitch = Angle::from_degrees(30.0);
        }
        assert_eq!(a_fire_missile(&mut w, player), ActionResult::Continue);
        let missile = w.thinkers.default.iter().find_map(|&aid| {
            let a = w.actors.get(aid)?;
            (a.class == rocket).then_some(a)
        });
        let Some(missile) = missile else {
            panic!("rocket");
        };
        assert!((missile.pitch.degrees() - 30.0).abs() < 1e-6);
        let spd3 = dmath::sqrt(
            missile.vel.x() * missile.vel.x()
                + missile.vel.y() * missile.vel.y()
                + missile.vel.z() * missile.vel.z(),
        );
        assert!((spd3 - 20.0).abs() < 1e-6, "3D speed={spd3}");
        assert!(missile.vel.z() > 0.0);
    }

    #[test]
    fn a_bruis_attack_spawns_baron_ball() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let baron = classes.find_name("BaronOfHell").unwrap();
        let ball = classes.find_name("BaronBall").unwrap();
        let id = w
            .spawn_at(baron, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_bruis_attack(&mut w, id), ActionResult::Continue);
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == ball))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_cyber_attack_spawns_rocket() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cyber = classes.find_name("Cyberdemon").unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let id = w
            .spawn_at(cyber, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_cyber_attack(&mut w, id), ActionResult::Continue);
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == rocket))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_hoof_plays_cyber_hoof() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cyber = classes.find_name("Cyberdemon").unwrap();
        let id = w
            .spawn_at(cyber, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = w.drain_sound_events();
        assert_eq!(
            a_walk_sound_chase(&mut w, id, "cyber/hoof"),
            ActionResult::Continue
        );
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "cyber/hoof"),
            "expected cyber/hoof, got {sounds:?}"
        );
    }

    #[test]
    fn a_scream_boss_is_full_volume() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cyber = classes.find_name("Cyberdemon").unwrap();
        assert_ne!(
            classes.get(cyber).unwrap().flags & crate::class::FLAG_BOSS,
            0
        );
        let id = w
            .spawn_at(cyber, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = w.drain_sound_events();
        assert_eq!(call_action(&mut w, id, "A_Scream"), ActionResult::Continue);
        let ev = w.drain_sound_events();
        assert!(
            ev.iter()
                .any(|e| e.name == "cyber/death" && e.attenuation == 0.0),
            "boss A_Scream should be full volume, got {ev:?}"
        );
    }

    #[test]
    fn a_xscream_plays_misc_gibbed() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Default::default())
            .unwrap();
        let _ = w.drain_sound_events();
        assert_eq!(call_action(&mut w, id, "A_XScream"), ActionResult::Continue);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "misc/gibbed"),
            "vanilla A_XScream plays sfx_slop (misc/gibbed), got {sounds:?}"
        );
    }

    #[test]
    fn a_look_jumps_to_see_when_player_near() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  SeeSound "grunt/sight"
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS A 4 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 100,
            y: 64,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: i32::MIN,
            stamina: i32::MIN,
            species: String::new(),
            melee_range: -1.0,
            view_height: -1.0,
            mass: i32::MIN,
            speed: -1.0,
            friction: -1.0,
            damage_factor: -1.0,
            max_step_height: -1.0,
            max_dropoff_height: -1.0,
            float_speed: -1.0,
            fast_speed: -1.0,
            jump_z: -1.0,
            attack_z_offset: -1.0,
        });
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        assert!(w.players[0].is_some(), "player1 start auto-possessed");
        let zombie = w.thinkers.default[1];
        let see = w
            .states
            .find_label(w.actors.get(zombie).unwrap().class, "See")
            .unwrap();
        match call_action(&mut w, zombie, "A_Look") {
            ActionResult::Jump(s) => assert_eq!(s, see),
            other => panic!("expected Jump, got {other:?}"),
        }
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "grunt/sight")
        );
    }

    #[test]
    fn mbf21_projectile_radius_tracer_flags() {
        let ir = parse_decorate(
            r#"
actor Bolt : Actor 9200
{
  Speed 20
  +MISSILE
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Gunner : Actor 9201
{
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  Hurt: POSS B -1 Stop
  Seen: POSS C -1 Stop
  Near: POSS D -1 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let gunner = classes.find_name("Gunner").unwrap();
        let id = w
            .spawn_at(gunner, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
            a.reaction_time = 0;
        }
        let before = w.actors.len();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_MonsterProjectile",
                &[
                    "Bolt".into(),
                    "0".into(),
                    "0".into(),
                    "0".into(),
                    "0".into()
                ]
            ),
            ActionResult::Continue
        );
        assert!(w.actors.len() > before);

        let puppet = w
            .spawn_at(gunner, DVec3::new(220.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let h0 = w.actors.get(puppet).unwrap().health;
        assert_eq!(
            call_action_args(&mut w, id, "A_RadiusDamage", &["50".into(), "64".into()]),
            ActionResult::Continue
        );
        assert!(w.actors.get(puppet).unwrap().health < h0);

        assert_eq!(
            call_action(&mut w, id, "A_FindTracer"),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().tracer, Some(player));
        assert_eq!(
            call_action(&mut w, id, "A_ClearTracer"),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().tracer, None);

        let hurt = w.states.find_label(gunner, "Hurt").unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.health = 10;
        }
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_JumpIfHealthBelow",
                &["Hurt".into(), "50".into()]
            ),
            ActionResult::Jump(hurt)
        );

        assert_eq!(
            call_action_args(&mut w, id, "A_AddFlags", &["FRIENDLY".into()]),
            ActionResult::Continue
        );
        assert_ne!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_FRIENDLY,
            0
        );
        let seen = w.states.find_label(gunner, "Seen").unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_JumpIfFlagsSet",
                &["Seen".into(), "FRIENDLY".into()]
            ),
            ActionResult::Jump(seen)
        );
    }

    #[test]
    fn mbf_detonate_turn_face_spawn_scratch() {
        let ir = parse_decorate(
            r#"
actor Bomb : Actor 9100
{
  Damage 64
  States { Spawn: BAL1 A -1 Stop }
}
actor Puppet : Actor 9101
{
  Health 100
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Drop : Actor 9102
{
  States { Spawn: POSS A -1 Stop }
}
actor Claw : Actor 9103
{
  MeleeRange 128
  ReactionTime 0
  +SHOOTABLE
  States
  {
  Spawn: POSS A -1 Stop
  See: POSS A 1 A_Scratch(25, "demon/melee") Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bomb = classes.find_name("Bomb").unwrap();
        let puppet = classes.find_name("Puppet").unwrap();
        let drop = classes.find_name("Drop").unwrap();
        let claw = classes.find_name("Claw").unwrap();
        let bid = w
            .spawn_at(bomb, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let pid = w
            .spawn_at(puppet, DVec3::new(210.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let h0 = w.actors.get(pid).unwrap().health;
        assert_eq!(
            call_action(&mut w, bid, "A_Detonate"),
            ActionResult::Continue
        );
        assert!(w.actors.get(pid).unwrap().health < h0);

        let cid = w
            .spawn_at(
                claw,
                DVec3::new(100.0, 100.0, 0.0),
                Angle::from_degrees(10.0),
            )
            .unwrap();
        assert_eq!(
            call_action_args(&mut w, cid, "A_Turn", &["45".into()]),
            ActionResult::Continue
        );
        assert!((w.actors.get(cid).unwrap().angle.degrees() - 55.0).abs() < 1e-6);
        assert_eq!(
            call_action_args(&mut w, cid, "A_Face", &["90".into()]),
            ActionResult::Continue
        );
        assert!((w.actors.get(cid).unwrap().angle.degrees() - 90.0).abs() < 1e-6);

        let before = w.actors.len();
        assert_eq!(
            call_action_args(&mut w, cid, "A_Spawn", &["Drop".into(), "16".into()]),
            ActionResult::Continue
        );
        assert!(w.actors.len() > before);
        let _ = drop;

        // Scratch vs nearby puppet.
        if let Some(a) = w.actors.get_mut(pid) {
            a.pos = DVec3::new(120.0, 100.0, 0.0);
            a.health = 100;
        }
        if let Some(a) = w.actors.get_mut(cid) {
            a.target = Some(pid);
            a.reaction_time = 0;
        }
        let h1 = w.actors.get(pid).unwrap().health;
        assert_eq!(
            call_action_args(
                &mut w,
                cid,
                "A_Scratch",
                &["25".into(), "demon/melee".into()]
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(pid).unwrap().health, h1 - 25);
    }

    #[test]
    fn mbf_random_jump_and_line_effect() {
        let ir = parse_decorate(
            r#"
actor Jumper : Actor 9104
{
  States
  {
  Spawn:
    POSS A 1
    Loop
  Alt:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let jumper = classes.find_name("Jumper").unwrap();
        let id = w
            .spawn_at(jumper, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let alt = w.states.find_label(jumper, "Alt").unwrap();
        // chance 256 → always (roll < 256).
        assert_eq!(
            call_action_args(&mut w, id, "A_RandomJump", &["Alt".into(), "256".into()]),
            ActionResult::Jump(alt)
        );
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_LineEffect",
                &["12".into(), "1".into()] // Door_Raise
            ),
            ActionResult::Continue
        );
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn a_chase_friendly_follows_player_without_attack() {
        let ir = parse_decorate(
            r#"
actor Buddy : Actor 9003
{
  Speed 8
  ReactionTime 0
  +FRIENDLY
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A 1
    Loop
  See:
    POSS A 1 A_Chase
    Loop
  Melee:
    POSS B 1
    Goto See
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let buddy = classes.find_name("Buddy").unwrap();
        let id = w
            .spawn_at(buddy, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.reaction_time = 0;
        }
        let x0 = w.actors.get(id).unwrap().pos.x();
        // No enemy: should walk toward player via `P_Move`, never enter Melee.
        match a_chase(&mut w, id) {
            ActionResult::Jump(_) => panic!("friend must not attack player"),
            ActionResult::Continue => {}
        }
        let x1 = w.actors.get(id).unwrap().pos.x();
        assert!(x1 < x0, "expected westward follow step, {x0} -> {x1}");
        // Extra chase steps so position keeps closing.
        for _ in 0..20 {
            match a_chase(&mut w, id) {
                ActionResult::Jump(_) => panic!("friend must not attack player"),
                ActionResult::Continue => {}
            }
            crate::physics::xy_movement(&mut w, id);
        }
        let x1 = w.actors.get(id).unwrap().pos.x();
        assert!(x1 < x0, "expected friend to approach player ({x1} < {x0})");
    }

    #[test]
    fn a_look_friendly_ignores_players() {
        let ir = parse_decorate(
            r#"
actor Buddy : Actor 3004
{
  SeeSound "grunt/sight"
  +FRIENDLY
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS A 4 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let buddy = classes.find_name("Buddy").unwrap();
        assert_eq!(
            classes.get(buddy).unwrap().flags & crate::class::FLAG_FRIENDLY,
            crate::class::FLAG_FRIENDLY
        );
        let id = w
            .spawn_at(buddy, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_look(&mut w, id), ActionResult::Continue);
        assert!(w.drain_sound_events().is_empty());
    }

    #[test]
    fn a_look_friendly_wakes_on_enemy_monster() {
        let ir = parse_decorate(
            r#"
actor Buddy : Actor 9001
{
  SeeSound "grunt/sight"
  +FRIENDLY
  +SHOOTABLE
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS A 4 A_Chase
    Loop
  }
}
actor Foe : Actor 9002
{
  Health 20
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: TROO A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let buddy = classes.find_name("Buddy").unwrap();
        let foe = classes.find_name("Foe").unwrap();
        let id = w
            .spawn_at(buddy, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let enemy = w
            .spawn_at(foe, DVec3::new(140.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let see = w.states.find_label(buddy, "See").unwrap();
        match a_look(&mut w, id) {
            ActionResult::Jump(j) => assert_eq!(j, see),
            other => panic!("expected Jump to See, got {other:?}"),
        }
        assert_eq!(w.actors.get(id).unwrap().target, Some(enemy));
    }

    #[test]
    fn a_look_ignores_notarget_player() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SOLID
  +SHOOTABLE
  +NOTARGET
  States { Spawn: PLAY A -1 Stop }
}
actor ZombieMan : Actor 3004
{
  SeeSound "grunt/sight"
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS A 4 A_Chase
    Loop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_ne!(
            w.actors.get(mo).unwrap().flags & crate::class::FLAG_NOTARGET,
            0
        );
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_look(&mut w, id), ActionResult::Continue);
        assert!(w.drain_sound_events().is_empty());
    }

    #[test]
    fn a_play_sound_queues_named_sfx() {
        let ir = parse_decorate(
            r#"
actor Noisy : Actor 9994
{
  States
  {
  Spawn:
    POSS A 1 A_PlaySound("weapons/pistol", 0.5)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let noisy = classes.find_name("Noisy").unwrap();
        let spawn = states.find_label(noisy, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(noisy, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        let sounds = w.drain_sound_events();
        assert!(
            sounds
                .iter()
                .any(|e| e.name == "weapons/pistol" && (e.volume - 0.5).abs() < 1e-6),
            "{sounds:?}"
        );
    }

    #[test]
    fn a_throw_grenade_and_spawn_debris() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Bomb : Actor 9030
{
  Speed 10
  Radius 6
  Height 8
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Chunk : Actor 9031
{
  Health 1
  Radius 4
  Height 4
  States { Spawn: POSS A -1 Stop }
}
actor Thrower : Actor 9032
{
  Health 50
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_ThrowGrenade("Bomb", 32, 12, 8)
    POSS A 1 A_SpawnDebris("Chunk", 0, 1, 1)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let thrower = classes.find_name("Thrower").unwrap();
        let bomb = classes.find_name("Bomb").unwrap();
        let chunk = classes.find_name("Chunk").unwrap();
        let spawn = states.find_label(thrower, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(thrower, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert!(
            w.thinkers
                .default
                .iter()
                .any(|&oid| oid != id && w.actors.get(oid).is_some_and(|a| a.class == bomb))
        );
        let bombs: Vec<_> = w
            .thinkers
            .default
            .iter()
            .filter_map(|&oid| w.actors.get(oid).filter(|a| a.class == bomb).map(|_| oid))
            .collect();
        assert!(!bombs.is_empty());
        let b = w.actors.get(bombs[0]).unwrap();
        assert!(b.vel.z() > 0.0, "grenade should have loft");
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        let chunks = w
            .thinkers
            .default
            .iter()
            .filter(|&&oid| w.actors.get(oid).is_some_and(|a| a.class == chunk))
            .count();
        assert!(chunks >= 8, "debris burst expected, got {chunks}");
    }

    #[test]
    fn a_custom_punch_and_fire_bullets() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Dummy : Actor 9020
{
  Health 80
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Brawler : Actor 9021
{
  Health 50
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_CustomPunch(10, 1)
    POSS A 1 A_FireBullets(0, 0, 1, 15, 0, "", 2048)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let brawler = classes.find_name("Brawler").unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        let spawn = states.find_label(brawler, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let d1 = w
            .spawn_at(dummy, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let id = w
            .spawn_at(brawler, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.angle = Angle::ZERO;
            a.target = Some(d1);
        }
        let hp0 = w.actors.get(d1).unwrap().health;
        w.set_state(id, Some(spawn), true);
        let hp1 = w.actors.get(d1).unwrap().health;
        assert!(hp1 < hp0, "custom punch should damage");
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert!(
            w.actors.get(d1).unwrap().health < hp1,
            "fire bullets should damage"
        );
    }

    #[test]
    fn a_stop_sound_clears_queued_sfx() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_PlaySound", &["weapons/pistol".into()]),
            ActionResult::Continue
        );
        assert!(!w.sound_events.is_empty());
        assert_eq!(
            call_action(&mut w, mo, "A_StopSound"),
            ActionResult::Continue
        );
        assert!(w.sound_events.is_empty());
    }

    #[test]
    fn a_radius_thrust_pushes_without_damage() {
        let ir = parse_decorate(
            r#"
actor Boom : Actor 9019
{
  States { Spawn: BAR1 A -1 Stop }
}
actor Dummy : Actor 9020
{
  Health 100
  Mass 50
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let boom = classes.find_name("Boom").unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bar = w
            .spawn_at(boom, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let id = w
            .spawn_at(dummy, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(&mut w, bar, "A_RadiusThrust", &["128".into(), "128".into()],),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().health, 100);
        assert!(w.actors.get(id).unwrap().vel.x() > 0.0);
    }

    #[test]
    fn a_brain_spit_spawns_cube_then_monster() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let eye = classes.find_name("BossEye").unwrap();
        let target = classes.find_name("BossTarget").unwrap();
        let shot = classes.find_name("SpawnShot").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let eye_id = w
            .spawn_at(eye, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _t = w
            .spawn_at(target, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let before = w.actors.len();
        assert_eq!(
            call_action_args(&mut w, eye_id, "A_BrainSpit", &[]),
            ActionResult::Continue
        );
        assert!(w.actors.len() > before);
        let cube = w
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                w.actors
                    .get(id)
                    .is_some_and(|a| a.class == shot && !a.destroyed)
            })
            .unwrap();
        let flight = w.actors.get(cube).unwrap().reaction_time;
        assert!(flight > 0);
        for _ in 0..flight {
            assert_eq!(
                call_action(&mut w, cube, "A_SpawnFly"),
                ActionResult::Continue
            );
        }
        assert!(w.actors.get(cube).unwrap().destroyed);
        let monsters = w
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                w.actors.get(id).is_some_and(|a| {
                    !a.destroyed
                        && a.class != eye
                        && a.class != target
                        && a.class != shot
                        && w.classes
                            .get(a.class)
                            .is_some_and(|c| (c.flags & crate::class::FLAG_COUNTKILL) != 0)
                })
            })
            .count();
        assert!(monsters >= 1, "expected a boss-spawned monster");
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "brain/spit" || e.name == "brain/spawn")
        );
    }

    #[test]
    fn a_brain_die_exits_level() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let brain = classes.find_name("BossBrain").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let id = w
            .spawn_at(brain, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action(&mut w, id, "A_BrainDie"),
            ActionResult::Continue
        );
        assert!(w.level.exiting);
    }

    #[test]
    fn a_brain_explode_spawns_rocket_death() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let brain = classes.find_name("BossBrain").unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP30")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let id = w
            .spawn_at(brain, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == rocket))
            .count();
        assert_eq!(
            call_action(&mut w, id, "A_BrainExplode"),
            ActionResult::Continue
        );
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == rocket))
            .count();
        assert!(after > before, "A_BrainExplode should spawn a Rocket");
        let boom = w.thinkers.default.iter().copied().find(|&aid| {
            w.actors
                .get(aid)
                .is_some_and(|a| a.class == rocket && (a.flags & crate::class::FLAG_MISSILE) == 0)
        });
        assert!(boom.is_some(), "brain rocket should clear MISSILE");
    }

    #[test]
    fn a_brain_scream_spawns_many_rockets() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let brain = classes.find_name("BossBrain").unwrap();
        let rocket = classes.find_name("Rocket").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 256.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP30")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let id = w
            .spawn_at(brain, DVec3::new(400.0, 400.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action(&mut w, id, "A_BrainScream"),
            ActionResult::Continue
        );
        let rockets = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| w.actors.get(aid).is_some_and(|a| a.class == rocket))
            .count();
        // (320 - (-196)) / 8 ≈ 64 spawns.
        assert!(rockets >= 60, "expected ~64 brain rockets, got {rockets}");
        assert!(
            w.sound_events
                .iter()
                .any(|e| e.name == "brain/death" || e.name.contains("brain")),
            "brain death sound should play"
        );
    }

    #[test]
    fn a_quake_damages_player_and_counts_down() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let player = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let hp0 = w.actors.get(player).unwrap().health;
        let id = w
            .spawn_at(
                classes.find_name("Actor").unwrap(),
                DVec3::new(80.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_Quake",
                &[
                    "5".into(),
                    "64".into(),
                    "256".into(),
                    "256".into(),
                    "world/quake".into(),
                ],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.quakes.len(), 1);
        assert!(w.quake_intensity_at(player) > 0.0);
        let mut damaged = false;
        for _ in 0..64 {
            w.tick();
            if w.actors.get(player).unwrap().health < hp0 {
                damaged = true;
                break;
            }
        }
        assert!(damaged, "quake should damage player within 64 tics");
        while !w.quakes.is_empty() {
            w.tick();
        }
        assert!(w.quakes.is_empty());
    }

    #[test]
    fn a_check_floor_and_ceiling() {
        let ir = parse_decorate(
            r#"
actor Floater : Actor 9018
{
  Height 56
  States
  {
  Spawn:
    POSS A 0 A_CheckFloor("OnFloor")
    POSS A 0 A_CheckCeiling("OnCeil")
    POSS A -1
    Stop
  OnFloor:
    POSS B -1
    Stop
  OnCeil:
    POSS C -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let floater = classes.find_name("Floater").unwrap();
        let spawn = states.find_label(floater, "Spawn").unwrap();
        let on_floor = states.find_label(floater, "OnFloor").unwrap();
        let on_ceil = states.find_label(floater, "OnCeil").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(floater, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(on_floor));
        // Mid-air: neither floor nor ceiling → fall through Spawn to terminal frame.
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(64.0, 64.0, 40.0);
        }
        w.set_state(id, Some(spawn), true);
        let mid = w.actors.get(id).unwrap().state;
        assert_ne!(mid, Some(on_floor));
        assert_ne!(mid, Some(on_ceil));
        // Head against ceiling (floor 0, ceil 128, height 56 → z = 72).
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(64.0, 64.0, 72.0);
        }
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(on_ceil));
    }

    #[test]
    fn a_check_range_sight_and_sight_or_range() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Scout : Actor 9017
{
  States
  {
  Spawn:
    POSS A 0 A_CheckRange(50, "Far")
    POSS A 0 A_CheckSight("Seen")
    POSS A -1
    Stop
  Far:
    POSS B 0 A_CheckSightOrRange(50, "Bad")
    POSS B -1
    Stop
  Seen:
    POSS C -1
    Stop
  Bad:
    POSS D -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let scout = classes.find_name("Scout").unwrap();
        let spawn = states.find_label(scout, "Spawn").unwrap();
        let bad = states.find_label(scout, "Bad").unwrap();
        let seen = states.find_label(scout, "Seen").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        // ~136 units away → CheckRange(50) → Far → CheckSightOrRange(50) → Bad.
        let id = w
            .spawn_at(scout, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(bad));
        // Close + LOS → CheckRange does not jump; CheckSight → Seen.
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(80.0, 64.0, 0.0);
        }
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(seen));
    }

    #[test]
    fn a_fade_to_and_print() {
        let ir = parse_decorate(
            r#"
actor Ghost : Actor 9016
{
  Alpha 1.0
  States
  {
  Spawn:
    POSS A 1 A_FadeTo(0.25, 0.5)
    POSS A 1 A_Print("Hello")
    POSS A 1 A_LogInt(42)
    POSS A 1 A_LogFloat(1.5)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let ghost = classes.find_name("Ghost").unwrap();
        let spawn = states.find_label(ghost, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(ghost, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert!((w.actors.get(id).unwrap().alpha - 0.5).abs() < 1e-9);
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_eq!(w.drain_messages(), vec!["Hello".to_owned()]);
        let log_int = states.get(next).unwrap().next.unwrap();
        w.set_state(id, Some(log_int), true);
        assert_eq!(w.drain_messages(), vec!["42".to_owned()]);
        let log_float = states.get(log_int).unwrap().next.unwrap();
        w.set_state(id, Some(log_float), true);
        assert_eq!(w.drain_messages(), vec!["1.5".to_owned()]);
        // Second FadeTo call would reach 0.25.
        assert_eq!(
            call_action_args(&mut w, id, "A_FadeTo", &["0.25".into(), "0.5".into()]),
            ActionResult::Continue
        );
        assert!((w.actors.get(id).unwrap().alpha - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_set_inventory_and_jump_if_in_target_inventory() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor Checker : Actor 9015
{
  States
  {
  Spawn:
    POSS A 1 A_SetInventory("Clip", 3)
    POSS A 1 A_JumpIfInTargetInventory("Clip", 5, "Yes")
    POSS A -1
    Stop
  Yes:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let checker = classes.find_name("Checker").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let spawn = states.find_label(checker, "Spawn").unwrap();
        let yes = states.find_label(checker, "Yes").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(checker, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        w.set_state(id, Some(spawn), true);
        assert_eq!(crate::inventory::count_inventory(&w, id, clip), 3);
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_ne!(w.actors.get(id).unwrap().state, Some(yes));
        crate::inventory::set_inventory(&mut w, player, clip, 8);
        w.set_state(id, Some(next), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(yes));
    }

    #[test]
    fn a_seeker_missile_homes_within_threshold() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Seeker : Actor 9014
{
  States { Spawn: BAL1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let seeker = classes.find_name("Seeker").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(seeker, DVec3::new(200.0, 64.0, 0.0), Angle::DEG_90)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.tracer = Some(player);
            a.vel = DVec3::new(0.0, 20.0, 0.0);
        }
        // Facing 90°, aim is west (~180°) → within 90° threshold; turn up to 20°.
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SeekerMissile",
                &["90".into(), "20".into(), "0".into()],
            ),
            ActionResult::Continue
        );
        let ang = w.actors.get(id).unwrap().angle.degrees();
        assert!(ang > 90.0 && ang <= 110.0, "ang={ang}");
        // Outside cone: facing away.
        if let Some(a) = w.actors.get_mut(id) {
            a.angle = Angle::ZERO; // east, aim west = 180° > 30° threshold
        }
        let before = w.actors.get(id).unwrap().angle.degrees();
        assert_eq!(
            call_action_args(&mut w, id, "A_SeekerMissile", &["30".into(), "20".into()],),
            ActionResult::Continue
        );
        assert!((w.actors.get(id).unwrap().angle.degrees() - before).abs() < 1e-9);
    }

    #[test]
    fn a_warp_to_target_and_tid() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Anchor : Actor 9012
{
  States { Spawn: TFOG A -1 Stop }
}
actor Warper : Actor 9013
{
  States
  {
  Spawn:
    POSS A 1 A_Warp("Target", 16, 0, 8, 90)
    POSS A 1 A_Warp("99", 0, 0, 0)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let warper = classes.find_name("Warper").unwrap();
        let anchor = classes.find_name("Anchor").unwrap();
        let spawn = states.find_label(warper, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let aid = w
            .spawn_at(
                anchor,
                DVec3::new(200.0, 100.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(aid) {
            a.tid = 99;
        }
        let id = w
            .spawn_at(warper, DVec3::new(10.0, 10.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        w.set_state(id, Some(spawn), true);
        {
            let a = w.actors.get(id).unwrap();
            // Player at (64,64) facing 0° → offset +16x → (80,64,8); angle = 0+90.
            assert!((a.pos.x() - 80.0).abs() < 1e-6);
            assert!((a.pos.y() - 64.0).abs() < 1e-6);
            assert!((a.pos.z() - 8.0).abs() < 1e-6);
            assert!((a.angle.degrees() - 90.0).abs() < 1e-6);
        }
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.x() - 200.0).abs() < 1e-6);
        assert!((a.pos.y() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn a_transfer_pointer_and_missile_tracer() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Ball : Actor 9010
{
  Speed 20
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Gunner : Actor 9011
{
  States
  {
  Spawn:
    POSS A 1 A_TransferPointer("Target", "Tracer")
    POSS A 1 A_CustomMissile("Ball", 32, 0, 0)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let gunner = classes.find_name("Gunner").unwrap();
        let ball = classes.find_name("Ball").unwrap();
        let spawn = states.find_label(gunner, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(gunner, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().tracer, Some(player));
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        let missile = w
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&oid| oid != id && w.actors.get(oid).is_some_and(|a| a.class == ball))
            .unwrap();
        let m = w.actors.get(missile).unwrap();
        assert_eq!(m.target, Some(id));
        assert_eq!(m.master, Some(id));
        assert_eq!(m.tracer, Some(player));
    }

    #[test]
    fn a_call_special_exit_and_teleport_by_actor_tid() {
        let ir = parse_decorate(
            r#"
actor Dest : Actor 9008
{
  States { Spawn: TFOG A -1 Stop }
}
actor Caller : Actor 9009
{
  States
  {
  Spawn:
    POSS A 1 A_CallSpecial(243)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let caller = classes.find_name("Caller").unwrap();
        let dest = classes.find_name("Dest").unwrap();
        let spawn = states.find_label(caller, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let dest_id = w
            .spawn_at(
                dest,
                DVec3::new(180.0, 100.0, 0.0),
                Angle::from_degrees(45.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(dest_id) {
            a.tid = 7;
        }
        let id = w
            .spawn_at(caller, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert!(w.level.exiting);
        assert_eq!(
            crate::specials::execute_special(&mut w, 70, &[7, 0, 0, 0, 0], Some(mo), None),
            1
        );
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - 180.0).abs() < 1e-6);
        assert!((a.pos.y() - 100.0).abs() < 1e-6);
        assert!((a.angle.degrees() - 45.0).abs() < 1e-6);
    }

    #[test]
    fn a_set_tid_and_find_by_tid() {
        let ir = parse_decorate(
            r#"
actor Tagged : Actor 9007
{
  States
  {
  Spawn:
    POSS A 1 A_SetTid(42)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let tagged = classes.find_name("Tagged").unwrap();
        let spawn = states.find_label(tagged, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(tagged, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(w.find_by_tid(42).is_none());
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().tid, 42);
        assert_eq!(w.find_by_tid(42), Some(id));
        assert!(w.find_by_tid(0).is_none());
    }

    #[test]
    fn a_set_special_and_set_arg() {
        let ir = parse_decorate(
            r#"
actor Thingy : Actor 9006
{
  States
  {
  Spawn:
    POSS A 1 A_SetSpecial(80, 1, 2, 3, 4, 5)
    POSS A 1 A_SetArg(2, 99)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let thingy = classes.find_name("Thingy").unwrap();
        let spawn = states.find_label(thingy, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(thingy, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        {
            let a = w.actors.get(id).unwrap();
            assert_eq!(a.special, 80);
            assert_eq!(a.args, [1, 2, 3, 4, 5]);
        }
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_eq!(w.actors.get(id).unwrap().args[2], 99);
    }

    #[test]
    fn a_damage_self_hurts_caller() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        assert!(matches!(
            call_action_args(&mut w, mo, "A_DamageSelf", &["15".into()]),
            ActionResult::Continue
        ));
        assert_eq!(w.actors.get(mo).unwrap().health, hp0 - 15);
        let dtype = w.actors.get(mo).unwrap().damage_type.clone();
        assert!(matches!(
            call_action_args(&mut w, mo, "A_DamageSelf", &["5".into(), "Fire".into()]),
            ActionResult::Continue
        ));
        assert_eq!(w.actors.get(mo).unwrap().health, hp0 - 20);
        assert_eq!(
            w.actors.get(mo).unwrap().damage_type,
            dtype,
            "mod should not stick on the actor"
        );
    }

    #[test]
    fn a_set_damage_type_and_radius_give() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor Giver : Actor 9005
{
  DamageType Ice
  States
  {
  Spawn:
    POSS A 1 A_SetDamageType("Fire")
    POSS A 1 A_RadiusGive("Clip", 80, 0, 7)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let giver = classes.find_name("Giver").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let spawn = states.find_label(giver, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(giver, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.actors.get(id).unwrap().damage_type, "Ice");
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().damage_type, "Fire");
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_eq!(crate::inventory::count_inventory(&w, player, clip), 7);
        assert_eq!(crate::inventory::count_inventory(&w, id, clip), 0);
    }

    #[test]
    fn a_custom_missile_and_bullet_attack() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Ball : Actor 9003
{
  Speed 20
  Radius 6
  Height 8
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Gunner : Actor 9004
{
  Health 50
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_CustomMissile("Ball", 32, 0, 0)
    POSS A 1 A_CustomBulletAttack(0, 0, 1, 10, 2048)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let gunner = classes.find_name("Gunner").unwrap();
        let ball = classes.find_name("Ball").unwrap();
        let spawn = states.find_label(gunner, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp = w.actors.get(player).unwrap().health;
        let id = w
            .spawn_at(gunner, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
        }
        w.set_state(id, Some(spawn), true);
        assert!(
            w.thinkers
                .default
                .iter()
                .any(|&oid| oid != id && w.actors.get(oid).is_some_and(|a| a.class == ball))
        );
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert!(w.actors.get(player).unwrap().health < hp);
    }

    #[test]
    fn a_rail_attack_pierces_multiple_targets() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Dummy : Actor 9010
{
  Health 50
  Radius 20
  Height 56
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Railer : Actor 9011
{
  Health 50
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_RailAttack(20)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let railer = classes.find_name("Railer").unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        let spawn = states.find_label(railer, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let d1 = w
            .spawn_at(dummy, DVec3::new(200.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let d2 = w
            .spawn_at(dummy, DVec3::new(400.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let id = w
            .spawn_at(railer, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(d1);
            a.angle = Angle::ZERO;
        }
        let h1 = w.actors.get(d1).unwrap().health;
        let h2 = w.actors.get(d2).unwrap().health;
        w.set_state(id, Some(spawn), true);
        assert!(w.actors.get(d1).unwrap().health < h1);
        assert!(w.actors.get(d2).unwrap().health < h2);
    }

    #[test]
    fn a_custom_melee_and_combo_attack() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Ball : Actor 9005
{
  Speed 20
  Radius 6
  Height 8
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Brawler : Actor 9006
{
  Health 50
  Radius 20
  Height 56
  MeleeRange 128
  States
  {
  Spawn:
    POSS A 1 A_CustomMeleeAttack(15, "", "")
    POSS A 1 A_CustomComboAttack("Ball", 32, 10, "")
    POSS A -1
    Stop
  Far:
    POSS A 1 A_CustomComboAttack("Ball", 32, 10, "")
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let brawler = classes.find_name("Brawler").unwrap();
        let ball = classes.find_name("Ball").unwrap();
        let spawn = states.find_label(brawler, "Spawn").unwrap();
        let far = states.find_label(brawler, "Far").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(player).unwrap().health;
        let id = w
            .spawn_at(brawler, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(player);
            a.melee_range = 128.0;
        }
        w.set_state(id, Some(spawn), true);
        let hp1 = w.actors.get(player).unwrap().health;
        assert!(hp1 < hp0, "melee should damage");
        // Move far away so combo fires a missile.
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(400.0, 64.0, 0.0);
        }
        let balls_before = w
            .thinkers
            .default
            .iter()
            .filter(|&&oid| w.actors.get(oid).is_some_and(|a| a.class == ball))
            .count();
        w.set_state(id, Some(far), true);
        let balls_after = w
            .thinkers
            .default
            .iter()
            .filter(|&&oid| w.actors.get(oid).is_some_and(|a| a.class == ball))
            .count();
        assert!(
            balls_after > balls_before,
            "combo should spawn missile when out of melee"
        );
    }

    #[test]
    fn a_spawn_item_ex_offsets_relative_to_yaw() {
        let ir = parse_decorate(
            r#"
actor Drop : Actor 9001
{
  Health 1
  States { Spawn: BAL1 A -1 Stop }
}
actor Spawner : Actor 9002
{
  States
  {
  Spawn:
    POSS A 1 A_SpawnItemEx("Drop", 16, 0, 8, 4, 0, 0, 0)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let spawner = classes.find_name("Spawner").unwrap();
        let drop = classes.find_name("Drop").unwrap();
        let spawn = states.find_label(spawner, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(spawner, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        let child = w
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&oid| oid != id && w.actors.get(oid).is_some_and(|a| a.class == drop));
        let child = child.unwrap();
        let a = w.actors.get(child).unwrap();
        assert!((a.pos.x() - 80.0).abs() < 1e-6);
        assert!((a.pos.y() - 64.0).abs() < 1e-6);
        assert!((a.pos.z() - 8.0).abs() < 1e-6);
        assert!((a.vel.x() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn a_change_velocity_scale_and_set_pitch() {
        let ir = parse_decorate(
            r#"
actor Mover : Actor 9999
{
  States
  {
  Spawn:
    POSS A 1 A_SetPitch(15)
    POSS A 1 A_ChangeVelocity(10, 0, 2, 1)
    POSS A 1 A_ScaleVelocity(0.5)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mover = classes.find_name("Mover").unwrap();
        let spawn = states.find_label(mover, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(mover, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert!((w.actors.get(id).unwrap().pitch.degrees() - 15.0).abs() < 1e-9);
        // Relative +X along facing (0°) → vel (10,0,2), then scale 0.5 → (5,0,1).
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        {
            let a = w.actors.get(id).unwrap();
            assert!((a.vel.x() - 10.0).abs() < 1e-6);
            assert!(a.vel.y().abs() < 1e-6);
            assert!((a.vel.z() - 2.0).abs() < 1e-6);
        }
        let next2 = states.get(next).unwrap().next.unwrap();
        w.set_state(id, Some(next2), true);
        {
            let a = w.actors.get(id).unwrap();
            assert!((a.vel.x() - 5.0).abs() < 1e-6);
            assert!((a.vel.z() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn a_die_enters_death_and_a_remove_destroys() {
        let ir = parse_decorate(
            r#"
actor Mortal : Actor 9997
{
  Health 50
  +SHOOTABLE
  +SOLID
  +COUNTKILL
  States
  {
  Spawn:
    POSS A -1
    Stop
  Death:
    POSS B -1
    Stop
  }
}
actor Ephemeral : Actor 9998
{
  Health 10
  +SOLID
  States
  {
  Spawn:
    BAL1 A 1 A_Remove
    BAL1 A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mortal = classes.find_name("Mortal").unwrap();
        let ephemeral = classes.find_name("Ephemeral").unwrap();
        let death = states.find_label(mortal, "Death").unwrap();
        let espawn = states.find_label(ephemeral, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mid = w
            .spawn_at(mortal, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.level.total_kills, 1);
        assert_eq!(call_action(&mut w, mid, "A_Die"), ActionResult::Continue);
        assert_eq!(w.actors.get(mid).unwrap().state, Some(death));
        assert_eq!(w.actors.get(mid).unwrap().health, 0);
        assert_eq!(w.level.found_kills, 1);
        let eid = w
            .spawn_at(ephemeral, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(eid, Some(espawn), true);
        assert!(w.actors.get(eid).unwrap().destroyed);
    }

    #[test]
    fn a_clear_target_and_jump_if_target_los() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Scout : Actor 9996
{
  States
  {
  Spawn:
    POSS A 1 A_JumpIfNoTarget("NoTgt")
    POSS A -1
    Stop
  NoTgt:
    POSS B 1 A_JumpIfTargetInLOS("Seen")
    POSS B -1
    Stop
  Seen:
    POSS D 0 A_ClearTarget
    POSS D 1 A_JumpIfNoTarget("Cleared")
    POSS D -1
    Stop
  Cleared:
    POSS E -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let scout = classes.find_name("Scout").unwrap();
        let spawn = states.find_label(scout, "Spawn").unwrap();
        let no_tgt = states.find_label(scout, "NoTgt").unwrap();
        let cleared = states.find_label(scout, "Cleared").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(scout, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(no_tgt));
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(mo);
        }
        w.set_state(id, Some(no_tgt), true);
        // In LOS → Seen → ClearTarget → JumpIfNoTarget → Cleared.
        assert_eq!(w.actors.get(id).unwrap().state, Some(cleared));
        assert!(w.actors.get(id).unwrap().target.is_none());
        assert_eq!(
            call_action_args(&mut w, id, "A_JumpIfTargetInLOS", &["Seen".into()]),
            ActionResult::Continue
        );
    }

    #[test]
    fn a_look_sets_target_and_give_take_inventory() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SOLID
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor ZombieMan : Actor 3004
{
  SeeSound "grunt/sight"
  States
  {
  Spawn:
    POSS A 10 A_Look
    Loop
  See:
    POSS A 4 A_Chase
    Loop
  Gift:
    POSS A 0 A_GiveInventory("Clip", 20)
    POSS A 0 A_TakeInventory("Clip", 5)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let gift = states.find_label(zombie, "Gift").unwrap();
        let see = states.find_label(zombie, "See").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(a_look(&mut w, id), ActionResult::Jump(see));
        assert_eq!(w.actors.get(id).unwrap().target, Some(mo));
        w.set_state(id, Some(gift), true);
        assert_eq!(crate::inventory::count_inventory(&w, id, clip), 15);
    }

    #[test]
    fn a_set_translation_and_jump_if_inventory() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor Holder : Actor 9995
{
  Translation "Ice"
  States
  {
  Spawn:
    POSS A 1 A_SetTranslation("None")
    POSS A 1 A_JumpIfInventory("Clip", 5, "Rich")
    POSS A -1
    Stop
  Rich:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let holder = classes.find_name("Holder").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let spawn = states.find_label(holder, "Spawn").unwrap();
        let rich = states.find_label(holder, "Rich").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(holder, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.actors.get(id).unwrap().translation, "Ice");
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().translation, "None");
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_ne!(w.actors.get(id).unwrap().state, Some(rich));
        assert!(crate::inventory::give_inventory(&mut w, id, clip, 10));
        w.set_state(id, Some(next), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(rich));
    }

    #[test]
    fn a_set_render_style_and_fade_out() {
        let ir = parse_decorate(
            r#"
actor Ghost : Actor 9993
{
  Alpha 1.0
  States
  {
  Spawn:
    POSS A 1 A_SetRenderStyle(0.5, "Add")
    POSS A 1 A_FadeOut(0.6)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let ghost = classes.find_name("Ghost").unwrap();
        let spawn = states.find_label(ghost, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(ghost, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!((w.actors.get(id).unwrap().alpha - 1.0).abs() < 1e-9);
        w.set_state(id, Some(spawn), true);
        assert!((w.actors.get(id).unwrap().alpha - 0.5).abs() < 1e-9);
        assert_eq!(
            w.actors.get(id).unwrap().render_style,
            crate::class::RenderStyle::Add
        );
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert!(w.actors.get(id).unwrap().destroyed);
    }

    #[test]
    fn a_jump_if_closer_and_check_flag() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Scout : Actor 9992
{
  +SOLID
  States
  {
  Spawn:
    POSS A 1 A_CheckFlag("SOLID", "SolidOk")
    POSS A -1
    Stop
  SolidOk:
    POSS B 1 A_JumpIfCloser(200, "Near")
    POSS B -1
    Stop
  Near:
    POSS C -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let scout = classes.find_name("Scout").unwrap();
        let spawn = states.find_label(scout, "Spawn").unwrap();
        let near = states.find_label(scout, "Near").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(scout, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        // CheckFlag → SolidOk, then JumpIfCloser → Near.
        assert_eq!(w.actors.get(id).unwrap().state, Some(near));
    }

    #[test]
    fn a_clear_last_heard_clears_soundtarget() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.sectors[0].soundtarget = Some(mo);
        assert_eq!(
            call_action(&mut w, mo, "A_ClearLastHeard"),
            ActionResult::Continue
        );
        assert!(w.sectors[0].soundtarget.is_none());
    }

    #[test]
    fn a_spawn_particle_queues_event() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(100.0, 100.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SpawnParticle",
                &[
                    "red".into(),
                    "1".into(),
                    "20".into(),
                    "8".into(),
                    "0".into(),
                    "0".into(),
                    "0".into(),
                    "16".into(),
                ],
            ),
            ActionResult::Continue
        );
        let ev = w.drain_particle_events();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].fullbright);
        assert_eq!(ev[0].lifetime, 20);
        assert!((ev[0].z - 16.0 - w.actors.get(id).unwrap().height * 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_spray_decal_queues_event() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(100.0, 100.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SprayDecal",
                &["BulletChip".into(), "16".into()],
            ),
            ActionResult::Continue
        );
        let ev = w.drain_decal_events();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].name, "BulletChip");
        assert!((ev[0].x - 116.0).abs() < 1e-6);
        assert!((ev[0].y - 100.0).abs() < 1e-6);
    }

    #[test]
    fn a_set_and_swap_tele_fog() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(128.0, 128.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SetTeleFog",
                &["TeleportFog".into(), "Blood".into()],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().telefog_source, "TeleportFog");
        assert_eq!(w.actors.get(id).unwrap().telefog_dest, "Blood");
        assert_eq!(
            call_action(&mut w, id, "A_SwapTeleFog"),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().telefog_source, "Blood");
        assert_eq!(w.actors.get(id).unwrap().telefog_dest, "TeleportFog");
        assert_eq!(
            call_action_args(&mut w, id, "A_Log", &["logged".into()]),
            ActionResult::Continue
        );
        assert!(w.drain_messages().iter().any(|m| m == "logged"));
    }

    #[test]
    fn a_set_damage_overrides_missile_base() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(128.0, 128.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(&mut w, id, "A_SetDamage", &["42".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().damage, 42);
    }

    #[test]
    fn a_quake_ex_and_set_blend() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(
                classes.find_name("Actor").unwrap(),
                DVec3::new(80.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_QuakeEx",
                &[
                    "3".into(),
                    "5".into(),
                    "1".into(),
                    "20".into(),
                    "64".into(),
                    "128".into(),
                ],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.quakes.len(), 1);
        assert!((w.quakes[0].intensity - 5.0).abs() < 1e-9);
        assert_eq!(w.quakes[0].countdown, 20);
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SetBlend",
                &["red".into(), "0.5".into(), "10".into()],
            ),
            ActionResult::Continue
        );
        assert!((w.screen_fade.target_r - 1.0).abs() < 1e-5);
        assert!((w.screen_fade.target_a - 0.5).abs() < 1e-5);
        assert_eq!(w.screen_fade.tics_left, 10);
    }

    #[test]
    fn a_face_movement_and_alert_monsters() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(id).unwrap().vel = DVec3::new(0.0, 8.0, 0.0);
        assert_eq!(
            call_action(&mut w, id, "A_FaceMovementDirection"),
            ActionResult::Continue
        );
        assert!((w.actors.get(id).unwrap().angle.degrees() - 90.0).abs() < 1e-6);
        let sleeper = w
            .spawn_at(z, DVec3::new(240.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(id).unwrap().target = Some(mo);
        assert_eq!(
            call_action_args(&mut w, id, "A_AlertMonsters", &["128".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(sleeper).unwrap().target, Some(mo));
        assert!(w.sectors.iter().any(|s| s.soundtarget == Some(mo)));
    }

    #[test]
    fn a_set_thresholds_and_jump_higher_lower() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let high = w
            .spawn_at(z, DVec3::new(200.0, 200.0, 40.0), Angle::ZERO)
            .unwrap();
        let low = w
            .spawn_at(z, DVec3::new(220.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(high).unwrap().target = Some(low);
        assert_eq!(
            call_action_args(&mut w, high, "A_SetPainThreshold", &["40".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(high).unwrap().pain_threshold, 40);
        assert_eq!(
            call_action_args(&mut w, high, "A_SetThreshold", &["12".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(high).unwrap().threshold, 12);
        // Need labels — use Jump via action args resolving StateTable labels on ZombieMan.
        // ZombieMan has See / Pain; use See as "high" and Pain as "low".
        let see = states.find_label(z, "See").unwrap();
        assert_eq!(
            call_action_args(
                &mut w,
                high,
                "A_JumpIfHigherOrLower",
                &["See".into(), "Pain".into()],
            ),
            ActionResult::Jump(see)
        );
    }

    #[test]
    fn a_set_float_bob_phase_and_weave() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(z, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(&mut w, id, "A_SetFloatBobPhase", &["16".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().float_bob_phase, 16);
        let before = w.actors.get(id).unwrap().pos;
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_Weave",
                &["2".into(), "2".into(), "4".into(), "4".into()],
            ),
            ActionResult::Continue
        );
        let after = w.actors.get(id).unwrap().pos;
        assert!(
            (after.x() - before.x()).abs() > 1e-6
                || (after.y() - before.y()).abs() > 1e-6
                || (after.z() - before.z()).abs() > 1e-6
        );
        assert_eq!(w.actors.get(id).unwrap().float_bob_phase, 17);
    }

    #[test]
    fn a_look_ex_fov_and_melee_range_jumps() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor Scout : Actor 9930
{
  Health 20
  Radius 20
  Height 56
  MeleeRange 80
  +SHOOTABLE
  Monster
  States
  {
  Spawn:
    POSS A 1 A_LookEx(0, 0, 0, 0, 90)
    POSS A -1
    Stop
  See:
    POSS B -1
    Stop
  Far:
    POSS C -1
    Stop
  Near:
    POSS D -1
    Stop
  }
}
"#;
        let ir = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let scout = classes.find_name("Scout").unwrap();
        let spawn = states.find_label(scout, "Spawn").unwrap();
        let see = states.find_label(scout, "See").unwrap();
        let far = states.find_label(scout, "Far").unwrap();
        let near = states.find_label(scout, "Near").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(200.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(scout, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(see));
        assert!(w.actors.get(id).unwrap().target.is_some());

        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_JumpIfTargetOutsideMeleeRange",
                &["Far".into()],
            ),
            ActionResult::Jump(far)
        );
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().pos = DVec3::new(100.0, 64.0, 0.0);
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_JumpIfTargetInsideMeleeRange",
                &["Near".into()],
            ),
            ActionResult::Jump(near)
        );
    }

    #[test]
    fn a_rearrange_pointers_swaps_and_nulls() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let z = classes.find_name("ZombieMan").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let a = w
            .spawn_at(z, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let b = w
            .spawn_at(z, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(a).unwrap().target = Some(b);
        w.actors.get_mut(a).unwrap().master = Some(b);
        assert_eq!(
            call_action_args(
                &mut w,
                a,
                "A_RearrangePointers",
                &["Tracer".into(), "Null".into(), "Target".into()],
            ),
            ActionResult::Continue
        );
        let act = w.actors.get(a).unwrap();
        assert_eq!(act.target, None); // Tracer was empty → null
        assert_eq!(act.master, None);
        assert_eq!(act.tracer, Some(b)); // from old Target
    }

    #[test]
    fn a_respawn_and_copy_friendliness() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let zombieman = classes.find_name("ZombieMan").unwrap();
        let spawn = states.find_label(zombieman, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(zombieman, DVec3::new(240.0, 240.0, 0.0), Angle::ZERO)
            .unwrap();
        let spawn_pt = w.actors.get(id).unwrap().spawn_point;
        w.actors.get_mut(id).unwrap().pos = DVec3::new(400.0, 400.0, 0.0);
        w.actors.get_mut(id).unwrap().health = 0;
        w.actors.get_mut(id).unwrap().flags |= crate::class::FLAG_CORPSE;
        assert_eq!(
            call_action_args(&mut w, id, "A_Respawn", &["1".into()]),
            ActionResult::Continue
        );
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.x() - spawn_pt.x()).abs() < 1e-6);
        assert!((a.pos.y() - spawn_pt.y()).abs() < 1e-6);
        assert!(a.health > 0);
        assert_eq!(a.flags & crate::class::FLAG_CORPSE, 0);
        assert_eq!(a.state, Some(spawn));

        let friend = w
            .spawn_at(zombieman, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(friend).unwrap().flags |= crate::class::FLAG_FRIENDLY;
        w.actors.get_mut(id).unwrap().target = Some(friend);
        w.actors.get_mut(id).unwrap().flags &= !crate::class::FLAG_FRIENDLY;
        assert_eq!(
            call_action_args(&mut w, id, "A_CopyFriendliness", &[]),
            ActionResult::Continue
        );
        assert_ne!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_FRIENDLY,
            0
        );
        assert_eq!(
            call_action_args(&mut w, id, "A_SetFriendly", &["0".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_FRIENDLY,
            0
        );
        assert_eq!(
            call_action_args(&mut w, id, "A_SetSpecies", &["Pack".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().species, "Pack");
        assert_eq!(
            call_action_args(&mut w, id, "A_SetTag", &["Named".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().nametag, "Named");
        assert_eq!(
            call_action_args(&mut w, id, "A_SetFriendlySeeBlocks", &["7".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().friendly_see_blocks, 7);
        assert_eq!(
            call_action_args(&mut w, id, "A_SetConversation", &["42".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().conversation, 42);
    }

    #[test]
    fn a_teleport_to_dest_and_select_weapon() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor Warper : Actor 9940
{
  States
  {
  Spawn:
    POSS A 1 A_Teleport("Land", "TeleportDest")
    POSS A -1
    Stop
  Land:
    POSS B -1
    Stop
  }
}
"#;
        let ir = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let warper_c = classes.find_name("Warper").unwrap();
        let dest_c = classes.find_name("TeleportDest").unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let spawn = states.find_label(warper_c, "Spawn").unwrap();
        let land = states.find_label(warper_c, "Land").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let _spot = w
            .spawn_at(
                dest_c,
                DVec3::new(400.0, 400.0, 0.0),
                Angle::from_degrees(90.0),
            )
            .unwrap();
        let id = w
            .spawn_at(warper_c, DVec3::new(80.0, 80.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.x() - 400.0).abs() < 1e-6);
        assert!((a.pos.y() - 400.0).abs() < 1e-6);
        assert!((a.angle.degrees() - 90.0).abs() < 1e-6);
        assert_eq!(a.state, Some(land));
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = crate::inventory::give_inventory(&mut w, mo, shotgun, 1);
        assert_eq!(
            call_action_args(&mut w, mo, "A_SelectWeapon", &["Shotgun".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            w.players[0]
                .as_ref()
                .unwrap()
                .pending_weapon
                .or(w.players[0].as_ref().unwrap().ready_weapon),
            Some(shotgun)
        );
    }

    #[test]
    fn a_check_block_jumps_when_offset_hits_wall() {
        let ir = parse_decorate(
            r#"
actor Probe : Actor 9950
{
  Radius 16
  Height 56
  +SOLID
  States
  {
  Spawn:
    POSS A 1 A_CheckBlock("Hit", 0, 0, 24, 0, 0, 0)
    POSS A -1
    Stop
  Hit:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let probe = classes.find_name("Probe").unwrap();
        let spawn = states.find_label(probe, "Spawn").unwrap();
        let hit = states.find_label(probe, "Hit").unwrap();
        // Start near west wall facing west; +64 forward should leave the room.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(
                probe,
                DVec3::new(20.0, 64.0, 0.0),
                Angle::from_degrees(180.0),
            )
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(hit));
    }

    #[test]
    fn a_give_to_target_and_damage_children() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor Boss : Actor 9960
{
  Health 100
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop Death: POSS B -1 Stop }
}
actor Minion : Actor 9961
{
  Health 40
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop Death: POSS B -1 Stop }
}
"#;
        let ir = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let boss_c = classes.find_name("Boss").unwrap();
        let minion_c = classes.find_name("Minion").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let boss = w
            .spawn_at(boss_c, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let minion = w
            .spawn_at(minion_c, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(boss).unwrap().target = Some(mo);
        w.actors.get_mut(minion).unwrap().master = Some(boss);
        let before = w.inventory.count(mo, clip);
        assert_eq!(
            call_action_args(&mut w, boss, "A_GiveToTarget", &["Clip".into(), "5".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.inventory.count(mo, clip), before + 5);
        assert_eq!(
            call_action_args(&mut w, boss, "A_DamageChildren", &["15".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(minion).unwrap().health, 25);
    }

    #[test]
    fn a_set_view_height_and_attack_z_offset() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetViewHeight", &["48".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetAttackZOffset", &["12".into()]),
            ActionResult::Continue
        );
        assert!((w.actors.get(mo).unwrap().view_height - 48.0).abs() < 1e-9);
        assert!((w.actors.get(mo).unwrap().attack_z_offset - 12.0).abs() < 1e-9);
    }

    #[test]
    fn a_set_fill_color_named() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetFillColor", &["Red".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().fill_color, Some([255, 0, 0]));
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetFillColor", &["None".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().fill_color, None);
    }

    #[test]
    fn a_set_blood_color_named_and_rgb() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetBloodColor", &["Blue".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().blood_color, Some([0, 0, 255]));
        assert_eq!(
            call_action_args(
                &mut w,
                mo,
                "A_SetBloodColor",
                &["10".into(), "20".into(), "30".into()],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().blood_color, Some([10, 20, 30]));
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetBloodColor", &["None".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().blood_color, None);
    }

    #[test]
    fn a_set_float_and_fast_speed() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetFloatSpeed", &["7".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetFastSpeed", &["18".into()]),
            ActionResult::Continue
        );
        assert!((w.actors.get(mo).unwrap().float_speed - 7.0).abs() < 1e-9);
        assert!((w.actors.get(mo).unwrap().fast_speed - 18.0).abs() < 1e-9);
    }

    #[test]
    fn a_copy_sprite_frame_and_raise_self() {
        let ir = parse_decorate(
            r#"
actor Body : Actor 9964
{
  Health 10
  +COUNTKILL
  States
  {
  Spawn: POSS A -1 Stop
  Raise: POSS C 1 Goto Spawn
  Death: POSS B -1 Stop
  }
}
actor Mimic : Actor 9963
{
  States
  {
  Spawn: TROO A -1 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let body_c = classes.find_name("Body").unwrap();
        let mimic_c = classes.find_name("Mimic").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let body = w
            .spawn_at(body_c, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let mimic = w
            .spawn_at(mimic_c, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(body).unwrap().sprite = "POSS".into();
        w.actors.get_mut(body).unwrap().frame = 3;
        w.actors.get_mut(mimic).unwrap().target = Some(body);
        assert_eq!(
            call_action_args(&mut w, mimic, "A_CopySpriteFrame", &[]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mimic).unwrap().sprite, "POSS");
        assert_eq!(w.actors.get(mimic).unwrap().frame, 3);
        w.actors.get_mut(body).unwrap().health = 0;
        w.actors.get_mut(body).unwrap().flags |= crate::class::FLAG_CORPSE;
        w.level.found_kills = 1;
        assert_eq!(
            call_action(&mut w, body, "A_RaiseSelf"),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(body).unwrap().health, 10);
        assert_eq!(w.level.found_kills, 0);
    }

    #[test]
    fn a_set_sprite_frame_and_transfer_inventory() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor Carrier : Actor 9965
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let carrier_c = classes.find_name("Carrier").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let src = w
            .spawn_at(carrier_c, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let dst = w
            .spawn_at(carrier_c, DVec3::new(96.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(dst).unwrap().tid = 42;
        let _ = crate::inventory::give_inventory(&mut w, src, clip, 7);
        assert_eq!(
            call_action_args(&mut w, src, "A_SetSprite", &["TROO".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, src, "A_SetFrame", &["C".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(src).unwrap().sprite, "TROO");
        assert_eq!(w.actors.get(src).unwrap().frame, 2);
        assert_eq!(
            call_action_args(
                &mut w,
                src,
                "A_TransferInventory",
                &["42".into(), "Clip".into()],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.inventory.count(src, clip), 0);
        assert_eq!(w.inventory.count(dst, clip), 7);
    }

    #[test]
    fn a_skip_unless_species_matches_runtime() {
        let ir = parse_decorate(
            r#"
actor Packer : Actor 9967
{
  Species "Pack"
  States
  {
  Spawn:
    POSS A 1 A_SkipUnlessSpecies(0, "Pack", 1)
    POSS A 1 A_SetDamage(9)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let packer = classes.find_name("Packer").unwrap();
        let sid = states.find_label(packer, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(packer, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(sid), false);
        assert_eq!(w.actors.get(id).unwrap().action_skip, 0);
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SkipUnlessSpecies",
                &["0".into(), "Pack".into(), "1".into()],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().action_skip, 0);
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_SkipUnlessSpecies",
                &["0".into(), "Other".into(), "1".into()],
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(id).unwrap().action_skip, 1);
    }

    #[test]
    fn a_set_invisible_toggles_dontdraw() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action(&mut w, mo, "A_SetInvisible"),
            ActionResult::Continue
        );
        assert_ne!(
            w.actors.get(mo).unwrap().flags & crate::class::FLAG_DONTDRAW,
            0
        );
        assert_eq!(
            call_action(&mut w, mo, "A_UnsetInvisible"),
            ActionResult::Continue
        );
        assert_eq!(
            w.actors.get(mo).unwrap().flags & crate::class::FLAG_DONTDRAW,
            0
        );
    }

    #[test]
    fn a_raise_and_remove_siblings() {
        let ir = parse_decorate(
            r#"
actor Parent : Actor 9968
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
actor Kid : Actor 9969
{
  Health 10
  +COUNTKILL
  States
  {
  Spawn: POSS A -1 Stop
  Raise: POSS A 1 Goto Spawn
  Death: POSS B -1 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let parent_c = classes.find_name("Parent").unwrap();
        let kid_c = classes.find_name("Kid").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let parent = w
            .spawn_at(parent_c, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let a = w
            .spawn_at(kid_c, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let b = w
            .spawn_at(kid_c, DVec3::new(96.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(a).unwrap().master = Some(parent);
        w.actors.get_mut(b).unwrap().master = Some(parent);
        w.actors.get_mut(b).unwrap().health = 0;
        w.actors.get_mut(b).unwrap().flags |= crate::class::FLAG_CORPSE;
        w.level.found_kills = 1;
        assert_eq!(
            call_action_args(&mut w, a, "A_RaiseSiblings", &[]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(b).unwrap().health, 10);
        assert_eq!(
            call_action_args(&mut w, a, "A_RemoveSiblings", &[]),
            ActionResult::Continue
        );
        assert!(w.actors.get(b).unwrap().destroyed);
        assert!(!w.actors.get(a).unwrap().destroyed);
    }

    #[test]
    fn a_raise_and_remove_children() {
        let ir = parse_decorate(
            r#"
actor Parent : Actor 9970
{
  States
  {
  Spawn:
    POSS A -1
    Stop
  }
}
actor Kid : Actor 9971
{
  Health 10
  +COUNTKILL
  States
  {
  Spawn: POSS A -1 Stop
  Raise: POSS A 1 Goto Spawn
  Death: POSS B -1 Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let parent_c = classes.find_name("Parent").unwrap();
        let kid_c = classes.find_name("Kid").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let parent = w
            .spawn_at(parent_c, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let kid = w
            .spawn_at(kid_c, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(kid).unwrap().master = Some(parent);
        w.actors.get_mut(kid).unwrap().health = 0;
        w.actors.get_mut(kid).unwrap().flags |= crate::class::FLAG_CORPSE;
        w.level.found_kills = 1;
        assert_eq!(
            call_action_args(&mut w, parent, "A_RaiseChildren", &[]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(kid).unwrap().health, 10);
        assert_eq!(w.level.found_kills, 0);
        assert_eq!(
            call_action_args(&mut w, parent, "A_RemoveChildren", &[]),
            ActionResult::Continue
        );
        assert!(w.actors.get(kid).unwrap().destroyed);
    }

    #[test]
    fn a_set_roll_sets_actor_roll() {
        let ir = parse_decorate(
            r#"
actor Roller : Actor 9990
{
  States
  {
  Spawn:
    POSS A 1 A_SetRoll(45)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let roller = classes.find_name("Roller").unwrap();
        let spawn = states.find_label(roller, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(roller, DVec3::new(100.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        let roll = w.actors.get(id).unwrap().roll.degrees();
        assert!((roll - 45.0).abs() < 1e-6, "roll={roll}");
    }

    #[test]
    fn a_set_angle_and_jump_if_health_lower() {
        let ir = parse_decorate(
            r#"
actor Jumper : Actor 9991
{
  Health 30
  States
  {
  Spawn:
    POSS A 1 A_SetAngle(90)
    POSS A 1 A_JumpIfHealthLower(50, "Hurt")
    POSS A -1
    Stop
  Hurt:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let jumper = classes.find_name("Jumper").unwrap();
        let spawn = states.find_label(jumper, "Spawn").unwrap();
        let hurt = states.find_label(jumper, "Hurt").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(jumper, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(spawn), true);
        assert!((w.actors.get(id).unwrap().angle.degrees() - 90.0).abs() < 1e-6);
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_eq!(w.actors.get(id).unwrap().state, Some(hurt));
    }

    #[test]
    fn a_set_gravity_and_mass() {
        let ir = parse_decorate(
            r#"
actor Heavy : Actor 9990
{
  Gravity 1.0
  Mass 100
  States
  {
  Spawn:
    POSS A 1 A_SetGravity(0.25)
    POSS A 1 A_SetMass(500)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let heavy = classes.find_name("Heavy").unwrap();
        let spawn = states.find_label(heavy, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(heavy, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!((w.actors.get(id).unwrap().gravity - 1.0).abs() < 1e-9);
        assert_eq!(w.actors.get(id).unwrap().mass, 100);
        w.set_state(id, Some(spawn), true);
        assert!((w.actors.get(id).unwrap().gravity - 0.25).abs() < 1e-9);
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert_eq!(w.actors.get(id).unwrap().mass, 500);
    }

    #[test]
    fn a_set_health_speed_tics() {
        let ir = parse_decorate(
            r#"
actor Tunable : Actor 9989
{
  Health 50
  Speed 8
  States
  {
  Spawn:
    POSS A 10 A_SetHealth(20)
    POSS A 1 A_SetSpeed(16)
    POSS A 5 A_SetTics(3)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let tunable = classes.find_name("Tunable").unwrap();
        let spawn = states.find_label(tunable, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(tunable, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(w.actors.get(id).unwrap().health, 50);
        assert!((w.actors.get(id).unwrap().speed - 8.0).abs() < 1e-9);
        w.set_state(id, Some(spawn), true);
        assert_eq!(w.actors.get(id).unwrap().health, 20);
        // Advance to A_SetSpeed frame.
        let next = states.get(spawn).unwrap().next.unwrap();
        w.set_state(id, Some(next), true);
        assert!((w.actors.get(id).unwrap().speed - 16.0).abs() < 1e-9);
        let next2 = states.get(next).unwrap().next.unwrap();
        w.set_state(id, Some(next2), true);
        assert_eq!(w.actors.get(id).unwrap().tics, 3);
    }

    #[test]
    fn a_change_flag_toggles_solid() {
        let ir = parse_decorate(
            r#"
actor Toggle : Actor 9988
{
  +SOLID
  States
  {
  Spawn:
    POSS A 1 A_ChangeFlag("SOLID", 0)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let toggle = classes.find_name("Toggle").unwrap();
        let sid = states.find_label(toggle, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(toggle, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_ne!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_SOLID,
            0
        );
        w.set_state(id, Some(sid), true);
        assert_eq!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_SOLID,
            0
        );
    }

    #[test]
    fn a_set_size_sets_absolute_collision() {
        let ir = parse_decorate(
            r#"
actor Resizer : Actor 9987
{
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_SetSize(10, 32)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let resizer = classes.find_name("Resizer").unwrap();
        let sid = states.find_label(resizer, "Spawn").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(resizer, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        w.set_state(id, Some(sid), true);
        assert!((w.actors.get(id).unwrap().radius - 10.0).abs() < 1e-9);
        assert!((w.actors.get(id).unwrap().height - 32.0).abs() < 1e-9);
    }

    #[test]
    fn a_set_scale_resizes_from_class_defaults() {
        let ir = parse_decorate(
            r#"
actor Grower : Actor 9986
{
  Radius 20
  Height 56
  States
  {
  Spawn:
    POSS A 1 A_SetScale(2.0, 0.5)
    POSS A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let grower = classes.find_name("Grower").unwrap();
        let sid = states.find_label(grower, "Spawn").unwrap();
        assert_eq!(
            states.get(sid).unwrap().action.as_deref(),
            Some("A_SetScale")
        );
        let args = &states.get(sid).unwrap().action_args;
        assert_eq!(args.len(), 2);
        assert!((args[0].parse::<f64>().unwrap() - 2.0).abs() < 1e-9);
        assert!((args[1].parse::<f64>().unwrap() - 0.5).abs() < 1e-9);
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(grower, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!((w.actors.get(id).unwrap().radius - 20.0).abs() < 1e-9);
        w.set_state(id, Some(sid), true);
        assert!((w.actors.get(id).unwrap().radius - 40.0).abs() < 1e-9);
        assert!((w.actors.get(id).unwrap().height - 28.0).abs() < 1e-9);
        assert!((w.actors.get(id).unwrap().scale_x - 2.0).abs() < 1e-9);
        assert!((w.actors.get(id).unwrap().scale_y - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_set_ripper_level_min_max() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetRipperLevel", &["3".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetRipMin", &["2".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetRipMax", &["5".into()]),
            ActionResult::Continue
        );
        let a = w.actors.get(mo).unwrap();
        assert_eq!(a.ripper_level, 3);
        assert_eq!(a.rip_level_min, 2);
        assert_eq!(a.rip_level_max, 5);
        let mut proj = a.clone();
        proj.ripper_level = 1;
        assert!(!can_rip_through(&proj, a));
        proj.ripper_level = 3;
        assert!(can_rip_through(&proj, a));
        proj.ripper_level = 6;
        assert!(!can_rip_through(&proj, a));
    }

    #[test]
    fn a_set_accuracy_and_stamina() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetAccuracy", &["50".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetStamina", &["25".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().accuracy, 50);
        assert_eq!(w.actors.get(mo).unwrap().stamina, 25);
    }

    #[test]
    fn a_set_team_and_camera_props() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetTeam", &["2".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().designated_team, 2);
        // cmp 0 = == ; team==2 → no skip; team==1 → action_skip=1
        assert_eq!(
            call_action_args(
                &mut w,
                mo,
                "A_SkipUnlessTeam",
                &["0".into(), "2".into(), "1".into()]
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().action_skip, 0);
        assert_eq!(
            call_action_args(
                &mut w,
                mo,
                "A_SkipUnlessTeam",
                &["0".into(), "1".into(), "1".into()]
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().action_skip, 1);
        w.actors.get_mut(mo).unwrap().action_skip = 0;
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetMeleeRange", &["80".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(
                &mut w,
                mo,
                "A_SkipUnlessMeleeRange",
                &["2".into(), "64".into(), "1".into()]
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().action_skip, 0);
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetViewHeight", &["41".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(
                &mut w,
                mo,
                "A_SkipUnlessViewHeight",
                &["0".into(), "41".into(), "1".into()]
            ),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().action_skip, 0);
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetCameraHeight", &["56".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetCameraFOV", &["75".into()]),
            ActionResult::Continue
        );
        let a = w.actors.get(mo).unwrap();
        assert_eq!(a.designated_team, 2);
        assert!((a.camera_height - 56.0).abs() < 1e-9);
        assert!((a.camera_fov - 75.0).abs() < 1e-9);
    }

    #[test]
    fn a_set_damage_factor_scales_incoming() {
        let ir = parse_decorate(
            r#"
actor Tough : Actor 9962
{
  Health 100
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let tough = classes.find_name("Tough").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(tough, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert_eq!(
            call_action_args(&mut w, id, "A_SetDamageFactor", &["0.5".into()]),
            ActionResult::Continue
        );
        let dealt = crate::damage::damage_mobj(&mut w, id, None, None, 20);
        assert_eq!(dealt, 10);
        assert_eq!(w.actors.get(id).unwrap().health, 90);
    }

    #[test]
    fn a_set_obituary_strings() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetObituary", &["%o got zapped".into()]),
            ActionResult::Continue
        );
        assert_eq!(
            call_action_args(&mut w, mo, "A_SetHitObituary", &["%o was punched".into()]),
            ActionResult::Continue
        );
        assert_eq!(w.actors.get(mo).unwrap().obituary, "%o got zapped");
        assert_eq!(w.actors.get(mo).unwrap().hit_obituary, "%o was punched");
    }

    #[test]
    fn a_pain_uses_howl_sound_when_dead() {
        let ir = parse_decorate(
            r#"
actor Howler : Actor 9981
{
  Health 10
  PainSound "misc/pain"
  HowlSound "misc/howl"
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let howler = classes.find_name("Howler").unwrap();
        let id = w
            .spawn_at(howler, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = call_action(&mut w, id, "A_Pain");
        assert!(w.drain_sound_events().iter().any(|e| e.name == "misc/pain"));
        w.actors.get_mut(id).unwrap().health = 0;
        let _ = call_action(&mut w, id, "A_Pain");
        assert!(w.drain_sound_events().iter().any(|e| e.name == "misc/howl"));
    }

    #[test]
    fn a_recoil_uservar_translucent_proximity_and_clear_pointers() {
        let ir = parse_decorate(
            r#"
actor Buddy : Actor 9979
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
actor Scout : Actor 9980
{
  Health 10
  States
  {
  Spawn:
    POSS A -1
    Stop
  Near:
    POSS B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let scout = classes.find_name("Scout").unwrap();
        let buddy = classes.find_name("Buddy").unwrap();
        let near = states.find_label(scout, "Near").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let id = w
            .spawn_at(scout, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _buddy = w
            .spawn_at(buddy, DVec3::new(80.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.master = Some(id);
            a.tracer = Some(id);
        }
        let _ = call_action_args(&mut w, id, "A_SetTranslucent", &["0.25".into()]);
        let a = w.actors.get(id).unwrap();
        assert!((a.alpha - 0.25).abs() < 1e-9);
        assert_eq!(a.render_style, crate::class::RenderStyle::Translucent);
        let _ = call_action_args(&mut w, id, "A_Recoil", &["8".into()]);
        let a = w.actors.get(id).unwrap();
        assert!((a.vel.x() + 8.0).abs() < 1e-6);
        assert!(a.vel.y().abs() < 1e-6);
        let _ = call_action_args(
            &mut w,
            id,
            "A_SetUserVar",
            &["user_count".into(), "7".into()],
        );
        assert_eq!(
            w.actors
                .get(id)
                .unwrap()
                .user_vars
                .get("user_count")
                .copied(),
            Some(7)
        );
        let _ = call_action_args(
            &mut w,
            id,
            "A_SetUserArray",
            &["user_hits".into(), "2".into(), "9".into()],
        );
        assert_eq!(
            w.actors
                .get(id)
                .unwrap()
                .user_arrays
                .get("user_hits")
                .and_then(|m| m.get(&2).copied()),
            Some(9)
        );
        assert_eq!(
            call_action_args(
                &mut w,
                id,
                "A_CheckProximity",
                &[
                    "Buddy".into(),
                    "64".into(),
                    "1".into(),
                    "0".into(),
                    "Near".into()
                ]
            ),
            ActionResult::Jump(near)
        );
        let _ = call_action(&mut w, id, "A_ClearMaster");
        let _ = call_action(&mut w, id, "A_ClearTracer");
        let a = w.actors.get(id).unwrap();
        assert!(a.master.is_none());
        assert!(a.tracer.is_none());
    }
}

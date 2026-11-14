//! Raw `"room"` game imports (wasm32) / zero-returning stubs (native tests).
//! Signatures mirror `room_script::GameFn`.

macro_rules! game_imports {
    ($( fn $name:ident($($a:ident: $t:ty),*) $(-> $r:ty)?; )*) => {
        #[cfg(target_arch = "wasm32")]
        #[link(wasm_import_module = "room")]
        unsafe extern "C" {
            $( pub fn $name($($a: $t),*) $(-> $r)?; )*
        }

        #[cfg(not(target_arch = "wasm32"))]
        mod native {
            $(
                #[allow(clippy::unused_unit)]
                pub unsafe fn $name($(_: $t),*) $(-> $r)? {
                    Default::default()
                }
            )*
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub use native::*;
    };
}

game_imports! {
    fn reg_class_property_str(prop: i32, ptr: i32, len: i32);
    fn game_skill() -> i32;
    fn level_mapname(buf: i32, cap: i32) -> i32;
    fn level_stat(stat: i32) -> i32;
    fn math_sin(rad: f64) -> f64;
    fn math_cos(rad: f64) -> f64;
    fn math_atan2(y: f64, x: f64) -> f64;
    fn math_sqrt(x: f64) -> f64;
    fn rng_range(lo: i32, hi: i32) -> i32;
    fn player_actor(player: i32) -> i64;
    fn player_get_i32(player: i32, prop: i32) -> i32;
    fn player_print(player: i32, ptr: i32, len: i32);
    fn actor_class(actor: i64) -> i32;
    fn class_name(class: i32, buf: i32, cap: i32) -> i32;
    fn class_is_descendant(class: i32, parent: i32) -> i32;
    fn actor_pos(actor: i64, out: i32);
    fn actor_vel(actor: i64, out: i32);
    fn actor_angle(actor: i64) -> f64;
    fn actor_get_i32(actor: i64, prop: i32) -> i32;
    fn actor_get_f64(actor: i64, prop: i32) -> f64;
    fn actor_get_ref(actor: i64, ptr: i32) -> i64;
    fn actor_iter_class(class: i32, sub: i32, out: i32, cap: i32) -> i32;
    fn actor_iter_radius(x: f64, y: f64, r: f64, out: i32, cap: i32) -> i32;
    fn actor_check_sight(from: i64, to: i64) -> i32;
    fn actor_distance(a: i64, b: i64) -> f64;
    fn actor_count_inv(actor: i64, class: i32) -> i32;
    fn point_floor_z(x: f64, y: f64) -> f64;
    fn point_ceiling_z(x: f64, y: f64) -> f64;
    fn point_reachable(x: f64, y: f64) -> i32;
    fn actor_spawn(class: i32, x: f64, y: f64, z: f64, angle: f64, flags: i32) -> i64;
    fn actor_destroy(actor: i64);
    fn actor_set_i32(actor: i64, prop: i32, v: i32);
    fn actor_set_f64(actor: i64, prop: i32, v: f64);
    fn actor_set_ref(actor: i64, ptr: i32, v: i64);
    fn actor_set_pos(actor: i64, x: f64, y: f64, z: f64) -> i32;
    fn actor_set_vel(actor: i64, x: f64, y: f64, z: f64);
    fn actor_set_angle(actor: i64, deg: f64);
    fn actor_damage(target: i64, inflictor: i64, source: i64, amount: i32) -> i32;
    fn actor_give_inventory(actor: i64, class: i32, amount: i32) -> i32;
    fn actor_take_inventory(actor: i64, class: i32, amount: i32) -> i32;
    fn actor_set_state(actor: i64, ptr: i32, len: i32) -> i32;
    fn actor_start_sound(actor: i64, ptr: i32, len: i32, volume: f64);
    fn level_exit(secret: i32);
    fn level_set_music(ptr: i32, len: i32);
    fn game_get_rule(rule: i32) -> i32;
    fn game_set_rule(rule: i32, value: i32);
    fn game_end_match(winner: i32);
    fn game_end_match_ex(winner: i32, standings_ptr: i32, n: i32);
    fn level_things(ednum: i32, out: i32, cap: i32) -> i32;
    fn hud_widget_set(seat1: i32, id: i32, params_ptr: i32, params_len: i32) -> i32;
    fn hud_widget_patch(seat1: i32, id: i32, field: i32, value: i32) -> i32;
    fn hud_widget_remove(seat1: i32, id: i32) -> i32;
    fn hud_clear(seat1: i32) -> i32;
    fn player_capture_input(player: i32, mask: i32);
    fn player_cmd(player: i32, out_ptr: i32) -> i32;
    fn player_sound(player: i32, ptr: i32, len: i32, volume: f64);
    fn hud_widget_animate(seat1: i32, id: i32, field: i32, target: i32, tics: i32, easing: i32) -> i32;
    fn actor_flag(actor: i64, flag: i32) -> i32;
    fn actor_set_flag(actor: i64, flag: i32, on: i32);
    fn player_set_i32(player: i32, prop: i32, v: i32);
    fn actor_quake(actor: i64, intensity: f64, duration: i32, damrad: f64, tremrad: f64);
    fn actor_radius_attack(actor: i64, source: i64, damage: i32, distance: f64);
    fn actor_spawn_missile(src: i64, dest: i64, class: i32) -> i64;
    fn zone_set(id: i32, cx: i32, cy: i32, r: i32, next_cx: i32, next_cy: i32, next_r: i32, rgba: i32, flags: i32) -> i32;
    fn zone_clear(id: i32) -> i32;
    fn player_set_camera(player: i32, actor: i64);
    fn player_set_team(player: i32, team: i32);
    fn player_team(player: i32) -> i32;
    fn player_trace(player: i32, max_dist: f64, out_ptr: i32) -> i32;
    fn player_name(player: i32, buf: i32, cap: i32) -> i32;
    fn actor_hit_info(actor: i64, out_ptr: i32) -> i32;
    fn script_arg_count() -> i32;
    fn script_arg_i32(i: i32) -> i32;
    fn script_arg_str(i: i32, buf: i32, cap: i32) -> i32;
    fn script_return_str(ptr: i32, len: i32);
    fn player_gender(player: i32) -> i32;
    fn player_cvar_i32(player: i32, ptr: i32, len: i32) -> i32;
    fn actor_set_sound_class(actor: i64, ptr: i32, len: i32);
}

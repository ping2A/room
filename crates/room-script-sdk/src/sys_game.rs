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
    fn level_things(ednum: i32, out: i32, cap: i32) -> i32;
}

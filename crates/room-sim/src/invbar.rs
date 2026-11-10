//! Inventory bar: the selected item, cycling and use (UZDoom `invnext`,
//! `invprev`, `invuse`, `PlayerPawn.InvSel` / `InvFirst`).

use crate::player::Player;
use crate::world::World;
use crate::actor::ActorId;
use room_core::ClassId;

/// Tics the bar stays open after a cycle (UZDoom `5 * TICRATE`).
pub const INVENTORY_TICS: i32 = 5 * crate::player::TICRATE;

/// `+INVENTORY.INVBAR` items `mo` carries (amount above 0), in pickup order.
#[must_use]
pub fn invbar_items(world: &World, mo: ActorId) -> Vec<ClassId> {
    let mut out: Vec<ClassId> = world
        .inventory
        .iter(mo)
        .filter(|it| it.amount > 0)
        .filter(|it| {
            world
                .classes
                .get(it.class)
                .is_some_and(|c| c.ext.has_qualified_flag("INVENTORY.INVBAR"))
        })
        .map(|it| it.class)
        .collect();
    // The bag is newest-first; the bar lists the first pickup first.
    out.reverse();
    out
}

/// Keep `InvSel` on an item still carried; with none, select the first
/// (UZDoom `PlayerPawn.AddInventory` / `InvSel` repair).
pub fn validate_selection(world: &World, player: &mut Player, mo: ActorId) {
    let items = invbar_items(world, mo);
    if player.inv_sel.is_some_and(|c| items.contains(&c)) {
        return;
    }
    player.inv_sel = items.first().copied();
}

/// `invnext` (`dir > 0`) / `invprev`: move the selection, wrapping, and open
/// the bar.
pub fn cycle(world: &World, player: &mut Player, mo: ActorId, dir: i32) {
    let items = invbar_items(world, mo);
    if items.is_empty() {
        player.inv_sel = None;
        return;
    }
    let n = items.len();
    let cur = player
        .inv_sel
        .and_then(|c| items.iter().position(|&x| x == c))
        .unwrap_or(0);
    let next = if dir > 0 { (cur + 1) % n } else { (cur + n - 1) % n };
    player.inv_sel = Some(items[next]);
    player.inventory_tics = INVENTORY_TICS;
}

/// `invuse`: with the bar closed use the selected item; with it open, only
/// close it (UZDoom `CCMD(invuse)`). Returns true when something was used.
pub fn use_selected(world: &mut World, player: &mut Player, mo: ActorId) -> bool {
    if player.inventory_tics > 0 {
        player.inventory_tics = 0;
        return false;
    }
    let Some(class) = player.inv_sel else {
        return false;
    };
    let used = crate::pickup::use_inventory_item(world, mo, class);
    validate_selection(world, player, mo);
    used
}

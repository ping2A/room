//! Chocolate `badguy` / `badguypic` (actors.c) — picnum table + `actortype[]`.

/// Chocolate `badguy(s)` / `badguypic(pn)`.
#[must_use]
pub fn badguy_pic(picnum: i16, actortype: &[u8]) -> bool {
    let hit = matches!(
        picnum,
        1550 | // SHARK
        1960 | // RECON
        1880 | // DRONE
        1741 | // LIZTROOPONTOILET
        1742 | // LIZTROOPJUSTSIT
        1682 | // LIZTROOPSTAYPUT
        1715 | // LIZTROOPSHOOT
        1725 | // LIZTROOPJETPACK
        1744 | // LIZTROOPDUCKING
        1681 | // LIZTROOPRUNNING
        1680 | // LIZTROOP
        1820 | // OCTABRAIN (OCTABRAINSTAYPUT is not in Chocolate's list)
        1920 | 1921 | // COMMANDER*
        2000 | // PIGCOP
        675 | // EGG
        2001 | // PIGCOPSTAYPUT
        2045 | // PIGCOPDIVE
        2120 | // LIZMAN
        2150 | // LIZMANSPITTING
        2160 | // LIZMANFEEDING
        2165 | // LIZMANJUMP
        2420 | // ORGANTIC
        2630 | // BOSS1 (BOSS1STAYPUT is not in Chocolate's list)
        2710 | // BOSS2
        2760 | // BOSS3
        4740 | // BOSS4
        2370
            ..=2377 | // GREENSLIME*
        1267 | // RAT
        2360 // ROTATEGUN
    );
    if hit {
        return true;
    }
    actortype
        .get(picnum.max(0) as usize)
        .copied()
        .is_some_and(|t| t != 0)
}

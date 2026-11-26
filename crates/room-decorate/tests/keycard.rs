//! DECORATE parsing of GL keycard actors.

// Test helpers: a failed setup step should panic the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
#[test]
fn gl_blue_card() {
    let src = r#"
Actor GLBlueCard : KeySpawnerBase 5
{
	Inventory.PickupMessage "$BDGOTBLUECARD"
	States
	{
	Spawn:
		BKEY A 0
		BKEY A 0 A_SpawnItemEx("BlueCard", 0, 0, 0, 0, 0, 0, 0, SXF_NOCHECKPOSITION|SXF_CLEARCALLERTID|SXF_TRANSFERSCALE, 0, TID)
		BKEY A 0 A_SetScale(1.0, 1.0)
		BKEY A 2
		BKEY A 0 A_Jump(128,2)
		BKEY A 0 A_SetScale(-scalex,scaley)
	NoRandom:
		BKEY A 10
		BKEY B 10 Bright
		Loop
	Pickup:
		TNT1 A 0 A_GiveInventory("BlueCard",1)
		stop
	}
}
"#;
    parse_decorate(src).expect("GLBlueCard");
}

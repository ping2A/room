use room_decorate::parse_decorate;

#[test]
fn ceil_blood_spot_thrustthingz() {
    let src = r#"
actor CeilBloodSpot
{
  +MISSILE
  states {
  Live:
    BPDL A 5 ThrustThingZ(0, 20, 0, 1)
    Loop
  }
}
"#;
    parse_decorate(src).expect("parse ThrustThingZ");
}

#[test]
fn axe_slash_hit_token() {
    let src = r#"
actor bd_Token : Inventory {}
Actor Axe/Hit : bd_Token {}
"#;
    parse_decorate(src).expect("slash name");
}

#[test]
fn action_args_equality() {
    let src = r#"
actor Foo : Actor {
  States {
  Spawn:
    TNT1 A 0 A_JumpIf(waterlevel == 0, "Splash")
    Stop
  }
}
"#;
    parse_decorate(src).expect("== in action args");
}

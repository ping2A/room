// MH2 AssemblyScript mirror of the Mod HUD builder (`docs/27` §9.2).
import { RoomMod, register, Hud, HudAnchor, HudFont, Events } from "./room";

class HudDemo extends RoomMod {
  constructor() {
    super("HudAs");
    this.events = Events.LOADED;
  }

  onLoad(_maptime: i32): void {
    Hud.text(3)
      .anchor(HudAnchor.TOP_CENTER)
      .at(0, 24)
      .font(HudFont.BIG)
      .color("Gold")
      .text("ROUND 5")
      .seat(0)
      .show();
  }
}

register(new HudDemo());

#!/usr/bin/env python3
"""Make the Android launcher icon and the iOS app icon from the Room logo
(`docs/media/logo.png`, transparent isometric gaming-room mark).

    python3 scripts/make-android-icon.py

Writes into mobile/android/app/src/main/res/:
  mipmap-<density>/ic_launcher.png, ic_launcher_round.png  (Android 7 and older)
  mipmap-<density>/ic_launcher_foreground.png              (adaptive icon layer)
  mipmap-anydpi-v26/ic_launcher*.xml, values/ic_launcher_background.xml
and mobile/android/icon-512.png (store listing size), plus the iOS asset
catalog mobile/ios/Assets.xcassets (one 1024 px AppIcon, no transparency:
iOS rounds the corners itself), compiled by scripts/build-ios.sh.

Needs Pillow (pip install pillow).
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
LOGO = ROOT / "docs/media/logo.png"
RES = ROOT / "mobile/android/app/src/main/res"
BACKGROUND = (42, 6, 6)  # dark blood red behind the diorama
EDGE = (92, 14, 12)  # lighter rim on the legacy icons
DENSITIES = {"mdpi": 1.0, "hdpi": 1.5, "xhdpi": 2.0, "xxhdpi": 3.0, "xxxhdpi": 4.0}


def fit_logo(logo: Image.Image, canvas_px: int, fill: float = 0.86) -> Image.Image:
    """`logo` scaled to about `fill` of the canvas, centred, aspect preserved."""
    target = int(canvas_px * fill)
    scale = min(target / logo.width, target / logo.height)
    if canvas_px <= 144:
        scale = max(0.25, round(scale * 4) / 4)
    nw = max(1, int(logo.width * scale))
    nh = max(1, int(logo.height * scale))
    resample = Image.Resampling.NEAREST if scale >= 1 else Image.Resampling.LANCZOS
    big = logo.resize((nw, nh), resample)
    layer = Image.new("RGBA", (canvas_px, canvas_px), (0, 0, 0, 0))
    layer.paste(big, ((canvas_px - nw) // 2, (canvas_px - nh) // 2), big)
    return layer


def legacy_icon(size: int, logo: Image.Image, round_shape: bool) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    inset = max(1, size // 24)
    box = (inset, inset, size - 1 - inset, size - 1 - inset)
    if round_shape:
        draw.ellipse(box, fill=BACKGROUND, outline=EDGE, width=max(1, size // 48))
    else:
        draw.rounded_rectangle(
            box, radius=size // 5, fill=BACKGROUND, outline=EDGE, width=max(1, size // 48)
        )
    img.alpha_composite(fit_logo(logo, size, 0.88))
    return img


def main() -> None:
    if not LOGO.is_file():
        raise SystemExit(f"make-android-icon: missing {LOGO}")
    logo = Image.open(LOGO).convert("RGBA")

    for density, factor in DENSITIES.items():
        out = RES / f"mipmap-{density}"
        out.mkdir(parents=True, exist_ok=True)
        legacy = round(48 * factor)
        legacy_icon(legacy, logo, False).save(out / "ic_launcher.png", optimize=True)
        legacy_icon(legacy, logo, True).save(out / "ic_launcher_round.png", optimize=True)
        # Adaptive layer: 108 dp; safe zone is the middle ~66 dp.
        fit_logo(logo, round(108 * factor), 0.78).save(
            out / "ic_launcher_foreground.png", optimize=True
        )

    anydpi = RES / "mipmap-anydpi-v26"
    anydpi.mkdir(parents=True, exist_ok=True)
    adaptive = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        "<!-- Made by scripts/make-android-icon.py from docs/media/logo.png. -->\n"
        '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
        '    <background android:drawable="@color/ic_launcher_background" />\n'
        '    <foreground android:drawable="@mipmap/ic_launcher_foreground" />\n'
        '    <monochrome android:drawable="@mipmap/ic_launcher_foreground" />\n'
        "</adaptive-icon>\n"
    )
    for name in ("ic_launcher.xml", "ic_launcher_round.xml"):
        (anydpi / name).write_text(adaptive)
    values = RES / "values"
    values.mkdir(parents=True, exist_ok=True)
    (values / "ic_launcher_background.xml").write_text(
        '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
        '    <color name="ic_launcher_background">#%02X%02X%02X</color>\n</resources>\n'
        % BACKGROUND
    )
    legacy_icon(512, logo, False).save(ROOT / "mobile/android/icon-512.png", optimize=True)

    xcassets = ROOT / "mobile/ios/Assets.xcassets"
    appicon = xcassets / "AppIcon.appiconset"
    appicon.mkdir(parents=True, exist_ok=True)
    ios = Image.new("RGB", (1024, 1024), BACKGROUND)
    layer = fit_logo(logo, 1024, 0.92)
    ios.paste(layer, (0, 0), layer)
    ios.save(appicon / "icon-1024.png", optimize=True)
    (xcassets / "Contents.json").write_text(
        '{\n  "info" : { "author" : "room", "version" : 1 }\n}\n'
    )
    (appicon / "Contents.json").write_text(
        '{\n  "images" : [\n    {\n      "filename" : "icon-1024.png",\n'
        '      "idiom" : "universal",\n      "platform" : "ios",\n'
        '      "size" : "1024x1024"\n    }\n  ],\n'
        '  "info" : { "author" : "room", "version" : 1 }\n}\n'
    )
    print(f"make-android-icon: icons written under {RES}")


if __name__ == "__main__":
    main()

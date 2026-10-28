#!/usr/bin/env python3
"""Generate PBC-CAM application icons.

Requires Pillow:  python3 -m pip install pillow
On macOS the script also produces an .icns via `iconutil`.

Usage:  python3 scripts/gen_icons.py
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SIZE = 1024
SS = 2  # supersampling factor
CANVAS = SIZE * SS


def s(value: float) -> int:
    return round(value * SS)


def make_master() -> Image.Image:
    # Vertical background gradient, clipped to a rounded square.
    background = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    gradient = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    gd = ImageDraw.Draw(gradient)
    top = (25, 32, 41)
    bottom = (8, 11, 15)
    for y in range(CANVAS):
        t = y / (CANVAS - 1)
        color = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        gd.line([(0, y), (CANVAS, y)], fill=(*color, 255))

    mask = Image.new("L", (CANVAS, CANVAS), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [s(32), s(32), s(992), s(992)], radius=s(220), fill=255
    )
    background.paste(gradient, (0, 0), mask)

    draw = ImageDraw.Draw(background)
    draw.rounded_rectangle(
        [s(32), s(32), s(992), s(992)],
        radius=s(220),
        outline=(124, 87, 52, 255),
        width=s(9),
    )
    draw.rounded_rectangle(
        [s(50), s(50), s(974), s(974)],
        radius=s(204),
        outline=(227, 164, 94, 26),
        width=s(3),
    )

    trace_points = [
        [(120, 360), (330, 360)],
        [(690, 420), (900, 420)],
    ]
    p_points = [(330, 760), (330, 220), (570, 220), (690, 340), (690, 420), (570, 540), (330, 540)]

    # Soft copper glow under the traces.
    glow = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(glow)
    for points in trace_points:
        gdraw.line([(s(x), s(y)) for x, y in points], fill=(227, 164, 94, 150), width=s(30), joint="curve")
    gdraw.line([(s(x), s(y)) for x, y in p_points], fill=(243, 191, 132, 170), width=s(76), joint="curve")
    glow = glow.filter(ImageFilter.GaussianBlur(s(18)))
    background.alpha_composite(glow)

    # Copper traces.
    for points in trace_points:
        draw.line(
            [(s(x), s(y)) for x, y in points],
            fill=(227, 164, 94, 255),
            width=s(26),
            joint="curve",
        )
        for x, y in (points[0], points[-1]):
            draw.ellipse(
                [s(x - 13), s(y - 13), s(x + 13), s(y + 13)],
                fill=(227, 164, 94, 255),
            )

    # Stylised P routed as a copper trace.
    draw.line(
        [(s(x), s(y)) for x, y in p_points],
        fill=(243, 191, 132, 255),
        width=s(68),
        joint="curve",
    )
    for x, y in (p_points[0], p_points[-1]):
        draw.ellipse(
            [s(x - 34), s(y - 34), s(x + 34), s(y + 34)],
            fill=(243, 191, 132, 255),
        )

    # Through-hole pads.
    for x, y, outer, inner in [
        (120, 360, 48, 22),
        (900, 420, 48, 22),
        (330, 760, 40, 18),
        (570, 540, 34, 15),
    ]:
        draw.ellipse(
            [s(x - outer), s(y - outer), s(x + outer), s(y + outer)],
            fill=(227, 164, 94, 255),
        )
        draw.ellipse(
            [s(x - inner), s(y - inner), s(x + inner), s(y + inner)],
            fill=(10, 14, 18, 255),
        )

    return background.resize((SIZE, SIZE), Image.LANCZOS)


def make_icns(master: Image.Image) -> bool:
    if sys.platform != "darwin" or shutil.which("iconutil") is None:
        return False

    iconset = ASSETS / "icon.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir()

    variants = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]
    for size, name in variants:
        master.resize((size, size), Image.LANCZOS).save(iconset / name)

    try:
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset), "-o", str(ASSETS / "icon.icns")],
            check=True,
            capture_output=True,
        )
    except subprocess.CalledProcessError as error:
        print(f"iconutil failed: {error}", file=sys.stderr)
        return False
    finally:
        shutil.rmtree(iconset, ignore_errors=True)
    return True


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    master = make_master()
    master.save(ASSETS / "icon.png")
    master.save(
        ASSETS / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    if make_icns(master):
        print("wrote assets/icon.png, assets/icon.ico, assets/icon.icns")
    else:
        print("wrote assets/icon.png, assets/icon.ico")


if __name__ == "__main__":
    main()

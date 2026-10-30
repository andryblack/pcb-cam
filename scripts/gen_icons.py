#!/usr/bin/env python3
"""Generate PCB-CAM application icons from the vector source.

Source of truth: ``assets/icon.svg`` (1254x1254). The script rasterises it
with headless Chrome, then writes ``assets/icon.png`` (1024), ``assets/icon.ico``
and — on macOS — ``assets/icon.icns`` via ``iconutil``.

Requires Pillow (``python3 -m pip install pillow``) and Google Chrome.

Usage:  python3 scripts/gen_icons.py
"""

from __future__ import annotations

import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SVG = ASSETS / "icon.svg"
SIZE = 1024
CHROME_CANDIDATES = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "google-chrome",
    "chromium",
    "chromium-browser",
]


def find_chrome() -> str | None:
    for candidate in CHROME_CANDIDATES:
        if Path(candidate).is_file():
            return candidate
        found = shutil.which(candidate)
        if found:
            return found
    return None


def render_svg(chrome: str) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "icon.png"
        subprocess.run(
            [
                chrome,
                "--headless=new",
                "--disable-gpu",
                "--no-sandbox",
                "--hide-scrollbars",
                "--default-background-color=00000000",
                "--force-device-scale-factor=1",
                "--window-size=1254,1254",
                f"--user-data-dir={Path(tmp) / 'profile'}",
                f"--screenshot={out}",
                SVG.as_uri(),
            ],
            check=True,
            capture_output=True,
        )
        return Image.open(out).convert("RGBA")


def trim(master: Image.Image) -> Image.Image:
    """Crop to the drawn artwork and pad back to a centred square."""
    art = master.crop(master.split()[3].getbbox())
    side = max(art.size)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(art, ((side - art.width) // 2, (side - art.height) // 2), art)
    return square


def macos_body(master: Image.Image, canvas: int = 1024) -> Image.Image:
    """Fit the artwork to Apple's icon grid: an 824/1024 body centred on the canvas.

    The squircle silhouette lives in the SVG itself, so no extra mask is applied
    here — masking would clip the artwork's own frame.
    """
    body = round(canvas * 824 / 1024)
    art = master.crop(master.split()[3].getbbox()).resize((body, body), Image.LANCZOS)
    out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    out.paste(art, ((canvas - body) // 2, (canvas - body) // 2), art)
    return out


def make_icns(master: Image.Image) -> bool:
    iconset = ASSETS / "icon.iconset"
    shutil.rmtree(iconset, ignore_errors=True)
    iconset.mkdir(parents=True)
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
    if not SVG.is_file():
        sys.exit(f"missing {SVG}")
    chrome = find_chrome()
    if chrome is None:
        sys.exit("Google Chrome not found; install it or pass a renderer")

    master = trim(render_svg(chrome)).resize((SIZE, SIZE), Image.LANCZOS)
    master.save(ASSETS / "icon.png")
    master.save(
        ASSETS / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    wrote = "assets/icon.svg -> assets/icon.png, assets/icon.ico"
    if platform.system() == "Darwin" and make_icns(macos_body(master)):
        wrote += ", assets/icon.icns"
    print(wrote)


if __name__ == "__main__":
    main()

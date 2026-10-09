#!/usr/bin/env python3
"""Builds the icon and the logo from the two source images in assets/brand.

    assets/brand/source-icon.png   the mark on a black square (the app icon)
    assets/brand/source-logo.png   the mark with its outline on a transparent background

Writes assets/app-icon.{png,ico,icns} and assets/logo.png (what the app shows, 256 px high).
Needs Pillow. Run it again when a source image changes, and commit the results.
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"


def rounded(image: Image.Image, size: int, radius_share: float = 0.22) -> Image.Image:
    """`image` scaled to a square of `size` pixels with rounded corners."""
    scaled = image.convert("RGBA").resize((size, size), Image.LANCZOS)
    # Corners are drawn four times too large and scaled down, so their edge is smooth.
    big = size * 4
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, big - 1, big - 1), radius=int(big * radius_share), fill=255
    )
    scaled.putalpha(mask.resize((size, size), Image.LANCZOS))
    return scaled


def trimmed(image: Image.Image) -> Image.Image:
    box = image.getchannel("A").getbbox()
    return image.crop(box) if box else image


def make_icons() -> None:
    source = Image.open(ASSETS / "brand" / "source-icon.png")
    icon = rounded(source, 1024)
    icon.save(ASSETS / "app-icon.png", optimize=True)
    icon.save(
        ASSETS / "app-icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    icon.save(ASSETS / "app-icon.icns")


def make_logo() -> None:
    logo = trimmed(Image.open(ASSETS / "brand" / "source-logo.png").convert("RGBA"))
    height = 256
    width = round(logo.width * height / logo.height)
    logo.resize((width, height), Image.LANCZOS).save(ASSETS / "logo.png", optimize=True)


def main() -> None:
    make_icons()
    make_logo()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Convert the source SVG into platform icons. Requires cairosvg and Pillow."""
from pathlib import Path
import cairosvg
from PIL import Image

assets = Path(__file__).resolve().parents[1] / "assets"
cairosvg.svg2png(url=str(assets / "magpie.svg"), write_to=str(assets / "magpie.png"))
cairosvg.svg2png(url=str(assets / "magpie.svg"), write_to=str(assets / "magpie-256.png"), output_width=256, output_height=256)
with Image.open(assets / "magpie.png") as icon:
    icon.save(assets / "magpie.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    icon.save(assets / "magpie.icns")

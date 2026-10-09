#!/usr/bin/env python3
"""Export the selected Collector artwork as platform icons. Requires Pillow."""
from pathlib import Path
from PIL import Image

assets = Path(__file__).resolve().parents[1] / "assets"
with Image.open(assets / "magpie-source.png") as source:
    if source.width != source.height:
        raise ValueError("The app icon source must be square")
    icon = source.convert("RGBA").resize((1024, 1024), Image.Resampling.LANCZOS)
    icon.save(assets / "magpie.png")
    icon.resize((256, 256), Image.Resampling.LANCZOS).save(assets / "magpie-256.png")
    icon.save(assets / "magpie.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    icon.save(assets / "magpie.icns")

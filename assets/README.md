# App icon

Magpie uses the **Collector** design: a charcoal-and-ivory magpie carrying a sage image card on a cream tile. The selected artwork was generated with the built-in image-generation tool. Its original prompt is in `magpie-source-prompt.txt`.

`magpie-source.png` is the original artwork with transparent corners. Small flecks remain around its outer tile edge; the image is not a vector master. Keep this source intact when exporting platform formats.

Regenerate the checked-in icons with Pillow installed:

```sh
python3 scripts/generate_icons.py
```

The script exports `magpie.png` (1024px), `magpie-256.png`, `magpie.icns` for macOS, and a multi-resolution `magpie.ico` embedded in the Windows executable. Linux packages install both PNG sizes. The README uses the 256px PNG.

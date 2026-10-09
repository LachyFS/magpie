# Screenshots

The README screenshots are captures of the real Linux GPUI app, using a temporary demo library. They are not interface mockups and contain no user data. Demo photo positions are prepared before opening the app, and the zoom menu, collapsible board sidebar, and new-board state are reached through native input.

After building, regenerate them with:

```sh
python3 scripts/capture_screenshots.py
```

The script uses the same Xvfb, xdotool, Python Xlib, and Pillow dependencies as the smoke test. It downloads three demo photographs under the [Unsplash License](https://unsplash.com/license):

- [Forest photograph](https://images.unsplash.com/photo-1441974231531-c6227db76b6e)
- [Alpine lake photograph](https://images.unsplash.com/photo-1493246507139-91e8fad9978e)
- [Night mountain photograph](https://images.unsplash.com/photo-1519681393784-d120267933ba)

The photos are demo content only and are not bundled with the app. The Magpie icon is an original SVG in `assets/magpie.svg`; its PNG, ICO, and ICNS exports are generated with `scripts/generate_icons.py` using CairoSVG and Pillow.

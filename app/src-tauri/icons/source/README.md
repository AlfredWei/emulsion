# App icon source

`icon-1024.png` is the master icon (1024×1024, full-bleed square, no pre-baked corner rounding — macOS/Windows apply their own icon masks). `generate_icon.py` produces it programmatically: a warm amber camera-aperture iris on the app's dark background, using the same palette as `app/src/lib/styles/tokens.css`.

All the actual platform icon files under `app/src-tauri/icons/` (`.icns`, `.ico`, the `Square*Logo.png` Windows tile set, etc.) are generated from `icon-1024.png` via Tauri's own icon tool — not hand-edited.

## Regenerate or tweak the design

```bash
cd app/src-tauri/icons/source
python3 -m venv venv && ./venv/bin/pip install Pillow numpy
./venv/bin/python generate_icon.py          # writes icon-1024.png here

cd ../../../..                               # back to app/
npx tauri icon app/src-tauri/icons/source/icon-1024.png
```

The second command regenerates every platform icon file into `app/src-tauri/icons/` from the new `icon-1024.png`.

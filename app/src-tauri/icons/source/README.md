# App icon source

`icon-1024.png` is the master icon (1024×1024 RGBA, **transparent background** — the droplet floats on whatever the OS dock/taskbar/tile shows behind it; no baked-in backdrop or corner rounding). `generate_icon.py` produces it programmatically: a warm amber liquid droplet with an iridescent edge sheen, using the same palette as `app/src/lib/styles/tokens.css`.

All the actual platform icon files under `app/src-tauri/icons/` (`.icns`, `.ico`, the `Square*Logo.png` Windows tile set, etc.) are generated from `icon-1024.png` via Tauri's own icon tool — not hand-edited.

## Regenerate or tweak the design

```bash
cd app/src-tauri/icons/source
python3 -m venv venv && ./venv/bin/pip install Pillow numpy
./venv/bin/python generate_icon.py          # writes icon-1024.png here

cd ../../..                                  # back to app/
npx tauri icon src-tauri/icons/source/icon-1024.png
```

The second command regenerates every platform icon file into `app/src-tauri/icons/` from the new `icon-1024.png`. Recent Tauri versions also emit `android/`, `ios/` and `64x64.png`, which this desktop-only app doesn't use — delete them rather than committing them.

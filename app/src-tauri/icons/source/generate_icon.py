#!/usr/bin/env python3
"""Emulsion app icon: bold aperture-blade iris, painted with a warm glossy
amber gradient (matching app/src/lib/styles/tokens.css), on the app's dark
background. Full-bleed square, no pre-baked corner rounding -- OS icon
masks (macOS squircle, Windows tile shapes) apply their own shape.

Regenerate:
    python3 -m venv venv && ./venv/bin/pip install Pillow numpy
    ./venv/bin/python generate_icon.py
    cd ../../../.. && npx tauri icon app/src-tauri/icons/source/icon-1024.png
"""

import math
import os
import numpy as np
from PIL import Image, ImageDraw

SS = 2
S = 1024 * SS
C = S / 2


def lerp(a, b, t):
    return a + (b - a) * t


def hex_to_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


BG_CENTER = hex_to_rgb("#221f1a")
BG_EDGE = hex_to_rgb("#0f0d0b")
IRIS_HIGHLIGHT = hex_to_rgb("#fbd08c")
IRIS_MID = hex_to_rgb("#e0932f")
IRIS_RIM = hex_to_rgb("#7c4a1c")
PUPIL = hex_to_rgb("#120f0c")
BLADE_STROKE = (26, 16, 8)

# ---- background ----
yy, xx = np.mgrid[0:S, 0:S].astype(np.float64)
d_bg = np.clip(np.sqrt((xx - C) ** 2 + (yy - C) ** 2) / (S * 0.75), 0, 1)
bg = np.stack([lerp(BG_CENTER[c], BG_EDGE[c], d_bg) for c in range(3)], axis=-1)
img = Image.fromarray(bg.astype(np.uint8), "RGB").convert("RGBA")

# ---- glossy gradient field (used to paint the blades) ----
iris_r = S * 0.31
iris_cx, iris_cy = C, C
hl_cx, hl_cy = C - iris_r * 0.30, C - iris_r * 0.36
d_hl = np.clip(np.sqrt((xx - hl_cx) ** 2 + (yy - hl_cy) ** 2) / (iris_r * 1.5), 0, 1)
iris_rgb = np.zeros((S, S, 3), dtype=np.float64)
for c in range(3):
    near = lerp(IRIS_HIGHLIGHT[c], IRIS_MID[c], np.clip(d_hl * 1.7, 0, 1))
    far = lerp(IRIS_MID[c], IRIS_RIM[c], np.clip((d_hl - 0.5) / 0.5, 0, 1))
    iris_rgb[..., c] = np.where(d_hl < 0.5, near, far)

# ---- soft glow behind the iris ----
d_center = np.sqrt((xx - iris_cx) ** 2 + (yy - iris_cy) ** 2)
glow_mask = np.clip(1 - (d_center - iris_r) / (iris_r * 0.5), 0, 1) ** 2 * 0.35
glow_layer = np.dstack(
    [np.tile(np.array(IRIS_MID, dtype=np.float64), (S, S, 1)), (glow_mask * 255)[..., None]]
)
img.alpha_composite(Image.fromarray(glow_layer.astype(np.uint8), "RGBA"))

# ---- aperture blades: bold polygons, painted with the gradient field ----
N_BLADES = 7
R_OUTER = iris_r * 1.02
R_INNER = iris_r * 0.14
HALF_ANGLE_OUTER = 27.0
SKEW = 24.0


def point(radius, angle_deg):
    a = math.radians(angle_deg)
    return (iris_cx + radius * math.sin(a), iris_cy - radius * math.cos(a))


blade_mask_img = Image.new("L", (S, S), 0)
bd = ImageDraw.Draw(blade_mask_img)
for i in range(N_BLADES):
    rot = i * (360.0 / N_BLADES)
    pts = [
        point(R_OUTER, rot - HALF_ANGLE_OUTER),
        point(R_OUTER, rot + HALF_ANGLE_OUTER),
        point(R_INNER, rot + HALF_ANGLE_OUTER + SKEW),
        point(R_INNER, rot - HALF_ANGLE_OUTER + SKEW),
    ]
    bd.polygon(pts, fill=255)

blade_mask = np.array(blade_mask_img, dtype=np.float64) / 255.0
blade_layer = np.dstack([iris_rgb, (blade_mask * 255)[..., None]])
img.alpha_composite(Image.fromarray(blade_layer.astype(np.uint8), "RGBA"))

# ---- blade outlines for definition ----
outline_img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
od = ImageDraw.Draw(outline_img)
for i in range(N_BLADES):
    rot = i * (360.0 / N_BLADES)
    pts = [
        point(R_OUTER, rot - HALF_ANGLE_OUTER),
        point(R_OUTER, rot + HALF_ANGLE_OUTER),
        point(R_INNER, rot + HALF_ANGLE_OUTER + SKEW),
        point(R_INNER, rot - HALF_ANGLE_OUTER + SKEW),
    ]
    od.polygon(pts, outline=BLADE_STROKE + (140,), width=max(1, int(S * 0.0035)))
img.alpha_composite(outline_img)

# ---- pupil (aperture opening) ----
pupil_r = R_INNER * 1.35
d_pupil = np.sqrt((xx - iris_cx) ** 2 + (yy - iris_cy) ** 2)
rim_w = pupil_r * 0.35
rim_mask = np.clip((pupil_r + rim_w - d_pupil) / rim_w, 0, 1) * np.clip(
    (d_pupil - pupil_r + rim_w) / rim_w, 0, 1
)
rim_layer = np.dstack(
    [np.tile(np.array((5, 3, 2), dtype=np.float64), (S, S, 1)), (rim_mask * 160)[..., None]]
)
img.alpha_composite(Image.fromarray(rim_layer.astype(np.uint8), "RGBA"))

pupil_mask = np.clip((pupil_r - d_pupil) / (S * 0.004), 0, 1)
pupil_layer = np.dstack(
    [np.tile(np.array(PUPIL, dtype=np.float64), (S, S, 1)), pupil_mask[..., None] * 255]
)
img.alpha_composite(Image.fromarray(pupil_layer.astype(np.uint8), "RGBA"))

# ---- downsample ----
img = img.convert("RGB").resize((1024, 1024), Image.LANCZOS)
out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon-1024.png")
img.save(out_path)
print("wrote", out_path)

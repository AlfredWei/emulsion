#!/usr/bin/env python3
"""Emulsion app icon: bold aperture-blade iris, painted with a warm glossy
amber gradient (matching app/src/lib/styles/tokens.css), on the app's dark
background. Full-bleed square, no pre-baked corner rounding -- OS icon
masks (macOS squircle, Windows tile shapes) apply their own shape.

v2: mechanical beveled blade seams (dark groove + bright highlight core,
mimicking real overlapping aperture blades catching light) and an
iridescent blue/violet rim near the outer edge -- like anti-reflective
lens coating -- blending into the warm amber highlight, using the same
blue/purple already in the app's color-label palette (tokens.css) rather
than introducing new colors.

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
GROOVE = (18, 10, 5)
EDGE_HIGHLIGHT = (255, 224, 168)

# lens-coating iridescence, same blue/purple as the app's color-label swatches
RIM_BLUE = hex_to_rgb("#5b9ee1")
RIM_PURPLE = hex_to_rgb("#a87ce0")

yy, xx = np.mgrid[0:S, 0:S].astype(np.float64)


def radial_mask(cx, cy, r, softness):
    d = np.sqrt((xx - cx) ** 2 + (yy - cy) ** 2)
    return np.clip((r - d) / softness, 0, 1), d


def composite(base_img, rgb_field, alpha_field):
    layer = np.dstack([rgb_field, (alpha_field * 255)[..., None]])
    base_img.alpha_composite(Image.fromarray(layer.astype(np.uint8), "RGBA"))


# ---- background ----
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
composite(img, np.tile(np.array(IRIS_MID, dtype=np.float64), (S, S, 1)), glow_mask)

# ---- aperture blades: bold polygons, painted with the gradient field ----
N_BLADES = 7
R_OUTER = iris_r * 1.02
R_INNER = iris_r * 0.14
HALF_ANGLE_OUTER = 27.0
SKEW = 24.0


def point(radius, angle_deg):
    a = math.radians(angle_deg)
    return (iris_cx + radius * math.sin(a), iris_cy - radius * math.cos(a))


def blade_pts(i):
    rot = i * (360.0 / N_BLADES)
    return [
        point(R_OUTER, rot - HALF_ANGLE_OUTER),
        point(R_OUTER, rot + HALF_ANGLE_OUTER),
        point(R_INNER, rot + HALF_ANGLE_OUTER + SKEW),
        point(R_INNER, rot - HALF_ANGLE_OUTER + SKEW),
    ]


blade_mask_img = Image.new("L", (S, S), 0)
bd = ImageDraw.Draw(blade_mask_img)
for i in range(N_BLADES):
    bd.polygon(blade_pts(i), fill=255)
blade_mask = np.array(blade_mask_img, dtype=np.float64) / 255.0
composite(img, iris_rgb, blade_mask)

# ---- iridescent rim: angular blue -> violet -> warm highlight sweep,
# masked to a thin band near the outer edge of the actual blade silhouette
# (not a plain circle -- follows blade_mask so it respects the notches at
# each blade seam, reading as light catching the true edge, not a decal).
angle = (np.degrees(np.arctan2(xx - iris_cx, -(yy - iris_cy))) + 360) % 360
# 0 deg (up, where the highlight already lives) is warm; sweeping away from
# it the rim cools to blue then violet then warms back up on the other side.
# two full cool->cool->warm cycles around the ring (like light breaking into
# a coating's blue/violet sheen at several points, not just one side) plus a
# narrow warm notch right at the existing highlight so the two effects
# blend rather than fight.
cycle_t = (np.sin(np.radians(angle) * 2 + math.radians(40)) + 1) / 2  # 0..1, 2 cycles
hl_angle_dist = np.abs(((angle - 320 + 180) % 360) - 180) / 180.0  # 0 at highlight
warm_window = np.clip(1 - hl_angle_dist / 0.22, 0, 1) ** 2

rim_rgb = np.zeros((S, S, 3), dtype=np.float64)
for c in range(3):
    cool = lerp(RIM_BLUE[c], RIM_PURPLE[c], cycle_t)
    rim_rgb[..., c] = lerp(cool, IRIS_HIGHLIGHT[c], warm_window)

d_edge = np.sqrt((xx - iris_cx) ** 2 + (yy - iris_cy) ** 2)
rim_band = np.clip(1 - np.abs(d_edge - R_OUTER * 0.94) / (R_OUTER * 0.14), 0, 1) ** 1.1
rim_alpha = rim_band * blade_mask * 0.9
composite(img, rim_rgb, rim_alpha)

# ---- blade seams: dark groove first, bright highlight core on top --
# mimics real overlapping aperture blades catching light at their edges.
groove_img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
gd = ImageDraw.Draw(groove_img)
for i in range(N_BLADES):
    gd.polygon(blade_pts(i), outline=GROOVE + (210,), width=max(2, int(S * 0.0062)))
img.alpha_composite(groove_img)

highlight_img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
hd = ImageDraw.Draw(highlight_img)
for i in range(N_BLADES):
    hd.polygon(blade_pts(i), outline=EDGE_HIGHLIGHT + (150,), width=max(1, int(S * 0.0022)))
img.alpha_composite(highlight_img)

# ---- pupil (aperture opening) ----
pupil_r = R_INNER * 1.35
d_pupil = np.sqrt((xx - iris_cx) ** 2 + (yy - iris_cy) ** 2)
rim_w = pupil_r * 0.35
pupil_rim_mask = np.clip((pupil_r + rim_w - d_pupil) / rim_w, 0, 1) * np.clip(
    (d_pupil - pupil_r + rim_w) / rim_w, 0, 1
)
composite(img, np.tile(np.array((5, 3, 2), dtype=np.float64), (S, S, 1)), pupil_rim_mask * 0.63)

pupil_mask = np.clip((pupil_r - d_pupil) / (S * 0.004), 0, 1)
composite(img, np.tile(np.array(PUPIL, dtype=np.float64), (S, S, 1)), pupil_mask)

# ---- downsample ----
img = img.convert("RGB").resize((1024, 1024), Image.LANCZOS)
out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon-1024.png")
img.save(out_path)
print("wrote", out_path)

#!/usr/bin/env python3
"""Emulsion app icon: an abstract liquid droplet, not a camera part.

"Emulsion" is literally a light-sensitive chemical coating -- a liquid,
historically oil/gelatin based. Emulsions and thin liquid films naturally
show iridescent, oil-slick sheen where they catch light at an angle. That's
the actual concept here: a warm amber droplet with that iridescent sheen at
its edge, plus a small satellite droplet for asymmetry. No camera hardware
imagery (no aperture blades, no lens rings) -- deliberately dropped after
that direction didn't land.

Full-bleed square, no pre-baked corner rounding -- OS icon masks (macOS
squircle, Windows tile shapes) apply their own shape.

Regenerate:
    python3 -m venv venv && ./venv/bin/pip install Pillow numpy
    ./venv/bin/python generate_icon.py
    cd ../../../.. && npx tauri icon app/src-tauri/icons/source/icon-1024.png
"""

import math
import os
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

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
# v2: more saturated across the whole gradient -- the pale cream highlight
# (was #fbd08c, only ~44% saturation) and muddy brown shadow (was #6e3f18)
# were washing the droplet out and reading as dull. Same hue family, more
# vivid at every stop.
DROP_HIGHLIGHT = hex_to_rgb("#ffb238")
DROP_MID = hex_to_rgb("#f2851a")
DROP_DEEP = hex_to_rgb("#9c3d10")

# iridescent oil-slick sheen, using the same blue/purple already in the
# app's color-label palette (tokens.css --label-blue/--label-purple)
SHEEN_BLUE = hex_to_rgb("#5b9ee1")
SHEEN_PURPLE = hex_to_rgb("#a87ce0")
SHEEN_GREEN = hex_to_rgb("#6fcb6b")

yy, xx = np.mgrid[0:S, 0:S].astype(np.float64)


def composite(base_img, rgb_field, alpha_field):
    layer = np.dstack([rgb_field, (alpha_field * 255)[..., None]])
    base_img.alpha_composite(Image.fromarray(layer.astype(np.uint8), "RGBA"))


def teardrop_radius(theta_deg, base_r, wobble_seed=0.0):
    """Radius as a function of angle: a gently asymmetric organic blob
    (slightly fuller at the bottom, like a drop settling under its own
    weight), NOT a literal sharp-pointed teardrop -- an earlier version
    that pinched to a point at the top produced a heart-shaped silhouette
    instead (two lobes either side of the pinch), which is the wrong
    read entirely. A smooth single-min/single-max cosine taper plus a
    little organic wobble avoids that failure mode.
    """
    theta = np.radians(theta_deg)
    taper = (1 - np.cos(theta)) / 2  # 0 at top (theta=0), 1 at bottom (theta=pi)
    base = 0.86 + 0.16 * taper  # top ~0.86*base_r, bottom ~1.02*base_r -- subtle, no cusp
    wobble = (
        1.0
        + 0.045 * np.sin(theta * 3 + wobble_seed)
        + 0.025 * np.sin(theta * 5 + wobble_seed * 1.7)
    )
    return base_r * base * wobble


def blob_mask_array(cx, cy, base_r, wobble_seed=0.0, n_pts=720):
    pts = []
    for i in range(n_pts):
        deg = i * 360.0 / n_pts
        r = teardrop_radius(deg, base_r, wobble_seed)
        a = math.radians(deg)
        pts.append((cx + r * math.sin(a), cy - r * math.cos(a)))
    mask_img = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask_img).polygon(pts, fill=255)
    return np.array(mask_img, dtype=np.float64) / 255.0, pts


# ---- background ----
d_bg = np.clip(np.sqrt((xx - C) ** 2 + (yy - C) ** 2) / (S * 0.75), 0, 1)
bg = np.stack([lerp(BG_CENTER[c], BG_EDGE[c], d_bg) for c in range(3)], axis=-1)
img = Image.fromarray(bg.astype(np.uint8), "RGB").convert("RGBA")

# ---- main droplet ----
drop_r = S * 0.30
drop_cx, drop_cy = C - S * 0.01, C + S * 0.02
mask, boundary_pts = blob_mask_array(drop_cx, drop_cy, drop_r, wobble_seed=0.6)

# soft glow behind the droplet
d_center = np.sqrt((xx - drop_cx) ** 2 + (yy - drop_cy) ** 2)
glow_mask = np.clip(1 - (d_center - drop_r) / (drop_r * 0.55), 0, 1) ** 2 * 0.32
composite(img, np.tile(np.array(DROP_MID, dtype=np.float64), (S, S, 1)), glow_mask)

# glossy fill: off-center highlight like light on a liquid surface
hl_cx, hl_cy = drop_cx - drop_r * 0.32, drop_cy - drop_r * 0.4
d_hl = np.clip(np.sqrt((xx - hl_cx) ** 2 + (yy - hl_cy) ** 2) / (drop_r * 1.5), 0, 1)
fill_rgb = np.zeros((S, S, 3), dtype=np.float64)
for c in range(3):
    near = lerp(DROP_HIGHLIGHT[c], DROP_MID[c], np.clip(d_hl * 1.7, 0, 1))
    far = lerp(DROP_MID[c], DROP_DEEP[c], np.clip((d_hl - 0.5) / 0.5, 0, 1))
    fill_rgb[..., c] = np.where(d_hl < 0.5, near, far)
composite(img, fill_rgb, mask)

# ---- iridescent sheen ring near the droplet's own (irregular) edge ----
inner_mask, _ = blob_mask_array(drop_cx, drop_cy, drop_r * 0.84, wobble_seed=0.6)
rim_mask = np.clip(mask - inner_mask, 0, 1)

angle = (np.degrees(np.arctan2(xx - drop_cx, -(yy - drop_cy))) + 360) % 360
cyc = np.radians(angle)
sheen_rgb = np.zeros((S, S, 3), dtype=np.float64)
w_blue = (np.sin(cyc * 2 + 0.3) + 1) / 2
w_purple = (np.sin(cyc * 2 + 2.3) + 1) / 2
w_green = (np.sin(cyc * 2 + 4.3) + 1) / 2
total = w_blue + w_purple + w_green + 1e-6
for c in range(3):
    sheen_rgb[..., c] = (
        w_blue * SHEEN_BLUE[c] + w_purple * SHEEN_PURPLE[c] + w_green * SHEEN_GREEN[c]
    ) / total

# blend the sheen toward the warm highlight color near the existing glossy
# highlight so the two effects read as one coherent light interaction
hl_angle_dist = np.abs(((angle - 315 + 180) % 360) - 180) / 180.0
warm_window = np.clip(1 - hl_angle_dist / 0.30, 0, 1) ** 2
sheen_final = np.zeros((S, S, 3), dtype=np.float64)
for c in range(3):
    sheen_final[..., c] = lerp(sheen_rgb[..., c], DROP_HIGHLIGHT[c], warm_window * 0.8)

composite(img, sheen_final, rim_mask * 0.8)

# thin bright edge line right at the boundary for a crisp liquid-surface-
# tension look (surface tension makes droplet edges catch a bright rim)
edge_img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(edge_img).polygon(
    boundary_pts, outline=(255, 235, 205, 130), width=max(1, int(S * 0.0022))
)
img.alpha_composite(edge_img)

# small specular highlight dot for glassy realism
spec_cx, spec_cy = drop_cx - drop_r * 0.38, drop_cy - drop_r * 0.5
spec_r = drop_r * 0.09
d_spec = np.sqrt((xx - spec_cx) ** 2 + (yy - spec_cy) ** 2)
spec_mask = np.clip((spec_r - d_spec) / (spec_r * 0.9), 0, 1) ** 1.5
composite(img, np.tile(np.array((255, 244, 224), dtype=np.float64), (S, S, 1)), spec_mask * 0.8)

# ---- small satellite droplet for asymmetry ----
sat_r = drop_r * 0.22
sat_cx, sat_cy = drop_cx + drop_r * 1.05, drop_cy + drop_r * 0.55
sat_mask, sat_pts = blob_mask_array(sat_cx, sat_cy, sat_r, wobble_seed=2.4)

sat_hl_cx, sat_hl_cy = sat_cx - sat_r * 0.35, sat_cy - sat_r * 0.4
d_sat_hl = np.clip(np.sqrt((xx - sat_hl_cx) ** 2 + (yy - sat_hl_cy) ** 2) / (sat_r * 1.5), 0, 1)
sat_rgb = np.zeros((S, S, 3), dtype=np.float64)
for c in range(3):
    near = lerp(DROP_HIGHLIGHT[c], DROP_MID[c], np.clip(d_sat_hl * 1.7, 0, 1))
    far = lerp(DROP_MID[c], DROP_DEEP[c], np.clip((d_sat_hl - 0.5) / 0.5, 0, 1))
    sat_rgb[..., c] = np.where(d_sat_hl < 0.5, near, far)
composite(img, sat_rgb, sat_mask)

sat_inner, _ = blob_mask_array(sat_cx, sat_cy, sat_r * 0.8, wobble_seed=2.4)
sat_rim = np.clip(sat_mask - sat_inner, 0, 1)
composite(img, sheen_final, sat_rim * 0.7)

# ---- downsample, then a whisper of blur to settle the edges ----
img = img.convert("RGB").resize((1024, 1024), Image.LANCZOS)
img = img.filter(ImageFilter.GaussianBlur(radius=0.6))

out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon-1024.png")
img.save(out_path)
print("wrote", out_path)

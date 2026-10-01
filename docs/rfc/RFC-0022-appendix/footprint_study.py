#!/usr/bin/env python3
"""RFC-0022 appendix: how much does point-sampling a grain field overstate
its per-pixel amplitude when the grain is smaller than a pixel, and does a
single closed-form factor correct it?

The ground truth is the continuous field INTEGRATED over each pixel's 1x1
footprint (8x8 supersampling). The candidate is the RFC-0020 point-sampled
generator times r(c) = c / sqrt(c^2 + K2), c = grain cell size in pixels.

    ../RFC-0020-appendix/venv or any venv with numpy+scipy:
    python3 footprint_study.py
"""
import os, sys
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "RFC-0020-appendix"))
from grainlab import grain_v2, grid  # the RFC-0020 reference implementation

N, SS, RHO = 64, 8, 0.5
OFFS = (np.arange(SS) + 0.5) / SS
K2 = 0.7


def l1(f):
    return np.corrcoef(f[:, :-1].ravel(), f[:, 1:].ravel())[0, 1]


print(f"{'cell px':>8} {'std point':>10} {'std box-avg':>12} {'ratio':>7} {'fit r(c)':>9} {'lag1 point':>11} {'lag1 box':>9}")
for c in (0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.5, 6.0):
    x, y = grid(N)
    acc = np.zeros((N, N), np.float64)
    for oy in OFFS:
        for ox in OFFS:
            acc += grain_v2((x - 0.5 + ox) / c, (y - 0.5 + oy) / c, RHO, 2, 0)  # pixel i covers [i-0.5, i+0.5]
    box = acc / (SS * SS)
    pt = grain_v2(x / c, y / c, RHO, 2, 0)
    fit = c / np.sqrt(c * c + K2)
    print(f"{c:8.2f} {pt.std():10.3f} {box.std():12.3f} {box.std()/pt.std():7.3f} {fit:9.3f} {l1(pt):+11.3f} {l1(box):+9.3f}")

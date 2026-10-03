"""Guided-filter edge refinement of a soft sky probability map (He et al.), numpy only."""
import numpy as np
def box(x, r):
    k = 2 * r + 1; p = np.pad(x, ((r + 1, r), (r + 1, r)), mode="edge"); c = p.cumsum(0).cumsum(1)
    H, W = x.shape
    s = c[k:k + H, k:k + W] - c[:H, k:k + W] - c[k:k + H, :W] + c[:H, :W]
    return s / (k * k)
def guided(I, p, r, eps):
    mI, mp = box(I, r), box(p, r); cov = box(I * p, r) - mI * mp; var = box(I * I, r) - mI * mI
    a = cov / (var + eps); b = mp - a * mI
    return box(a, r) * I + box(b, r)

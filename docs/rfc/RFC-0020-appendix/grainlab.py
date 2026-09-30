#!/usr/bin/env python3
"""RFC-0020 appendix: reference implementation + measurements for Grain.

Everything the RFC quotes as "measured" that does not need a GPU comes from
this file. It is a NUMPY REFERENCE, independent of the Rust/WGSL code under
test, which is also what makes its outputs usable as golden values.

    python3 -m venv venv && ./venv/bin/pip install numpy scipy
    ./venv/bin/python grainlab.py            # prints every table in RFC-0020

Sections printed (matching the RFC):
  hash     -- §3.1 integer-hash golden vectors + uniformity (mean/std/lag-1)
  cpu      -- §1.2 "CPU (accurate sin)" column at the GPU-repeat lags
  station  -- §1.3 variance-map stationarity table (800 seeds; slow, ~2 min)
  metrics  -- §3.2 autocorrelation / spectral spike / isotropy
  golden   -- §5 golden full-function vectors + N(rho)
  delta    -- §3.3 old-vs-new amplitude calibration numbers
  montage  -- writes montage.png: value noise vs grain v2 (needs Pillow)
The GPU-side measurements are in gpu_probe.js (needs a real WebGPU adapter).
"""
import sys
import numpy as np
from scipy.ndimage import gaussian_filter, uniform_filter

U32 = np.uint32


# ---------------------------------------------------------------- hashes
def lowbias32(x):
    x = x.astype(U32)
    x ^= x >> U32(16); x *= U32(0x7FEB352D)
    x ^= x >> U32(15); x *= U32(0x846CA68B)
    x ^= x >> U32(16)
    return x


def h32(ix, iy, seed=0):
    """particle_hash(ix, iy, s) of RFC §3.1 (u32)."""
    ix = ix.astype(np.int64).astype(U32)
    iy = iy.astype(np.int64).astype(U32)
    return lowbias32(ix ^ lowbias32(iy ^ U32(0x9E3779B9) ^ U32(seed)))


def unit(h):
    return (h >> U32(8)).astype(np.float32) / np.float32(16777216.0)


def sin_hash(ix, iy):
    """TODAY's hash, with an ACCURATE float32 sin (what the CPU/Rust side does)."""
    f = np.float32
    a = (ix.astype(f) * f(12.9898) + iy.astype(f) * f(78.233)).astype(f)
    v = (np.sin(a).astype(f) * f(43758.5453123)).astype(f)
    return (v - np.floor(v)).astype(f)


# ---------------------------------------------------------------- generators
def smooth(t):
    return t * t * (3 - 2 * t)


def value_noise(px, py, hashf):
    """Today's structure: bilinear-smoothstep value noise; hashf(ix,iy)->[0,1)."""
    ix, iy = np.floor(px), np.floor(py)
    fx, fy = (px - ix).astype(np.float32), (py - iy).astype(np.float32)
    a, b = hashf(ix, iy), hashf(ix + 1, iy)
    c, d = hashf(ix, iy + 1), hashf(ix + 1, iy + 1)
    ux, uy = smooth(fx), smooth(fy)
    ab, cd = a + (b - a) * ux, c + (d - c) * ux
    return (ab + (cd - ab) * uy).astype(np.float32)


def value_noise_seed(px, py, seed=0):
    return value_noise(px, py, lambda a, b: unit(h32(a, b, seed)))


def perlin(px, py, seed=0):
    ix, iy = np.floor(px), np.floor(py)
    fx, fy = (px - ix).astype(np.float32), (py - iy).astype(np.float32)

    def g(cx, cy, dx, dy):
        ang = unit(h32(cx, cy, seed)) * np.float32(2 * np.pi)
        return np.cos(ang) * dx + np.sin(ang) * dy

    n00, n10 = g(ix, iy, fx, fy), g(ix + 1, iy, fx - 1, fy)
    n01, n11 = g(ix, iy + 1, fx, fy - 1), g(ix + 1, iy + 1, fx - 1, fy - 1)
    fade = lambda t: t * t * t * (t * (t * 6 - 15) + 10)
    ux, uy = fade(fx), fade(fy)
    top = n00 + (n10 - n00) * ux
    return top + ((n01 + (n11 - n01) * ux) - top) * uy


def _rot(px, py, theta, scale, ox, oy):
    c, s = np.cos(theta), np.sin(theta)
    return (c * px - s * py) * scale + ox, (s * px + c * py) * scale + oy


def perlin_2oct(px, py, seed=0):
    a = perlin(*_rot(px, py, 0.5236, 1.0, 17.3, 4.1), seed=seed)
    b = perlin(*_rot(px, py, -0.9, 1.7, 9.7, 31.9), seed=seed + 1)
    return a + 0.6 * b


def sparse_conv(px, py, K=2, seed=0):
    """Plain sparse-convolution noise: K random-weight (-1..1) particles/cell."""
    ci, cj = np.floor(px), np.floor(py)
    out = np.zeros_like(px, dtype=np.float32)
    for a in (-1, 0, 1):
        for b in (-1, 0, 1):
            for k in range(K):
                h = h32(ci + a, cj + b, seed + k * 7919)
                jx = (h & U32(0x3FF)).astype(np.float32) / 1023
                jy = ((h >> U32(10)) & U32(0x3FF)).astype(np.float32) / 1023
                w = ((h >> U32(20)) & U32(0x3FF)).astype(np.float32) / 1023 * 2 - 1
                dx, dy = px - (ci + a + jx), py - (cj + b + jy)
                d2 = dx * dx + dy * dy
                out += w * np.where(d2 < 1, (1 - d2) ** 2, 0).astype(np.float32)
    return out


def grain_v2_raw(px, py, rho, K=2, seed=0):
    """RFC §3.2 `grain_particle_noise` BEFORE the N(rho) normalisation.
    px, py in cell units; rho = roughness in [0,1]."""
    f = np.float32
    ci, cj = np.floor(px), np.floor(py)
    out = np.zeros_like(px, dtype=np.float32)
    for a in (-1, 0, 1):
        for b in (-1, 0, 1):
            for k in range(K):
                h = h32(ci + a, cj + b, seed + k * 7919)
                jx = (h & U32(0xFF)).astype(f) / f(256.0)
                jy = ((h >> U32(8)) & U32(0xFF)).astype(f) / f(256.0)
                wbits = (h >> U32(16)) & U32(0xFF)
                sign = np.where((wbits & U32(0x80)) != 0, 1.0, -1.0).astype(f)
                um = (wbits & U32(0x7F)).astype(f) / f(128.0)
                ur = ((h >> U32(24)) & U32(0xFF)).astype(f) / f(256.0)
                w = sign * (f(1.0) - f(rho) * um)
                r = f(1.0) - f(0.5) * f(rho) * ur
                dx, dy = px - (ci + a + jx), py - (cj + b + jy)
                d2 = (dx * dx + dy * dy) / (r * r)
                t = f(1.0) - d2
                out += w * np.where(d2 < 1, t * t, 0).astype(f)
    return out


def v2_norm(rho, K=2):
    """N(rho) of RFC §3.2 (closed form)."""
    ew2 = 1 - rho + rho * rho / 3.0
    a = 0.5 * rho
    er2 = 1 - a + a * a / 3.0
    return 1.0 / np.sqrt(K * ew2 * 0.2 * np.pi * er2)


def grain_v2(px, py, rho, K=2, seed=0):
    return grain_v2_raw(px, py, rho, K, seed) * np.float32(v2_norm(rho, K))


# ---------------------------------------------------------------- metrics
def grid(n):
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    return x, y


def norm(f):
    return ((f - f.mean()) / f.std()).astype(np.float32)


def autocorr_max(f, minlag=12, maxlag=60):
    """Largest |normalised autocorrelation| at any lag with |lag| >= minlag px
    (linear, not circular: overlap-count normalised). This is the 'repeat' metric."""
    f = f - f.mean()
    n = f.shape[0]
    F = np.fft.rfft2(f, s=(2 * n, 2 * n))
    ac = np.fft.irfft2(F * np.conj(F), s=(2 * n, 2 * n))
    G = np.fft.rfft2(np.ones_like(f), s=(2 * n, 2 * n))
    cnt = np.fft.irfft2(G * np.conj(G), s=(2 * n, 2 * n))
    c = ac / np.maximum(cnt, 1) / f.var()
    best = (0.0, None)
    for dy in range(0, maxlag + 1):
        for dx in range(-maxlag, maxlag + 1):
            if dy == 0 and dx <= 0:
                continue
            if np.hypot(dx, dy) < minlag:
                continue
            v = c[dy % (2 * n), dx % (2 * n)]
            if abs(v) > abs(best[0]):
                best = (float(v), (dx, dy))
    return best


def autocorr_at(f, dx, dy):
    """Normalised autocorrelation at one specific lag (used for 'CPU at the GPU's lag')."""
    f = f - f.mean()
    a = f[max(0, -dy):f.shape[0] - max(0, dy), max(0, -dx):f.shape[1] - max(0, dx)]
    b = f[max(0, dy):, max(0, dx):][:a.shape[0], :a.shape[1]] if dx >= 0 else \
        f[max(0, dy):, :f.shape[1] + dx][:a.shape[0], :a.shape[1]]
    if dx < 0:
        a = f[:f.shape[0] - dy, -dx:]
        b = f[dy:, :f.shape[1] + dx]
    else:
        a = f[:f.shape[0] - dy, :f.shape[1] - dx]
        b = f[dy:, dx:]
    return float((a * b).mean() / f.var())


def spectrum_peaks(f, R=7):
    """(max, count) of power-spectrum bins that exceed 30x their local mean."""
    f = f - f.mean()
    P = np.abs(np.fft.fft2(f)) ** 2
    P[0, 0] = 0
    loc = uniform_filter(P, size=2 * R + 1, mode="wrap") * ((2 * R + 1) ** 2)
    loc = (loc - P) / ((2 * R + 1) ** 2 - 1)
    r = P / np.maximum(loc, 1e-30)
    r[0, 0] = 0
    return float(r.max()), int((r > 30).sum())


def anisotropy(f):
    """Spectral energy near the axes vs near the diagonals (1.0 = isotropic)."""
    f = f - f.mean()
    n = f.shape[0]
    P = np.abs(np.fft.fftshift(np.fft.fft2(f))) ** 2
    ky, kx = np.mgrid[-n // 2:n // 2, -n // 2:n // 2] / n
    r = np.hypot(kx, ky)
    th = np.degrees(np.arctan2(ky, kx)) % 90
    ann = (r > 0.05) & (r < 0.35)
    ax = ann & ((th < 15) | (th > 75))
    dg = ann & (th > 30) & (th < 60)
    return float(P[ax].mean() / P[dg].mean())


def variance_map_stats(gen, seeds, n=64, cell=6.0):
    """Stationarity: per-pixel variance ACROSS independent seeds, 3x3-smoothed.
    Returns (std/mean, max/min). A stationary field has no structure here."""
    x, y = grid(n)
    stack = np.stack([gen(x / cell, y / cell, s * 104729 + 1) for s in range(seeds)])
    v = uniform_filter(stack.var(axis=0), size=3, mode="wrap")
    return float(v.std() / v.mean()), float(v.max() / v.min())


# ---------------------------------------------------------------- reports
def sec_hash():
    print("== hash: golden vectors (particle_hash(ix,iy,0) and >>8 / 2^24)")
    for ix, iy in [(0, 0), (1, 0), (0, 1), (1, 1), (17, 42), (4095, 3071), (123456, 654321)]:
        h = int(h32(np.array([ix]), np.array([iy]))[0])
        print(f"  ({ix},{iy}) 0x{h:08x}  {float(unit(np.array([h], U32))[0])!r}")
    x, y = grid(512)
    v = unit(h32(x, y)).astype(np.float64)
    print(f"  512^2: mean={v.mean():.4f} std={v.std():.4f} "
          f"lag1 x={np.corrcoef(v[:, :-1].ravel(), v[:, 1:].ravel())[0,1]:+.4f} "
          f"y={np.corrcoef(v[:-1].ravel(), v[1:].ravel())[0,1]:+.4f}")


def sec_cpu():
    print("== cpu: today's OLD grain with ACCURATE sin (Rust-equivalent), Amount 100, Roughness 50")
    n = 256
    x, y = grid(n)
    for size, cell, lag in [(0, 1.0, (2, 13)), (25, 2.25, (4, 29)), (50, 3.5, (7, 46))]:
        px, py = x / cell, y / cell
        smooth_n = value_noise(px, py, sin_hash)
        rough_n = sin_hash(np.floor(px), np.floor(py))
        f = (smooth_n + (rough_n - smooth_n) * 0.5) * 2 - 1
        print(f"  Size {size:>2}: autocorr at GPU lag {lag} = {autocorr_at(f, *lag):+.3f}   "
              f"(max over all lags >=12: {autocorr_max(f)[0]:+.3f}@{autocorr_max(f)[1]})")


def sec_station():
    print("== station: variance-map stationarity, 800 seeds, 64x64 patch, cell 6 px")
    n, S = 64, 800
    rng = np.random.default_rng(3)
    for sig in (1.5, 2.0, 2.5):
        st = np.stack([gaussian_filter(rng.standard_normal((n, n)), sig, mode="wrap") for _ in range(S)])
        v = uniform_filter(st.var(axis=0), size=3, mode="wrap")
        print(f"  reference (gaussian-filtered iid, sigma={sig}): {v.std()/v.mean():.3f}  {v.max()/v.min():.2f}:1")
    gens = {
        "value noise": lambda a, b, s: value_noise_seed(a, b, s),
        "perlin": lambda a, b, s: perlin(a, b, s),
        "perlin 2-oct rotated": lambda a, b, s: perlin_2oct(a, b, s),
        "sparse-conv K=2": lambda a, b, s: sparse_conv(a, b, 2, s),
        "sparse-conv K=3": lambda a, b, s: sparse_conv(a, b, 3, s),
        "sparse-conv K=4": lambda a, b, s: sparse_conv(a, b, 4, s),
        "grain v2 rho=0": lambda a, b, s: grain_v2_raw(a, b, 0.0, 2, s),
        "grain v2 rho=0.5": lambda a, b, s: grain_v2_raw(a, b, 0.5, 2, s),
        "grain v2 rho=1": lambda a, b, s: grain_v2_raw(a, b, 1.0, 2, s),
    }
    for name, g in gens.items():
        r, mm = variance_map_stats(g, S)
        print(f"  {name:22s} rel.std={r:.3f}  max/min={mm:.2f}:1")


def sec_metrics():
    print("== metrics: 256^2 field, integer hash; iid noise is the ideal")
    n = 256
    x, y = grid(n)
    rng = np.random.default_rng(5)

    def rep(name, f):
        ac, sp = autocorr_max(f), spectrum_peaks(f)
        print(f"  {name:28s} std={f.std():.4f} maxAutocorr={ac[0]:+.3f}@{ac[1]} "
              f"specMax={sp[0]:5.1f} bins>30={sp[1]:3d} axis/diag={anisotropy(f):.2f}")

    rep("iid gaussian (ideal)", rng.standard_normal((n, n)).astype(np.float32))
    for cell in (1.0, 3.5, 6.0):
        print(f" cell {cell}")
        rep("  value noise", norm(value_noise_seed(x / cell, y / cell, 0)))
        for rho in (0.0, 0.5, 1.0):
            rep(f"  grain v2 rho={rho}", norm(grain_v2_raw(x / cell, y / cell, rho, 2, 0)))


def sec_golden():
    print("== golden: grain_particle_noise (seed 0, K=2), cell-space points")
    for rho in (0.0, 0.5, 1.0):
        row = []
        for a, b in [(0.0, 0.0), (3.25, 7.75), (100.5, 200.125), (1234.5, 2345.5)]:
            v = grain_v2(np.array([[a]], np.float32), np.array([[b]], np.float32), rho)[0, 0]
            row.append(f"{float(v):+.6f}")
        print(f"  rho={rho}: " + "  ".join(row) + f"   N={v2_norm(rho):.6f}")
    print("  std check over 800 seeds x 64x64 @cell 6:")
    x, y = grid(64)
    for rho in (0.0, 0.5, 1.0):
        s = np.stack([grain_v2(x / 6, y / 6, rho, 2, k * 104729 + 1) for k in range(200)])
        print(f"    rho={rho}: std={s.std():.3f} mean={s.mean():+.3f}")


def sec_delta():
    print("== delta: OLD grain delta std at Amount 100 (calibration target for GRAIN_SIGMA)")
    x, y = grid(512)
    cell = 1 + 0.25 * 5  # Size 25
    px, py = x / cell, y / cell
    # use the integer hash so the reference is the intended (not the GPU-corrupted) field
    hf = lambda a, b: unit(h32(a, b))
    smooth_n = value_noise(px, py, hf)
    rough_n = hf(np.floor(px), np.floor(py))
    for r in (0.0, 0.5, 1.0):
        d = ((smooth_n + (rough_n - smooth_n) * r) * 2 - 1) * 0.12
        print(f"  roughness {r*100:>3.0f}: std={d.std():.4f}")


def sec_montage():
    """Left column: today's structure (bilinear value noise, integer hash, so the
    hash is NOT what you are looking at). Right: grain v2, rho=0.5. Rows: cell 2.25
    (default Size), 6 (Size 100). Same std, same contrast stretch. Look for the
    faint square lattice in the left column."""
    from PIL import Image
    n = 192
    x, y = grid(n)
    tiles = []
    for cell in (2.25, 6.0):
        a = norm(value_noise_seed(x / cell, y / cell, 0))
        b = norm(grain_v2_raw(x / cell, y / cell, 0.5, 2, 0))
        tiles.append(np.hstack([a, np.ones((n, 6), np.float32) * 9, b]))
    img = np.vstack([tiles[0], np.ones((6, tiles[0].shape[1]), np.float32) * 9, tiles[1]])
    out = np.where(img > 8, 255, np.clip(128 + 45 * img, 0, 255)).astype(np.uint8)
    Image.fromarray(out).resize((out.shape[1] * 2, out.shape[0] * 2), Image.NEAREST).save("montage.png")
    print("wrote montage.png")


SECTIONS = dict(hash=sec_hash, cpu=sec_cpu, station=sec_station,
                metrics=sec_metrics, golden=sec_golden, delta=sec_delta, montage=sec_montage)

if __name__ == "__main__":
    want = sys.argv[1:] or [k for k in SECTIONS if k != 'montage']
    for k in want:
        SECTIONS[k]()

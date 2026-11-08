"""Slice F2 prototype: the missing texture is taken from the ring itself (blocks of its own high-pass, overlap-added
with a Hann window), scaled to the deficit, and applied only when the ring is clearly textured and the fill clearly softer."""
import json, math, statistics as st, sys
import numpy as np
from PIL import Image
from scipy import ndimage as ndi
cases = json.load(open("out/cases/cases.json")); rng = np.random.default_rng(2)
RING_MIN, RATIO_MAX, B = 2.5, 0.6, 32
def hf(a):
    g = a.astype(np.float32).mean(2); return 4*g[1:-1,1:-1]-g[:-2,1:-1]-g[2:,1:-1]-g[1:-1,:-2]-g[1:-1,2:]
def hp(x): return x - ndi.gaussian_filter(x, 2.0)
win1 = np.hanning(B + 2)[1:-1]; win = np.outer(win1, win1).astype(np.float32)
def texture(H, valid, shape, m):
    """overlap-add blocks of H copied from random fully-valid places in the ring, over the bounding box of the hole"""
    H_h, W_w = shape; ys, xs = np.where(m); y0, y1, x0, x1 = ys.min(), ys.max(), xs.min(), xs.max()
    ok = ndi.minimum_filter(valid.astype(np.uint8), size=B) > 0   # blocks whose whole B x B lies in the ring
    cy, cx = np.where(ok)
    if len(cy) < 20: return None
    out = np.zeros(shape, np.float32); acc = np.zeros(shape, np.float32)
    for y in range(y0 - B, y1 + 1, B // 2):
        for x in range(x0 - B, x1 + 1, B // 2):
            i = rng.integers(len(cy)); sy, sx = cy[i] - B // 2, cx[i] - B // 2
            if sy < 0 or sx < 0 or sy + B > H_h or sx + B > W_w: continue
            ty0, tx0 = max(y, 0), max(x, 0); ty1, tx1 = min(y + B, H_h), min(x + B, W_w)
            if ty1 <= ty0 or tx1 <= tx0: continue
            blk = H[sy + (ty0 - y): sy + (ty1 - y), sx + (tx0 - x): sx + (tx1 - x)]
            w = win[ty0 - y: ty1 - y, tx0 - x: tx1 - x]
            out[ty0:ty1, tx0:tx1] += blk * w; acc[ty0:ty1, tx0:tx1] += w
    return out / np.maximum(acc, 1e-3) * math.sqrt(2) if acc.max() > 0 else None   # overlap-add of 4 unrelated blocks halves variance roughly; sqrt2 restores about
res = []; applied = 0
for c in cases:
    d = f"out/cases/{c['id']}"
    gt = np.array(Image.open(f"{d}/crop.png").convert("RGB")).astype(np.float32); m = np.array(Image.open(f"{d}/mask.png")) > 127
    lama = np.array(Image.open(f"{d}/lama.png").convert("RGB")).astype(np.float32); luma = lama.mean(2)
    ring = ndi.binary_dilation(m, iterations=40) & ~ndi.binary_dilation(m, iterations=4)
    H = hp(luma); e_ring = float(np.sqrt((H[ring]**2).mean())); e_fill = float(np.sqrt((H[m]**2).mean()))
    out = lama; did = False
    if e_ring >= RING_MIN and e_fill / max(e_ring, 1e-6) <= RATIO_MAX:
        T = texture(H, ring, luma.shape, m)
        if T is not None:
            tm = float(np.sqrt((T[m]**2).mean()))
            amp = math.sqrt(max(0.0, e_ring**2 - e_fill**2)) / max(tm, 1e-6)
            w = np.clip(ndi.distance_transform_edt(m) / 6.0, 0, 1) * m
            out = np.clip(lama + (T * amp * w)[..., None], 0, 255); did = True; applied += 1
    r = {"id": c["id"], "size": c["size"], "applied": did}; inner = m[1:-1,1:-1]; g_e = float(np.sqrt((hf(gt)[inner]**2).mean()))
    for name, o in (("lama", lama), ("f2", out)):
        diff = (o-gt)[m]; r[name+"_psnr"] = 10*math.log10(255**2/max((diff**2).mean(),1e-9)); r[name+"_detail"] = float(np.sqrt((hf(o)[inner]**2).mean()))/max(g_e,1e-6)
    Image.fromarray(out.astype(np.uint8)).save(f"{d}/lama_f2.png"); res.append(r)
print("applied", applied, "of", len(res))
for s in ("S","M","L","all"):
    rr = [r for r in res if s=="all" or r["size"]==s]
    print(s, len(rr), "applied", sum(r["applied"] for r in rr), "  ".join(f"{k}: {st.mean(r[k+'_psnr'] for r in rr):.1f} dB, detail {st.median(r[k+'_detail'] for r in rr):.2f}" for k in ("lama","f2")))
json.dump(res, open("out/f2.json","w"))

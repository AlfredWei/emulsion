"""Slice F prototype: add the missing fine texture to a LaMa fill, measured against the bake-off ground truth.
Method: band-pass (difference of gaussians) energy of the ring just outside the hole vs inside the fill; add band-passed
noise (cheap stand-in for a texture bank) in the fill, amplitude = sqrt(max(0, ring^2 - fill^2)), faded by distance to the edge."""
import json, math, sys, statistics as st
import numpy as np
from PIL import Image
from scipy import ndimage as ndi
cases = json.load(open("out/cases/cases.json")); rng = np.random.default_rng(1)
def hf(a):
    g = a.astype(np.float32).mean(2); return 4*g[1:-1,1:-1]-g[:-2,1:-1]-g[2:,1:-1]-g[1:-1,:-2]-g[1:-1,2:]
def band(a, s1=0.8, s2=2.0): return ndi.gaussian_filter(a, s1) - ndi.gaussian_filter(a, s2)
res = []
for c in cases:
    d = f"out/cases/{c['id']}"
    gt = np.array(Image.open(f"{d}/crop.png").convert("RGB")).astype(np.float32); m = np.array(Image.open(f"{d}/mask.png")) > 127
    lama = np.array(Image.open(f"{d}/lama.png").convert("RGB")).astype(np.float32)
    ring = ndi.binary_dilation(m, iterations=24) & ~ndi.binary_dilation(m, iterations=4)
    luma = lama.mean(2); b = band(luma)
    e_ring = float(np.sqrt((b[ring]**2).mean())); e_fill = float(np.sqrt((b[m]**2).mean()))
    amp = math.sqrt(max(0.0, e_ring**2 - e_fill**2))
    noise = band(rng.standard_normal(luma.shape).astype(np.float32)); noise *= 1.0 / max(float(noise.std()), 1e-6)
    dist = ndi.distance_transform_edt(m); w = np.clip(dist / 6.0, 0, 1) * m   # fade in from the edge
    out = lama + (noise * amp * w)[..., None]
    out = np.clip(out, 0, 255)
    r = {"id": c["id"], "size": c["size"]}
    inner = m[1:-1,1:-1]; g_e = float(np.sqrt((hf(gt)[inner]**2).mean()))
    for name, o in (("lama", lama), ("lama+detail", out)):
        diff = (o-gt)[m]; r[name+"_psnr"] = 10*math.log10(255**2/max((diff**2).mean(),1e-9)); r[name+"_detail"] = float(np.sqrt((hf(o)[inner]**2).mean()))/max(g_e,1e-6)
    Image.fromarray(out.astype(np.uint8)).save(f"{d}/lama_detail.png"); res.append(r)
for s in ("S","M","L","all"):
    rr = [r for r in res if s=="all" or r["size"]==s]
    print(s, len(rr), "  ".join(f"{k}: {st.mean(r[k+'_psnr'] for r in rr):.1f} dB, detail {st.median(r[k+'_detail'] for r in rr):.2f}" for k in ("lama","lama+detail")))

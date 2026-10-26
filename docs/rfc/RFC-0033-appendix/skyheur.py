"""RFC-0033 slice A: a non-model sky resolver, measured against skyseg's masks on the 45 gt_photos (agreement, not ground truth).
Idea: sky is the smooth, top-connected region whose colour changes gradually; grown from the top row with a colour-step limit,
then bright low-saturation textured pixels next to it (clouds) are taken in, holes filled. Numpy only (no scipy / opencv)."""
import json, sys, numpy as np
from collections import deque
from PIL import Image
P = dict(edge=300, tex=0.035, step=0.045, tex_hi=0.10, min_area=0.02)
def box(a, r):
    k = 2 * r + 1; p = np.pad(a, r, mode="edge"); c = np.cumsum(np.cumsum(p, 0), 1); c = np.pad(c, ((1, 0), (1, 0)))
    return (c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]) / (k * k)
def resolve(im, P=P):
    W0, H0 = im.size; s = P["edge"] / max(W0, H0); im = im.resize((max(1, round(W0 * s)), max(1, round(H0 * s))), Image.BILINEAR)
    a = np.asarray(im, np.float32) / 255; h, w, _ = a.shape
    Y = a @ np.array([.2126, .7152, .0722], np.float32)
    gy, gx = np.gradient(box(Y, 1)); tex = box(np.hypot(gx, gy), 2) * 4          # local gradient energy
    sm = box(a[..., 0], 1), box(a[..., 1], 1), box(a[..., 2], 1); c = np.stack(sm, -1)
    ok = tex < P["tex"]
    seen = np.zeros((h, w), bool); q = deque()
    for x in range(w):
        if ok[0, x]: seen[0, x] = True; q.append((0, x))
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and not seen[ny, nx] and ok[ny, nx] and np.abs(c[ny, nx] - c[y, x]).max() < P["step"]:
                seen[ny, nx] = True; q.append((ny, nx))
    if seen.mean() < P["min_area"]: return None, (h, w), seen
    # clouds: textured but bright / low-saturation pixels touching the sky, grown a few steps
    med = np.median(Y[seen]); sat = a.max(-1) - a.min(-1)
    cloud = (tex < P["tex_hi"]) & (Y > med - 0.02) & (sat < np.median(sat[seen]) + 0.12)
    m = seen.copy()
    for _ in range(12):
        n = m.copy(); n[1:] |= m[:-1]; n[:-1] |= m[1:]; n[:, 1:] |= m[:, :-1]; n[:, :-1] |= m[:, 1:]
        m = m | (n & cloud)

    if P.get("rowcolour"):
        rows = np.where(seen.any(1))[0]; ymax = rows.max()
        ref = np.full((h, 3), np.nan, np.float32)
        for y in rows: ref[y] = np.median(c[y][seen[y]], 0)
        idx = np.arange(h); good = ~np.isnan(ref[:, 0])
        for k in range(3): ref[:, k] = np.interp(idx, idx[good], ref[good, k])
        near = np.abs(c - ref[:, None, :]).max(-1) < P["rowcolour"]
        extra = near & (tex < P["tex_hi"]) & (np.arange(h)[:, None] <= ymax + 2)
        m = m | extra
    # fill holes: background reached from the bottom / sides is not a hole
    out = ~m; reach = np.zeros_like(m); q = deque()
    for x in range(w):
        if out[h - 1, x]: reach[h - 1, x] = True; q.append((h - 1, x))
    for y in range(h):
        for x in (0, w - 1):
            if out[y, x] and not reach[y, x]: reach[y, x] = True; q.append((y, x))
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and out[ny, nx] and not reach[ny, nx]: reach[ny, nx] = True; q.append((ny, nx))
    m = m | (out & ~reach)
    return m, (h, w), seen
if __name__ == "__main__":
    meta = json.load(open("gt_photos/meta.json")); res = []
    for i, mt in enumerate(meta, 1):
        ref = np.load(f"masks/{i:02d}.npz")["skyseg"]
        m, (h, w), _ = resolve(Image.open("gt_photos/" + mt["file"]).convert("RGB"))
        r = np.asarray(Image.fromarray(ref.astype(np.uint8) * 255).resize((w, h), Image.BILINEAR)) > 127
        m = np.zeros((h, w), bool) if m is None else m
        inter, uni = (m & r).sum(), (m | r).sum()
        res.append(dict(i=i, cat=mt["category"], ref=float(r.mean()), got=float(m.mean()), iou=float(inter / uni) if uni else 1.0, wrong=float((m ^ r).mean())))
    json.dump(res, open("out_skyheur.json", "w"), indent=1)
    wrong5 = sum(x["wrong"] > .05 for x in res); iou = np.mean([x["iou"] for x in res if x["ref"] > .02])
    print(f"photos {len(res)}: >5% wrong {wrong5}; mean IoU (ref has sky) {iou:.3f}")
    by = {}
    for x in res: by.setdefault(x["cat"], []).append(x)
    for c, v in by.items(): print(f"{c:15s} n={len(v)} >5%wrong={sum(x['wrong']>.05 for x in v)} meanIoU={np.mean([x['iou'] for x in v]):.2f}")

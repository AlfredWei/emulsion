"""Where do our errors sit relative to the accepted mask's boundary? (edge band = within 12 px of the boundary at 900-px long side)"""
import json, numpy as np
from refine import box
k = json.load(open("review_key.json")); a = json.load(open("review_answers.json"))
near = far = 0; per = []
for i in range(1, 46):
    m = k[str(i)]; ok = {"A": [m["A"]], "B": [m["B"]], "both": ["ours", "skyseg"], "none": []}[a[str(i)]]
    if "skyseg" not in ok: continue
    d = np.load(f"masks/{i:02d}.npz"); gt, o = d["skyseg"], d["ours"]
    g = gt.astype(np.float64); edge = (box(g, 1) > 0.01) & (box(g, 1) < 0.99); band = box(edge.astype(np.float64), 12) > 0
    bad = o ^ gt; n_near = int((bad & band).sum()); n_far = int((bad & ~band).sum()); near += n_near; far += n_far
    per.append((i, m["category"], round(bad.mean() * 100, 1), round(100 * n_far / max(n_near + n_far, 1))))
print(f"disagreement pixels: {100*near/(near+far):.0f}% within 12 px of the boundary, {100*far/(near+far):.0f}% elsewhere (whole regions)")
print("photo category wrong-% of image, %-of-errors-far-from-edge")
for r in per: print(*r)

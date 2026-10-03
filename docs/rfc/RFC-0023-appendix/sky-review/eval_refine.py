"""Score raw vs guided-filter-refined sky probability against masks the owner accepted in the blind review (skyseg mask where skyseg was acceptable).
Grid search on odd-numbered photos, report on even-numbered ones."""
import json, itertools, numpy as np, torch
from PIL import Image
from train import UNet
from refine import guided
k = json.load(open("review_key.json")); a = json.load(open("review_answers.json")); meta = json.load(open("gt_photos/meta.json"))
net = UNet(); net.load_state_dict(torch.load("sky_unet.pt", map_location="cpu")); net.eval()
data = []
for i, mt in enumerate(meta, 1):
    m = k[str(i)]; ans = a[str(i)]; ok = {"A": [m["A"]], "B": [m["B"]], "both": ["ours", "skyseg"], "none": []}[ans]
    if "skyseg" not in ok: continue
    im = Image.open("gt_photos/" + mt["file"]).convert("RGB"); W0, H0 = im.size; s = 900 / max(W0, H0)
    im = im.resize((int(W0 * s), int(H0 * s))); W, H = im.size
    x = torch.from_numpy(np.asarray(im.resize((288, 288), Image.BILINEAR), np.float32) / 255).permute(2, 0, 1)[None]
    with torch.no_grad(): p = torch.sigmoid(net(x))[0, 0].numpy()
    pu = np.asarray(Image.fromarray(p).resize((W, H), Image.BILINEAR)).astype(np.float64)
    gray = np.asarray(im.convert("L"), np.float64) / 255
    gt = np.load(f"masks/{i:02d}.npz")["skyseg"]
    data.append((i, pu, gray, gt))
def iou(m, g): return (m & g).sum() / max((m | g).sum(), 1)
def score(rows, r=None, eps=None):
    v = [iou((pu if r is None else np.clip(guided(gray, pu, r, eps), 0, 1)) > 0.5, gt) for _, pu, gray, gt in rows]
    return float(np.mean(v)), v
tune = [d for d in data if d[0] % 2 == 1]; test = [d for d in data if d[0] % 2 == 0]
print(len(data), "photos where skyseg was acceptable;", len(tune), "tune /", len(test), "test")
res = sorted(((score(tune, r, e)[0], r, e) for r in (4, 8, 16, 32, 64) for e in (1e-5, 1e-4, 1e-3, 1e-2)), reverse=True)
print("best tune settings:", [(round(s, 3), r, e) for s, r, e in res[:4]])
_, r, e = res[0]
for name, rows in (("tune", tune), ("TEST", test), ("all", data)):
    raw, _ = score(rows); ref, v = score(rows, r, e); print(f"{name:5s} IoU vs accepted mask: raw {raw:.3f}  refined(r={r}, eps={e}) {ref:.3f}")
json.dump({"r": r, "eps": e}, open("refine_params.json", "w"))

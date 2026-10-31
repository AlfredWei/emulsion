"""Stage B (RFC-0037 section 5): detector box -> click(s) -> SAM 2 Tiny cut, scored against the COCO mask.
Usage: run_cuts.py DETECTOR [min_score=0.3] [max_per_image=8]  -> ../results_cuts/<DETECTOR>.json
Per detection and click strategy: SAM's predicted IoU, the IoU against the matched target's COCO mask (None if the detection matches no
target), the cut's coverage of the frame, and what the detection is: tp (matches a faceless target), ign (other real person / crowd), fp.
Strategies: mid = one click at the middle of the box; two = clicks at 35 % and 65 % of the box height; three = 15 %, 50 %, 85 %."""
import json, sys, pathlib, time
import numpy as np
from PIL import Image
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import sam_cut, evaluate
from pycocotools import mask as cm
ROOT = pathlib.Path(__file__).resolve().parent.parent
name = sys.argv[1]; min_score = float(sys.argv[2]) if len(sys.argv) > 2 else 0.3; cap = int(sys.argv[3]) if len(sys.argv) > 3 else 8
dets = json.load(open(ROOT / "results" / f"{name}.json"))["dets"]
rows = json.load(open(ROOT / "set" / "set.json"))
sam = sam_cut.Sam()
STRATS = {"two": [0.35, 0.65], "box": "box", "box_mid": "box+mid", "box_two": "box+two"}

def gt256(p, w, h):
    seg = p["seg"]
    rle = cm.merge(cm.frPyObjects(seg, h, w)) if isinstance(seg, list) else seg
    m = cm.decode(rle).astype(np.uint8) * 255
    return np.asarray(Image.fromarray(m).resize((256, 256), Image.BILINEAR)) > 127

def classify(d, r):
    hits, flags = evaluate.match([d], r, 0.0)
    if hits: return "tp", min(hits)
    return flags[0], None

out, t0 = [], time.time()
for n, r in enumerate(rows):
    ds = sorted([d for d in dets[r["file"]] if d[4] >= min_score], key=lambda d: -d[4])[:cap] if r["kind"] != "negative" else []
    if not ds: out.append({"file": r["file"], "kind": r["kind"], "cuts": []}); continue
    im = Image.open(ROOT / "set" / "img" / r["file"]); w, h = im.size
    sam.prepare(im)
    gts = {}
    cuts = []
    for d in ds:
        # a detection belongs to a target when it is the best-matching one (classify re-runs the matcher per box, so a duplicate box
        # of an already-claimed target counts as ign, as in evaluate.py)
        kind, ti = classify(d, r)
        if ti is not None and ti not in gts: gts[ti] = gt256(r["persons"][ti], w, h)
        x0, y0, x1, y1 = d[:4]; cx, bh = (x0 + x1) / 2 / w, (y1 - y0)
        res = {"box": [round(v, 1) for v in d[:4]], "score": round(d[4], 3), "kind": kind, "ti": ti, "strats": {}}
        for s, fr in STRATS.items():
            corners = [(x0 / w, y0 / h, 2), (x1 / w, y1 / h, 3)]
            if fr == "box": clicks = corners
            elif fr == "box+mid": clicks = corners + [(cx, (y0 + 0.5 * bh) / h, True)]
            elif fr == "box+two": clicks = corners + [(cx, (y0 + f_ * bh) / h, True) for f_ in (0.35, 0.65)]
            else: clicks = [(cx, (y0 + f_ * bh) / h, True) for f_ in fr]
            pi, logits = sam.cut(clicks)
            m = logits > 0
            giou = None
            if ti is not None:
                g = gts[ti]; giou = float((m & g).sum() / max((m | g).sum(), 1))
            res["strats"][s] = {"pred": round(pi, 3), "gt": None if giou is None else round(giou, 3), "cov": round(float(m.mean()), 4)}
        cuts.append(res)
    out.append({"file": r["file"], "kind": r["kind"], "cuts": cuts})
    if n % 50 == 0: print(name, n, len(rows), f"{time.time()-t0:.0f}s", flush=True)
(ROOT / "results_cuts").mkdir(exist_ok=True)
json.dump(out, open(ROOT / "results_cuts" / f"{name}_box.json", "w"))
print("done", name, f"{time.time()-t0:.0f}s")

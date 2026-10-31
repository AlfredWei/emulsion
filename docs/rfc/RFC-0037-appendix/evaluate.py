"""Score every ../results/<model>.json against ../set/set.json (RFC-0037 section 6).

Positives are the faceless target persons (no usable face, at least 8 % of the frame tall). A detection that matches a person who
is *not* a target (one with a face, a small one, a crowd region) is ignored, never counted as a false alarm.
Reported per model:
  AP50        COCO-style average precision at box IoU 0.5 over all images, faceless targets only
  rec@FP10    recall of faceless targets at the lowest score threshold where at most 10 % of the person-free images get any detection
              (the RFC's success criterion: at most 1 in 10 no-person photographs flagged), and that threshold
  by size     the same recall split by person height / frame height
  img-hit     for the 'faceless' kind (the app says not_found today): share of images where at least one target person is found at that threshold
Usage: evaluate.py [name...]  (default: every result file)"""
import json, sys, pathlib
import numpy as np
ROOT = pathlib.Path(__file__).resolve().parent.parent
rows = json.load(open(ROOT / "set" / "set.json"))
BINS = [(0.08, 0.15), (0.15, 0.30), (0.30, 0.60), (0.60, 9)]
def xyxy(b): return [b[0], b[1], b[0] + b[2], b[1] + b[3]]
def iou(a, b):
    ix = max(0, min(a[2], b[2]) - max(a[0], b[0])); iy = max(0, min(a[3], b[3]) - max(a[1], b[1])); i = ix * iy
    u = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - i
    return i / u if u > 0 else 0
def inside_frac(det, reg):
    ix = max(0, min(det[2], reg[2]) - max(det[0], reg[0])); iy = max(0, min(det[3], reg[3]) - max(det[1], reg[1]))
    a = (det[2] - det[0]) * (det[3] - det[1]); return ix * iy / a if a > 0 else 0

def match(dets, r, t):
    """-> (hits: set of target indices found, ignored/fp flags per detection >= t)."""
    tg = [(i, xyxy(p["bbox"])) for i, p in enumerate(r["persons"]) if p["faceless"]]
    others = [xyxy(p["bbox"]) for p in r["persons"] if not p["faceless"]] + [xyxy(b) for b in r["small"]]
    crowds = [xyxy(b) for b in r["crowd"]]
    hits, flags = set(), []
    for d in sorted([d for d in dets if d[4] >= t], key=lambda d: -d[4]):
        best, bi = 0.0, None
        for i, g in tg:
            if i in hits: continue
            v = iou(d, g)
            if v > best: best, bi = v, i
        if best >= 0.5: hits.add(bi); flags.append("tp"); continue
        if any(iou(d, o) >= 0.5 for o in others) or any(inside_frac(d, c) >= 0.5 for c in crowds) \
           or any(iou(d, g) >= 0.5 for _, g in tg): flags.append("ign"); continue
        flags.append("fp")
    return hits, flags

def ap50(name_dets):
    recs, npos = [], 0
    for r in rows:
        dets = sorted(name_dets[r["file"]], key=lambda d: -d[4])
        tg = [(i, xyxy(p["bbox"])) for i, p in enumerate(r["persons"]) if p["faceless"]]
        npos += len(tg)
        hits, _ = set(), None
        others = [xyxy(p["bbox"]) for p in r["persons"] if not p["faceless"]] + [xyxy(b) for b in r["small"]]
        crowds = [xyxy(b) for b in r["crowd"]]
        for d in dets:
            best, bi = 0.0, None
            for i, g in tg:
                if i in hits: continue
                v = iou(d, g)
                if v > best: best, bi = v, i
            if best >= 0.5: hits.add(bi); recs.append((d[4], 1)); continue
            if any(iou(d, o) >= 0.5 for o in others) or any(inside_frac(d, c) >= 0.5 for c in crowds) or any(iou(d, g) >= 0.5 for _, g in tg): continue
            recs.append((d[4], 0))
    recs.sort(key=lambda x: -x[0]); tp = np.cumsum([x[1] for x in recs]); fp = np.cumsum([1 - x[1] for x in recs])
    rec = tp / max(npos, 1); prec = tp / np.maximum(tp + fp, 1)
    mrec = np.concatenate([[0], rec, [1]]); mpre = np.concatenate([[0], prec, [0]])
    for i in range(len(mpre) - 2, -1, -1): mpre[i] = max(mpre[i], mpre[i + 1])
    idx = np.where(mrec[1:] != mrec[:-1])[0]
    return float(np.sum((mrec[idx + 1] - mrec[idx]) * mpre[idx + 1]))

def at_threshold(name_dets, t):
    neg = [r for r in rows if r["kind"] == "negative"]
    neg_flagged = sum(1 for r in neg if any(d[4] >= t for d in name_dets[r["file"]]))
    neg_animal = [r for r in neg if r["animal"]]
    neg_animal_flagged = sum(1 for r in neg_animal if any(d[4] >= t for d in name_dets[r["file"]]))
    bin_tot, bin_hit, img_hit, img_n = [0] * len(BINS), [0] * len(BINS), 0, 0
    for r in rows:
        if r["kind"] == "negative": continue
        hits, _ = match(name_dets[r["file"]], r, t)
        for i, p in enumerate(r["persons"]):
            if not p["faceless"]: continue
            for k, (lo, hi) in enumerate(BINS):
                if lo <= p["hrel"] < hi: bin_tot[k] += 1; bin_hit[k] += int(i in hits)
        if r["kind"] == "faceless": img_n += 1; img_hit += int(len(hits) > 0)
    return {"neg_rate": neg_flagged / max(len(neg), 1), "animal_rate": neg_animal_flagged / max(len(neg_animal), 1),
            "rec": sum(bin_hit) / max(sum(bin_tot), 1), "by_size": [h / max(t_, 1) for h, t_ in zip(bin_hit, bin_tot)],
            "img_hit": img_hit / max(img_n, 1)}

if __name__ == "__main__":
    names = sys.argv[1:] or sorted(p.stem for p in (ROOT / "results").glob("*.json"))
    n_t = [sum(1 for r in rows for p in r["persons"] if p["faceless"] and lo <= p["hrel"] < hi) for lo, hi in BINS]
    print(f"set: {len(rows)} images ({sum(r['kind']=='faceless' for r in rows)} faceless, {sum(r['kind']=='mixed' for r in rows)} mixed, "
          f"{sum(r['kind']=='negative' for r in rows)} no-person); faceless targets by size {n_t}")
    print(f"{'model':36s} {'ms':>6s} {'AP50':>5s} {'thr':>5s} {'rec':>5s}  by size (" + " / ".join(f'{lo:.2f}-{min(hi,1):.2f}' for lo, hi in BINS) + f")  img-hit  animals-flagged")
    for n in names:
        res = json.load(open(ROOT / "results" / f"{n}.json")); d = res["dets"]
        best = None
        for t in np.arange(0.05, 0.96, 0.025):
            m = at_threshold(d, float(t))
            if m["neg_rate"] <= 0.10 and (best is None or m["rec"] > best[1]["rec"]): best = (float(t), m)
        if best is None: print(f"{n:36s} no threshold keeps false alarms under 10 %"); continue
        t, m = best
        print(f"{n:36s} {res['ms']:6.0f} {ap50(d):5.3f} {t:5.2f} {m['rec']:5.2f}  " + " / ".join(f"{v:.2f}" for v in m["by_size"]) +
              f"      {m['img_hit']:.2f}      {m['animal_rate']:.2f}")

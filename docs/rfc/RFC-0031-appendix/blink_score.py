"""Scores the two blink candidates against hand labels (RFC-0031 §8.10).

Usage: python blink_score.py [out_dir]   (default: ./out, where blink_measure.py wrote faces.json and labels.csv)

labels.csv columns image_left / image_right take o (open), c (closed), h (half), x (cannot tell) or stay empty (not labelled yet).
Per face-width bin it reports, for each candidate, on the eyes marked o or c only:
  speaks   share of labelled eyes the candidate gives an answer for (mesh abstains below MESH_PRESENCE; BlinkCNN never abstains)
  acc      accuracy where it speaks;  closed recall / precision where it speaks
  h-closed share of half-closed eyes it calls closed (not scored as right or wrong: a person decides those)
  x-speaks share of 'cannot tell' eyes it still answers (a model that is confident there is what the RFC warns about)
"""
import csv, json, os, sys

CNN_T, BS_T, MESH_PRESENCE = 0.53, 0.5, 0.5
BINS = [(0, 24, "<24"), (24, 48, "24-48"), (48, 96, "48-96"), (96, 1e9, ">=96")]

def load(out):
    faces = {r["id"]: r for r in json.load(open(os.path.join(out, "faces.json")))}
    rows = []
    for r in csv.DictReader(open(os.path.join(out, "labels.csv"))):
        f = faces.get(int(r["id"]))
        if not f: continue
        for side in ("image_left", "image_right"):
            lab = (r[side] or "").strip().lower()
            if lab in ("o", "c", "h", "x"):
                rows.append(dict(w=f["face_w_px"], label=lab, cnn=f["cnn_closed_" + side] >= CNN_T,
                                 bs=f["blink_" + side] >= BS_T, mesh_ok=f["mesh_presence"] >= MESH_PRESENCE,
                                 bs_p=f["blink_" + side], cnn_p=f["cnn_closed_" + side]))
    return rows

def pct(a, b): return "  -  " if b == 0 else f"{100 * a / b:4.0f}%"

def report(rows, name, closed_of, speaks_of):
    print(f"\n{name}")
    print(f"{'face w px':>10} {'eyes':>5} {'speaks':>7} {'acc':>6} {'c-recall':>9} {'c-prec':>7} {'h->closed':>10} {'x-speaks':>9}")
    for lo, hi, tag in BINS + [(0, 1e9, "all")]:
        sel = [r for r in rows if lo <= r["w"] < hi]
        oc = [r for r in sel if r["label"] in "oc"]
        sp = [r for r in oc if speaks_of(r)]
        right = sum(closed_of(r) == (r["label"] == "c") for r in sp)
        c = [r for r in sp if r["label"] == "c"]; called = [r for r in sp if closed_of(r)]
        h = [r for r in sel if r["label"] == "h"]; x = [r for r in sel if r["label"] == "x"]
        print(f"{tag:>10} {len(oc):>5} {pct(len(sp), len(oc)):>7} {pct(right, len(sp)):>6} {pct(sum(closed_of(r) for r in c), len(c)):>9} "
              f"{pct(sum(r['label'] == 'c' for r in called), len(called)):>7} {pct(sum(closed_of(r) for r in h), len(h)):>10} {pct(sum(speaks_of(r) for r in x), len(x)):>9}")

def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "out"
    rows = load(out)
    if not rows: sys.exit("no labelled eyes yet: fill image_left / image_right in labels.csv with o, c, h or x")
    n = {k: sum(r["label"] == k for r in rows) for k in "ochx"}
    print(f"{len(rows)} labelled eyes: open {n['o']}, closed {n['c']}, half {n['h']}, cannot tell {n['x']}  (BlinkCNN >= {CNN_T}, blendshape >= {BS_T}, mesh presence >= {MESH_PRESENCE})")
    if n["c"] < 10: print("warning: fewer than 10 closed eyes; recall figures are not meaningful")
    report(rows, "BlinkCNN (always answers)", lambda r: r["cnn"], lambda r: True)
    report(rows, "Mesh + blendshape (abstains when the mesh is not sure there is a face)", lambda r: r["bs"], lambda r: r["mesh_ok"])
    report(rows, "Mesh + blendshape, answering everywhere (no abstain), for comparison", lambda r: r["bs"], lambda r: True)

if __name__ == "__main__":
    main()

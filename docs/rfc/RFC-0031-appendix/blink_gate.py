"""Scores 'can't tell' gates and closed-eye rules on hand labels (RFC-0031 §8.14). Rules are FIXED in this file, so a second labelled
batch is a hold-out: edit them only after reporting. Usage: python blink_gate.py faces.json labels.csv [strata.json pool.json]
With strata.json (face id -> [size band, flagged|unflagged]) and pool.json ("band|stratum" -> [pool size, sampled]) every face is weighted
by pool size / sampled, so the rates estimate the whole population the sample was drawn from, not the enriched sample."""
import csv, json, sys

EAR_T, BS_T, CNN_T, PRES_T = 0.18, 0.5, 0.53, 0.5
DETECTORS = {"EAR < 0.18 (eyelid geometry, mesh only)": lambda r: r["ear"] < EAR_T,
             "blendshape >= 0.5 (mesh only)": lambda r: r["bs"] >= BS_T,
             "EAR or blendshape (mesh only)": lambda r: r["ear"] < EAR_T or r["bs"] >= BS_T,
             "BlinkCNN >= 0.53 (training-data licence open)": lambda r: r["cnn"] >= CNN_T}
GATES = {"no gate": lambda r: True, "face >= 48 px": lambda r: r["w"] >= 48, "face >= 64 px": lambda r: r["w"] >= 64,
         "face >= 96 px": lambda r: r["w"] >= 96, "face >= 48 px + mesh sure": lambda r: r["w"] >= 48 and r["pres"] >= PRES_T,
         "face >= 96 px + mesh sure": lambda r: r["w"] >= 96 and r["pres"] >= PRES_T}

def load(faces_path, labels_path, strata=None, pool=None):
    faces = {r["id"]: r for r in json.load(open(faces_path))}; rows = []
    for r in csv.DictReader(open(labels_path)):
        f = faces.get(int(r["id"]))
        if not f: continue
        wt = 1.0
        if strata and pool:
            band, st = strata[str(r["id"])]; n_pool, n_samp = pool[f"{band}|{st}"]; wt = n_pool / max(1, n_samp)
        for side in ("image_left", "image_right"):
            lab = (r[side] or "").strip().lower()
            if lab in ("o", "c", "h", "x"):
                rows.append(dict(w=f["face_w_px"], l=lab, cnn=f["cnn_closed_" + side], bs=f["blink_" + side], ear=f["ear_" + side], pres=f["mesh_presence"], wt=wt))
    return rows

def main():
    a = sys.argv; rows = load(a[1], a[2], json.load(open(a[3])) if len(a) > 4 else None, json.load(open(a[4])) if len(a) > 4 else None)
    S = lambda xs: sum(r["wt"] for r in xs)
    pct = lambda x, y: "   -" if y == 0 else f"{100 * x / y:3.0f}%"
    n = {k: sum(1 for r in rows if r["l"] == k) for k in "ochx"}
    print(f"{len(rows)} labelled eyes (unweighted): open {n['o']}, closed {n['c']}, half {n['h']}, cannot tell {n['x']}; weighted = {'yes' if len(a) > 4 else 'no'}")
    for dn, det in DETECTORS.items():
        print(f"\n{dn}\n{'gate':28} {'answers':>8} {'acc':>5} {'c-found':>8} {'c-right':>8} {'x-answered':>11} {'h->closed':>10}")
        for gn, gate in GATES.items():
            oc = [r for r in rows if r["l"] in "oc"]; sp = [r for r in oc if gate(r)]; cl = [r for r in sp if r["l"] == "c"]; called = [r for r in sp if det(r)]
            x = [r for r in rows if r["l"] == "x"]; h = [r for r in rows if r["l"] == "h" and gate(r)]
            print(f"{gn:28} {pct(S(sp), S(oc)):>8} {pct(S([r for r in sp if det(r) == (r['l'] == 'c')]), S(sp)):>5} {pct(S([r for r in cl if det(r)]), S(cl)):>8} "
                  f"{pct(S([r for r in called if r['l'] == 'c']), S(called)):>8} {pct(S([r for r in x if gate(r)]), S(x)):>11} {pct(S([r for r in h if det(r)]), S(h)):>10}")
if __name__ == "__main__": main()

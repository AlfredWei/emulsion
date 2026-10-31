"""Score ../results_cuts/<model>.json (RFC-0037 section 5). For each click strategy:
  usable   share of faceless target persons (matched by a detection >= score) whose cut has IoU >= 0.7 with the COCO mask
  ok       the same at IoU >= 0.5
  gate     at SAM's predicted-IoU gate (0.6 = the app's PERSON_MIN_IOU): share of true-person cuts that pass, mean GT IoU of those, and
           share of FALSE detections (not a person, not an ignored person) that pass as well
  neg      person-free photographs where at least one detection >= score survives the gate (what the user would see as a wrong 'check')
Usage: score_cuts.py model [score=0.5]"""
import json, sys, pathlib
import numpy as np
ROOT = pathlib.Path(__file__).resolve().parent.parent
name = sys.argv[1]; thr = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
data = json.load(open(ROOT / "results_cuts" / f"{name}.json"))
print(f"{name} at detector score >= {thr}")
for s in ("mid", "two", "three"):
    tp = [(c["strats"][s]["pred"], c["strats"][s]["gt"]) for im in data for c in im["cuts"] if c["kind"] == "tp" and c["score"] >= thr]
    fp = [c["strats"][s]["pred"] for im in data for c in im["cuts"] if c["kind"] == "fp" and c["score"] >= thr]
    gt = np.array([g for _, g in tp]); pr = np.array([p for p, _ in tp])
    gate = pr >= 0.6
    neg = [im for im in data if im["kind"] == "negative"]
    neg_pass = sum(1 for im in neg if any(c["score"] >= thr and c["strats"][s]["pred"] >= 0.6 for c in im["cuts"]))
    print(f"  {s:5s} n={len(tp):4d} usable(>=.7) {np.mean(gt>=.7):.2f}  ok(>=.5) {np.mean(gt>=.5):.2f}  mean IoU {gt.mean():.2f} | gate .6: pass {gate.mean():.2f}, "
          f"IoU of passed {gt[gate].mean():.2f}, passed&usable {np.mean((gt>=.7)[gate]):.2f} | false dets {len(fp)}, pass gate {np.mean(np.array(fp)>=.6) if fp else 0:.2f} | "
          f"no-person photos with a surviving cut {neg_pass}/{len(neg)}")

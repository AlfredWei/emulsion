"""Run named detectors over ../set/set.json; writes ../results/<name>.json = {"ms": median ms per image, "dets": {file: [[x0,y0,x1,y1,score],..]}}.
Usage: run_detectors.py name [name...]   (resumable per model: skips a finished result)"""
import json, sys, time, pathlib, statistics
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import detectors
from PIL import Image
ROOT = pathlib.Path(__file__).resolve().parent.parent
rows = json.load(open(ROOT / "set" / "set.json"))
(ROOT / "results").mkdir(exist_ok=True)
for name in sys.argv[1:]:
    out = ROOT / "results" / f"{name}.json"
    if out.exists(): print("skip", name); continue
    try:
        fn = detectors.make(name)
    except Exception as e:
        print("LOAD FAIL", name, str(e)[:200], flush=True); continue
    dets, ms = {}, []
    for i, r in enumerate(rows):
        im = Image.open(ROOT / "set" / "img" / r["file"])
        t = time.time(); d = fn(im); ms.append((time.time() - t) * 1000)
        dets[r["file"]] = [list(x) for x in d]
        if i % 100 == 0: print(name, i, len(rows), f"{statistics.median(ms):.0f} ms", flush=True)
    json.dump({"ms": statistics.median(ms), "device": detectors.DEVICE, "dets": dets}, open(out, "w"))
    print("done", name, f"median {statistics.median(ms):.0f} ms on", detectors.DEVICE, flush=True)

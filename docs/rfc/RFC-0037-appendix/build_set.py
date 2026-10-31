"""Build the person-detector evaluation set from COCO val2017 (RFC-0037 section 6).

Every licence is used (local evaluation only, never redistributed or trained on; COCO licence ids 4, 7, 8 are the permissive ones and
stay in set.json for a subset run). Every person has a COCO box and mask. A person is *faceless* when the app's face detector (the numpy YuNet
port from the RFC-0033 appendix, with the app's 24 px floor at a 1600 px frame) finds no face centred in the upper half of
the person's box. Image kinds:
  faceless  - a target person and no kept face anywhere: the app answers not_found today (the case the detector is for)
  mixed     - a faceless target person and a kept face elsewhere: faces are found, the faceless person is missed
  negative  - no person annotated at all (animals and other lookalikes are sampled on purpose)
Writes ../set/set.json and copies the chosen JPEGs to ../set/img/. Needs the zips from the folders next to ../scripts."""
import json, random, sys, zipfile, pathlib, shutil
import numpy as np
from PIL import Image
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, "/Users/weishih-yi/src/claude_cowork_projects/lr_replace/docs/rfc/RFC-0033-appendix")
import faceheur  # noqa: E402  (the app's YuNet, ported)
YUNET = "/Users/weishih-yi/Library/Application Support/dev.alfredwei.emulsion/face_models/face_detection_yunet_2023mar.onnx"
OK_LICENCES = None        # None = every licence: this is a local, never-redistributed evaluation, not training data; the licence id stays in set.json
MIN_H = 0.08          # a target person is at least 8 % of the frame tall
FACE_MIN_PX, REF = 24.0, 1600.0
CAPS = {"faceless": 300, "mixed": 200, "negative": 200}
ANIMALS = {16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 88}   # bird..giraffe, teddy bear (COCO category ids)
random.seed(0)

inst = json.load(open(ROOT / "coco_ann_json" / "instances_val2017.json"))      # one member of annotations_trainval2017.zip (remote_member.py)
imgs = {i["id"]: i for i in inst["images"] if OK_LICENCES is None or i["license"] in OK_LICENCES}
by_img = {}
for a in inst["annotations"]:
    if a["image_id"] in imgs:
        by_img.setdefault(a["image_id"], []).append(a)
print(len(imgs), "licence-ok images of", len(inst["images"]))

img_zip = zipfile.ZipFile(next((ROOT / "coco_img").glob("val2017.zip")))
sess = faceheur.load(YUNET)
out_dir = ROOT / "set" / "img"; out_dir.mkdir(parents=True, exist_ok=True)

def kept_faces(im):
    w, h = im.size; long = max(w, h)
    res = []
    for (x, y, fw, fh, s) in faceheur.detect(sess, im):
        if min(fw * w, fh * h) / long * REF >= FACE_MIN_PX:
            res.append([x, y, fw, fh, s])
    return res

rows = {"faceless": [], "mixed": [], "negative": []}
ids = sorted(imgs)
random.shuffle(ids)
for iid in ids:
    info = imgs[iid]; anns = by_img.get(iid, [])
    persons = [a for a in anns if a["category_id"] == 1]
    kind_hint = None
    if not persons:
        # negatives: half animals / lookalikes, half anything else
        has_animal = any(a["category_id"] in ANIMALS for a in anns)
        quota = CAPS["negative"] // 2
        if sum(1 for r in rows["negative"] if r["animal"] == has_animal) >= quota: continue
        kind_hint = "negative"
    else:
        if len(rows["faceless"]) >= CAPS["faceless"] and len(rows["mixed"]) >= CAPS["mixed"]: continue
    with img_zip.open("val2017/" + info["file_name"]) as f:
        im = Image.open(f).convert("RGB")
    w, h = im.size
    faces = kept_faces(im)
    targets = []
    for a in persons:
        if a["iscrowd"]: continue
        x, y, bw, bh = a["bbox"]
        if bh / h < MIN_H: continue
        has_face = any(x <= (fx + fw / 2) * w <= x + bw and y <= (fy + fh / 2) * h <= y + bh / 2 for fx, fy, fw, fh, _ in faces)
        targets.append({"bbox": [x, y, bw, bh], "hrel": bh / h, "seg": a["segmentation"], "faceless": not has_face, "ann_id": a["id"]})
    crowd = [a["bbox"] for a in persons if a["iscrowd"]]
    small = [a["bbox"] for a in persons if not a["iscrowd"] and a["bbox"][3] / h < MIN_H]       # real people too small to ask for: ignored
    if kind_hint == "negative":
        rows["negative"].append({"id": iid, "file": info["file_name"], "w": w, "h": h, "kind": "negative", "faces": faces, "persons": [], "crowd": [], "small": [],
                                 "animal": any(a["category_id"] in ANIMALS for a in anns), "url": info["flickr_url"], "license": info["license"]})
        continue
    faceless_t = [t for t in targets if t["faceless"]]
    if not faceless_t: continue
    kind = "faceless" if not faces else "mixed"
    if len(rows[kind]) >= CAPS[kind]: continue
    rows[kind].append({"id": iid, "file": info["file_name"], "w": w, "h": h, "kind": kind, "faces": faces, "persons": targets, "crowd": crowd, "small": small,
                       "url": info["flickr_url"], "license": info["license"]})
    if all(len(rows[k]) >= CAPS[k] for k in CAPS): break

allrows = sum(rows.values(), [])
for r in allrows:
    with img_zip.open("val2017/" + r["file"]) as src, open(out_dir / r["file"], "wb") as dst:
        shutil.copyfileobj(src, dst)
json.dump(allrows, open(ROOT / "set" / "set.json", "w"))
for k, v in rows.items():
    n_t = sum(1 for r in v for t in r["persons"] if t["faceless"])
    print(f"{k}: {len(v)} images, {n_t} faceless target persons")

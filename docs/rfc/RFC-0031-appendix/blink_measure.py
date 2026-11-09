"""RFC-0031 slice E measurement tooling. For every face YuNet finds in the given photographs, score the eyes two ways:
 (a) BlinkCNN (64 px eye crop, closed-eye probability, operating threshold 0.53),
 (b) MediaPipe Face Mesh V2 -> Blendshapes (eyeBlinkLeft / eyeBlinkRight, 0..1),
and write out/faces.json (one record per face: size, scores, time), out/sheet_NN.png (face crop + the two eye crops + scores,
numbered) and out/labels.csv (a template: id, left, right with o / c / h / x blank for a person to fill in).
Usage: blink_measure.py <photo_or_dir> [...]   (env: YUNET=<path>, CROP_K=<eye crop side as a multiple of the inter-ocular distance>)"""
import os, sys, json, math, time, glob, csv
import numpy as np, cv2, onnxruntime as ort
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
YUNET = os.environ.get("YUNET") or glob.glob(os.path.expanduser("~/Library/Application Support/dev.alfredwei.emulsion/face_models/face_detection_yunet_2023mar.onnx"))[0]
CROP_K = float(os.environ.get("CROP_K", "1.0"))      # BlinkCNN crop side = 2.0 x eye span; span estimated as 0.5 x inter-ocular distance
REF_LONG = 1600                                        # YuNet runs on the photograph scaled to this long edge, as the app's face scan does
SUBSET = [0,1,4,5,6,7,8,10,13,14,17,21,33,37,39,40,46,52,53,54,55,58,61,63,65,66,67,70,78,80,81,82,84,87,88,91,93,95,103,105,107,109,127,132,133,136,144,145,146,148,149,150,152,153,154,155,157,158,159,160,161,162,163,168,172,173,176,178,181,185,191,195,197,234,246,249,251,263,267,269,270,276,282,283,284,285,288,291,293,295,296,297,300,308,310,311,312,314,317,318,321,323,324,332,334,336,338,356,361,362,365,373,374,375,377,378,379,380,381,382,384,385,386,387,388,389,390,397,398,400,402,405,409,415,454,466,468,469,470,471,472,473,474,475,476,477]
BLINK_L, BLINK_R = 9, 10      # eyeBlinkLeft, eyeBlinkRight in MediaPipe's canonical 52 (checked against the name list in the converter)
# ASSUMPTION, not verified: ARKit's Left / Right are the subject's own, so eyeBlinkLeft = the eye on the IMAGE RIGHT.
sess = lambda f: ort.InferenceSession(os.path.join(HERE, "models", f), providers=["CPUExecutionProvider"])
cnn, lmk, bsh = sess("blinkcnn.onnx"), sess("face_landmarks.onnx"), sess("face_blendshapes.onnx")
sigmoid = lambda x: 1 / (1 + math.exp(-x))

def rot_crop(img, cx, cy, w, h, theta, size):
    """size x size crop of the w x h rectangle centred (cx, cy) rotated by theta (radians, image coords), bilinear, zero border."""
    c, s = math.cos(theta), math.sin(theta)
    M = np.array([[c * w / size, -s * h / size, cx - 0.5 * (c * w - s * h)], [s * w / size, c * h / size, cy - 0.5 * (s * w + c * h)]], np.float32)
    return cv2.warpAffine(img, M, (size, size), flags=cv2.INTER_LINEAR | cv2.WARP_INVERSE_MAP, borderMode=cv2.BORDER_CONSTANT, borderValue=0)

def faces_in(path):
    full = cv2.cvtColor(cv2.imread(path, cv2.IMREAD_COLOR), cv2.COLOR_BGR2RGB); H, W = full.shape[:2]
    sc = min(1.0, REF_LONG / max(H, W)); small = cv2.resize(full, (round(W * sc), round(H * sc)), interpolation=cv2.INTER_AREA) if sc < 1 else full
    det = cv2.FaceDetectorYN.create(YUNET, "", (small.shape[1], small.shape[0]), 0.6, 0.3, 5000)
    _, f = det.detect(cv2.cvtColor(small, cv2.COLOR_RGB2BGR))
    return full, ([] if f is None else [np.concatenate([r[:14] / sc, r[14:]]) for r in f])

def score_face(full, r):
    x, y, w, h = r[:4]; re_, le = r[4:6], r[6:8]       # YuNet's "right eye" is the person's right = image left
    iod = float(np.hypot(*(le - re_))); ang = math.atan2(le[1] - re_[1], le[0] - re_[0])
    t0 = time.time(); out = {"face_w_px": float(w), "iod_px": iod, "yunet": float(r[14])}
    # (a) BlinkCNN: a rotated square on each eye centre, upright by the eye line
    side = CROP_K * iod; crops = {}
    for name, c in (("image_left", re_), ("image_right", le)):
        crop = rot_crop(full, c[0], c[1], side, side, ang, 64); crops[name] = crop
        p = cnn.run(None, {"crops": (crop.astype(np.float32) / 255)[None, None].transpose(0, 1, 4, 2, 3).copy()})[0].reshape(-1)[0]
        out["cnn_closed_" + name] = sigmoid(float(p))   # the graph returns a logit; the operating threshold 0.53 is on the probability
    t1 = time.time()
    # (b) Face Mesh V2 on the rotated 1.5x face square, then Blendshapes
    box_side = max(w, h) * 1.5; cx, cy = x + w / 2, y + h / 2
    face = rot_crop(full, cx, cy, box_side, box_side, ang, 256)
    o = lmk.run(None, {lmk.get_inputs()[0].name: (face.astype(np.float32) / 255)[None]})
    pts = o[0].reshape(-1, 3) / 256; pres = sigmoid(float(o[1].reshape(-1)[0]))
    c, s = math.cos(ang), math.sin(ang); dx, dy = (pts[:, 0] - 0.5) * box_side, (pts[:, 1] - 0.5) * box_side
    px = np.stack([cx + dx * c - dy * s, cy + dx * s + dy * c], 1).astype(np.float32)
    bs = bsh.run(None, {bsh.get_inputs()[0].name: px[SUBSET][None]})[0].reshape(-1)
    out.update(mesh_presence=pres, blink_image_left=float(bs[BLINK_R]), blink_image_right=float(bs[BLINK_L]), ms_cnn=(t1 - t0) * 1000, ms_mesh=(time.time() - t1) * 1000)
    return out, crops, face, px

if __name__ == "__main__":
    files = []
    for a in sys.argv[1:]: files += sorted(glob.glob(os.path.join(a, "*.jp*g")) + glob.glob(os.path.join(a, "*.JPG"))) if os.path.isdir(a) else [a]
    os.makedirs(os.path.join(HERE, "out"), exist_ok=True); recs, tiles = [], []
    for f in files:
        full, fs = faces_in(f)
        for k, r in enumerate(fs):
            rec, crops, face, px = score_face(full, r); rec.update(id=len(recs) + 1, photo=os.path.basename(f), n=k); recs.append(rec)
            tile = Image.new("RGB", (256 + 2 * 64 + 8, 256), (24, 22, 20)); tile.paste(Image.fromarray(face), (0, 0))
            tile.paste(Image.fromarray(crops["image_left"]).resize((128, 128), Image.NEAREST), (260, 0)); tile.paste(Image.fromarray(crops["image_right"]).resize((128, 128), Image.NEAREST), (260, 130))
            d = ImageDraw.Draw(tile); d.text((4, 4), f"#{rec['id']}  {rec['face_w_px']:.0f}px", fill=(255, 220, 120))
            d.text((262, 112), f"L cnn {rec['cnn_closed_image_left']:.2f} bs {rec['blink_image_left']:.2f}", fill=(255, 255, 255)); d.text((262, 242), f"R cnn {rec['cnn_closed_image_right']:.2f} bs {rec['blink_image_right']:.2f}", fill=(255, 255, 255))
            tiles.append(tile)
    json.dump(recs, open(os.path.join(HERE, "out", "faces.json"), "w"), indent=1)
    with open(os.path.join(HERE, "out", "labels.csv"), "w", newline="") as fh:
        w = csv.writer(fh); w.writerow(["id", "photo", "face_w_px", "image_left", "image_right"]); [w.writerow([r["id"], r["photo"], round(r["face_w_px"]), "", ""]) for r in recs]
    per = 12; cols = 3
    for i in range(0, len(tiles), per):
        chunk = tiles[i:i + per]; rows = math.ceil(len(chunk) / cols); sheet = Image.new("RGB", (cols * 392, rows * 260), (24, 22, 20))
        for j, t in enumerate(chunk): sheet.paste(t, ((j % cols) * 392, (j // cols) * 260))
        sheet.save(os.path.join(HERE, "out", f"sheet_{i // per + 1:02d}.png"))
    print(len(recs), "faces from", len(files), "photographs")

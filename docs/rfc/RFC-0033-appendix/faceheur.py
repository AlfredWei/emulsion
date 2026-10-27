"""RFC-0033 slice A: a numpy port of the app's YuNet face detector (face_detect.rs: 640x640 squashed input, BGR, strides 8/16/32,
score 0.6, NMS 0.3), so face / person resolvers can be looked at on photographs outside the app. Needs onnxruntime + numpy + PIL
(the mask_training venv has them). Usage: faceheur.py <yunet.onnx> <image>... -> one JSON line per image (normalised boxes)."""
import json, sys, numpy as np, onnxruntime as ort
from PIL import Image
S, STRIDES, THR, NMS = 640, (8, 16, 32), 0.6, 0.3
def load(path): return ort.InferenceSession(path, providers=["CPUExecutionProvider"])
def iou(a, b):
    x1, y1 = max(a[0], b[0]), max(a[1], b[1]); x2, y2 = min(a[0] + a[2], b[0] + b[2]), min(a[1] + a[3], b[1] + b[3])
    i = max(0, x2 - x1) * max(0, y2 - y1); u = a[2] * a[3] + b[2] * b[3] - i
    return i / u if u > 0 else 0
def detect(sess, im):
    a = np.asarray(im.convert("RGB").resize((S, S), Image.BILINEAR), np.float32)[..., ::-1].transpose(2, 0, 1)[None].copy()
    out = sess.run(None, {sess.get_inputs()[0].name: a}); cand = []
    for i, st in enumerate(STRIDES):
        n = S // st; cls, obj, bb = out[i][0, :, 0], out[i + 3][0, :, 0], out[i + 6][0]
        sc = np.sqrt(np.clip(cls, 0, 1) * np.clip(obj, 0, 1))
        for idx in np.nonzero(sc >= THR)[0]:
            r, c = divmod(int(idx), n); cx = (c + bb[idx, 0]) * st; cy = (r + bb[idx, 1]) * st
            w = np.exp(bb[idx, 2]) * st; h = np.exp(bb[idx, 3]) * st
            cand.append(tuple(float(v) for v in ((cx - w / 2) / S, (cy - h / 2) / S, w / S, h / S, sc[idx])))
    cand.sort(key=lambda f: -f[4]); keep = []
    for f in cand:
        if all(iou(f, k) <= NMS for k in keep): keep.append(f)
    return keep
if __name__ == "__main__":
    s = load(sys.argv[1])
    for p in sys.argv[2:]: print(json.dumps({"file": p, "faces": detect(s, Image.open(p))}))

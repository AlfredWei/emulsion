"""Candidate person detectors behind one interface: make(name) -> fn(PIL.Image) -> list[(x0, y0, x1, y1, score)] in image pixels,
person class only, low score floor (0.05) so thresholds can be swept afterwards."""
import os, pathlib
ROOT = pathlib.Path(__file__).resolve().parent.parent
os.environ.setdefault("TORCH_HOME", str(ROOT / "models" / "torchvision"))
os.environ.setdefault("HF_HOME", str(ROOT / "models" / "hf_cache"))
import numpy as np, torch
from PIL import Image

FLOOR = 0.05
DEVICE = "mps" if torch.backends.mps.is_available() else "cpu"

def yolox(name, size):
    import onnxruntime as ort
    from torchvision.ops import nms
    sess = ort.InferenceSession(str(ROOT / "models" / "yolox" / f"{name}.onnx"), providers=["CPUExecutionProvider"])
    grids, strides = [], []
    for s in (8, 16, 32):
        n = size // s
        xv, yv = np.meshgrid(np.arange(n), np.arange(n))
        grids.append(np.stack((xv, yv), 2).reshape(-1, 2)); strides.append(np.full((n * n, 1), s))
    grids, strides = np.concatenate(grids), np.concatenate(strides)
    def run(im):
        w, h = im.size; r = min(size / h, size / w)
        nw, nh = int(w * r), int(h * r)
        canvas = np.full((size, size, 3), 114, np.uint8)
        canvas[:nh, :nw] = np.asarray(im.convert("RGB").resize((nw, nh), Image.BILINEAR))[..., ::-1]    # BGR, as YOLOX trains
        x = canvas.transpose(2, 0, 1)[None].astype(np.float32)
        o = sess.run(None, {sess.get_inputs()[0].name: x})[0][0]
        xy = (o[:, :2] + grids) * strides; wh = np.exp(o[:, 2:4]) * strides
        score = o[:, 4] * o[:, 5]                       # objectness x person class
        keep = score >= FLOOR
        if not keep.any(): return []
        b = np.concatenate([xy[keep] - wh[keep] / 2, xy[keep] + wh[keep] / 2], 1) / r
        s = score[keep]
        k = nms(torch.from_numpy(b.astype(np.float32)), torch.from_numpy(s.astype(np.float32)), 0.45).numpy()
        return [(float(b[i, 0]), float(b[i, 1]), float(b[i, 2]), float(b[i, 3]), float(s[i])) for i in k]
    return run

def torchvision_det(ctor_name, weights="COCO_V1"):
    from torchvision.models import detection as D
    kwargs = {"box_score_thresh": FLOOR} if "rcnn" in ctor_name else {}
    if "ssdlite" in ctor_name: kwargs = {"score_thresh": FLOOR}
    if "retinanet" in ctor_name or "fcos" in ctor_name: kwargs = {"score_thresh": FLOOR}
    m = getattr(D, ctor_name)(weights=weights, **kwargs).eval().to(DEVICE)
    from torchvision.transforms.functional import to_tensor
    @torch.no_grad()
    def run(im):
        o = m([to_tensor(im.convert("RGB")).to(DEVICE)])[0]
        sel = o["labels"] == 1                                 # COCO person
        b, s = o["boxes"][sel].cpu().numpy(), o["scores"][sel].cpu().numpy()
        return [(float(x[0]), float(x[1]), float(x[2]), float(x[3]), float(c)) for x, c in zip(b, s)]
    return run

def hf_det(repo):
    from transformers import AutoImageProcessor, AutoModelForObjectDetection
    path = ROOT / "models" / repo.replace("/", "__")
    proc = AutoImageProcessor.from_pretrained(path)
    m = AutoModelForObjectDetection.from_pretrained(path).eval().to(DEVICE)
    person = [i for i, n in m.config.id2label.items() if n == "person"][0]
    @torch.no_grad()
    def run(im):
        im = im.convert("RGB")
        x = proc(images=im, return_tensors="pt").to(DEVICE)
        o = m(**x)
        r = proc.post_process_object_detection(o, threshold=FLOOR, target_sizes=[im.size[::-1]])[0]
        sel = r["labels"] == int(person)
        b, s = r["boxes"][sel].cpu().numpy(), r["scores"][sel].cpu().numpy()
        return [(float(x[0]), float(x[1]), float(x[2]), float(x[3]), float(c)) for x, c in zip(b, s)]
    return run

REGISTRY = {
    "yolox_nano": lambda: yolox("yolox_nano", 416),
    "yolox_tiny": lambda: yolox("yolox_tiny", 416),
    "yolox_s": lambda: yolox("yolox_s", 640),
    "yolox_m": lambda: yolox("yolox_m", 640),
    "yolox_l": lambda: yolox("yolox_l", 640),
    "ssdlite320_mobilenet_v3": lambda: torchvision_det("ssdlite320_mobilenet_v3_large"),
    "fasterrcnn_mobilenet_v3_large_fpn": lambda: torchvision_det("fasterrcnn_mobilenet_v3_large_fpn"),
    "retinanet_resnet50_fpn_v2": lambda: torchvision_det("retinanet_resnet50_fpn_v2"),
    "fcos_resnet50_fpn": lambda: torchvision_det("fcos_resnet50_fpn"),
    "fasterrcnn_resnet50_fpn_v2": lambda: torchvision_det("fasterrcnn_resnet50_fpn_v2"),
    "maskrcnn_resnet50_fpn_v2": lambda: torchvision_det("maskrcnn_resnet50_fpn_v2"),
}
for rid in ["PekingU/rtdetr_r18vd", "PekingU/rtdetr_r34vd", "PekingU/rtdetr_r50vd", "PekingU/rtdetr_v2_r18vd", "PekingU/rtdetr_v2_r50vd",
            "facebook/detr-resnet-50", "hustvl/yolos-tiny", "hustvl/yolos-small", "ustc-community/dfine-small-coco", "ustc-community/dfine-medium-coco"]:
    REGISTRY[rid.split("/")[1]] = (lambda r=rid: hf_det(r))

def make(name):
    return REGISTRY[name]()

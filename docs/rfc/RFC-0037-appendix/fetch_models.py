"""Fetch the candidate person detectors that come from libraries (the YOLOX ONNX files were curl-ed separately).
Each model lands in its own folder under ../models/. Prints the size on disk so the 200 MB limit can be checked."""
import os, sys, pathlib
ROOT = pathlib.Path(__file__).resolve().parent.parent / "models"
os.environ["TORCH_HOME"] = str(ROOT / "torchvision")
os.environ["HF_HOME"] = str(ROOT / "hf_cache")
import torch, torchvision
from torchvision.models import detection as D
from huggingface_hub import snapshot_download

def mb(p):
    p = pathlib.Path(p)
    return sum(f.stat().st_size for f in p.rglob("*") if f.is_file()) / 1e6

tv = {
    "ssdlite320_mobilenet_v3": (D.ssdlite320_mobilenet_v3_large, "COCO_V1"),
    "fasterrcnn_mobilenet_v3_large_fpn": (D.fasterrcnn_mobilenet_v3_large_fpn, "COCO_V1"),
    "retinanet_resnet50_fpn_v2": (D.retinanet_resnet50_fpn_v2, "COCO_V1"),
    "fcos_resnet50_fpn": (D.fcos_resnet50_fpn, "COCO_V1"),
    "fasterrcnn_resnet50_fpn_v2": (D.fasterrcnn_resnet50_fpn_v2, "COCO_V1"),
    "maskrcnn_resnet50_fpn_v2": (D.maskrcnn_resnet50_fpn_v2, "COCO_V1"),
}
for name, (fn, w) in tv.items():
    try:
        m = fn(weights=w)
        n = sum(p.numel() for p in m.parameters())
        print(f"torchvision {name}: {n/1e6:.1f} M params, {n*4/1e6:.0f} MB fp32", flush=True)
    except Exception as e:
        print("FAIL", name, e, flush=True)

hf = ["PekingU/rtdetr_r18vd", "PekingU/rtdetr_r34vd", "PekingU/rtdetr_r50vd", "PekingU/rtdetr_v2_r18vd", "PekingU/rtdetr_v2_r50vd",
      "facebook/detr-resnet-50", "hustvl/yolos-tiny", "hustvl/yolos-small", "ustc-community/dfine-small-coco", "ustc-community/dfine-medium-coco"]
for rid in hf:
    try:
        d = snapshot_download(rid, local_dir=str(ROOT / rid.replace("/", "__")), allow_patterns=["*.json", "*.safetensors", "*.txt"])
        print(f"hf {rid}: {mb(d):.0f} MB on disk", flush=True)
    except Exception as e:
        print("FAIL", rid, str(e)[:120], flush=True)

"""Export a Hugging Face detector to ONNX (fixed 640x640 squashed input) and compare with torch + time ORT CPU. Usage: export_onnx.py REPO NAME"""
import os, sys, time, pathlib
ROOT = pathlib.Path(__file__).resolve().parent.parent
os.environ["HF_HOME"] = str(ROOT / "models" / "hf_cache")
import numpy as np, torch, onnxruntime as ort
from transformers import AutoModelForObjectDetection
repo, name = sys.argv[1:3]
m = AutoModelForObjectDetection.from_pretrained(ROOT / "models" / repo.replace("/", "__")).eval()
class W(torch.nn.Module):
    def __init__(s, m): super().__init__(); s.m = m
    def forward(s, x):
        o = s.m(pixel_values=x); return o.logits, o.pred_boxes
x = torch.rand(1, 3, 640, 640)
out = ROOT / "onnx" / f"{name}.onnx"
torch.onnx.export(W(m), (x,), str(out), input_names=["pixel_values"], output_names=["logits", "pred_boxes"], opset_version=17, dynamo=False)
print("exported", out.stat().st_size / 1e6, "MB")
# The helper's ONNX Runtime (ort 2.0.0-rc.13) has no CPU kernel for Cos, which the positional embedding of a fixed 640 x 640 grid needs;
# the ORT basic graph optimisation folds those constants (and only standard ai.onnx ops are left), 3431 -> 1085 nodes.
so = ort.SessionOptions(); so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_BASIC
folded = ROOT / "onnx" / f"{name}_folded.onnx"; so.optimized_model_filepath = str(folded)
ort.InferenceSession(str(out), so, providers=["CPUExecutionProvider"])
print("folded", folded.stat().st_size, "bytes; this is the file the app ships")
out = folded
sess = ort.InferenceSession(str(out), providers=["CPUExecutionProvider"])
with torch.no_grad(): ref = W(m)(x)
got = sess.run(None, {"pixel_values": x.numpy()})
print("max |diff| logits", float(np.abs(got[0] - ref[0].numpy()).max()), "boxes", float(np.abs(got[1] - ref[1].numpy()).max()))
t = []
for _ in range(8):
    s = time.time(); sess.run(None, {"pixel_values": x.numpy()}); t.append(time.time() - s)
print("ORT CPU median ms", sorted(t)[len(t)//2] * 1000)

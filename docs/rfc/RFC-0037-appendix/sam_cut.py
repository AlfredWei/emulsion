"""SAM 2 Tiny exactly as the app's AI helper drives it (emulsion-ai/segment.rs): image squashed to 1024x1024 (bilinear/triangle),
ImageNet mean/std, encoder outputs [high_res_0, high_res_1, image_embed]; decoder gets point prompts in 1024 px plus one (0,0,-1)
padding point and returns 256x256 logits per candidate with a predicted IoU. Usage: s = Sam(); s.prepare(PIL); s.cut([(x, y, True)]) with
x, y normalised to the frame -> (pred_iou, logits 256x256 of the best candidate)."""
import numpy as np, onnxruntime as ort
from PIL import Image
DIR = "/Users/weishih-yi/Library/Application Support/dev.alfredwei.emulsion/segment_models/"
MEAN = np.array([0.485, 0.456, 0.406], np.float32); STD = np.array([0.229, 0.224, 0.225], np.float32)

class Sam:
    def __init__(self):
        self.enc = ort.InferenceSession(DIR + "sam2_hiera_tiny.encoder.onnx", providers=["CPUExecutionProvider"])
        self.dec = ort.InferenceSession(DIR + "sam2_hiera_tiny.decoder.onnx", providers=["CPUExecutionProvider"])
        self.emb = None
    def prepare(self, im):
        x = np.asarray(im.convert("RGB").resize((1024, 1024), Image.BILINEAR), np.float32) / 255.0
        x = ((x - MEAN) / STD).transpose(2, 0, 1)[None].astype(np.float32)
        o = self.enc.run(None, {"image": x})
        self.emb = {"high_res_feats_0": o[0], "high_res_feats_1": o[1], "image_embed": o[2]}
    def cut(self, clicks, pad=True):
        pts = [[min(max(x, 0), 1) * 1024, min(max(y, 0), 1) * 1024] for x, y, _ in clicks] + ([[0.0, 0.0]] if pad else [])
        lab = [(1.0 if p else 0.0) if isinstance(p, bool) else float(p) for *_, p in clicks] + ([-1.0] if pad else [])     # ints: SAM 2 labels, 2/3 = box corners
        o = self.dec.run(None, {**self.emb, "point_coords": np.array([pts], np.float32), "point_labels": np.array([lab], np.float32),
                                "mask_input": np.zeros((1, 1, 256, 256), np.float32), "has_mask_input": np.zeros(1, np.float32)})
        k = int(np.argmax(o[1][0]))
        return float(o[1][0][k]), o[0][0][k]

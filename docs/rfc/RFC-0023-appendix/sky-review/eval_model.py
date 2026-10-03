"""Score a checkpoint against the owner-reviewed reference masks (skyseg's mask on the 36 photos where it was not rejected). usage: eval_model.py ckpt.pt"""
import json, sys, numpy as np, torch
from PIL import Image
from train import UNet
k = json.load(open("review_key.json")); a = json.load(open("review_answers.json")); meta = json.load(open("gt_photos/meta.json"))
net = UNet(); net.load_state_dict(torch.load(sys.argv[1], map_location="cpu")); net.eval()
ious, wrong = [], []
for i, mt in enumerate(meta, 1):
    m = k[str(i)]; ok = {"A": [m["A"]], "B": [m["B"]], "both": ["ours", "skyseg"], "none": []}[a[str(i)]]
    if "skyseg" not in ok: continue
    im = Image.open("gt_photos/" + mt["file"]).convert("RGB"); W0, H0 = im.size; s = 900 / max(W0, H0); im = im.resize((int(W0 * s), int(H0 * s))); W, H = im.size
    x = torch.from_numpy(np.asarray(im.resize((288, 288), Image.BILINEAR), np.float32) / 255).permute(2, 0, 1)[None]
    with torch.no_grad(): p = torch.sigmoid(net(x))[0, 0].numpy()
    o = np.asarray(Image.fromarray(p).resize((W, H), Image.BILINEAR)) > 0.5; gt = np.load(f"masks/{i:02d}.npz")["skyseg"]
    ious.append((o & gt).sum() / max((o | gt).sum(), 1)); wrong.append((o ^ gt).mean() * 100)
w = np.array(wrong)
print(f"{sys.argv[1]}: {len(w)} photos  mean IoU {np.mean(ious):.3f}  differs <1%: {(w<1).sum()}  1-5%: {((w>=1)&(w<=5)).sum()}  >5%: {(w>5).sum()}")

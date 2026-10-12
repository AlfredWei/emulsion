"""RFC-0029 slice 4a: super-resolution quality of several 4x ONNX models on a known reference, two degradations.
  ideal:     LR = clean / 4 (bicubic)                      (the RFC-0023 §4.7 protocol)
  realistic: LR = gaussian blur 0.8 -> clean / 4 -> JPEG quality 50   (a soft, compressed source: web / old file)
Per photo and model: PSNR / SSIM vs the clean reference, plus HF = std(img - blur2) / std(clean - blur2) on luma (1.0 = same
fine-texture energy as the reference; above 1 means sharpened or invented texture), plus Strength blends with a Lanczos enlargement.
usage: quality.py ONNX_DIR PHOTOS_DIR OUT_DIR MODEL[,MODEL...] photo.jpg [photo.jpg ...]   (MODEL = onnx file stem)"""
import sys, os, io, json, time, numpy as np, onnxruntime as ort
from PIL import Image, ImageFilter

ONNX, PH, OUT, MODELS = sys.argv[1:5]; files = sys.argv[5:]; MODELS = MODELS.split(","); os.makedirs(OUT, exist_ok=True)
TILE = {"swinir_gan": 64, "swinir_psnr": 64}  # fixed-size exports are run tiled (LR px), the others whole
sess = {m: ort.InferenceSession(f"{ONNX}/{m}.onnx", providers=["CPUExecutionProvider"]) for m in MODELS}

def run_whole(s, a):  # a: float HxWx3 0..1
    return np.clip(s.run(None, {"image": a.transpose(2, 0, 1)[None].astype(np.float32)})[0][0].transpose(1, 2, 0), 0, 1)

def run_tiled(s, a, tile, ov=8, k=4):  # LR tiles with overlap, linear feather on the HR grid
    H, W = a.shape[:2]; step = tile - 2 * ov
    ap = np.pad(a, ((ov, ov + tile), (ov, ov + tile), (0, 0)), mode="reflect")
    acc = np.zeros((H * k, W * k, 3)); wsum = np.zeros((H * k, W * k, 1))
    ramp = np.minimum(np.arange(tile * k) + 1, np.arange(tile * k)[::-1] + 1).astype(np.float64); ramp = np.minimum(ramp / (ov * k * 2), 1.0)
    wt = (ramp[:, None] * ramp[None, :])[:, :, None]
    for y in range(0, H, step):
        for x in range(0, W, step):
            t = run_whole(s, ap[y:y + tile, x:x + tile]); hy, hx = min(tile * k, H * k - (y - ov) * k if False else tile * k), tile * k
            y0, x0 = (y - ov) * k, (x - ov) * k
            ys, xs = max(0, -y0), max(0, -x0); ye, xe = min(tile * k, H * k - y0), min(tile * k, W * k - x0)
            acc[y0 + ys:y0 + ye, x0 + xs:x0 + xe] += (t * wt)[ys:ye, xs:xe]; wsum[y0 + ys:y0 + ye, x0 + xs:x0 + xe] += wt[ys:ye, xs:xe]
    return acc / np.maximum(wsum, 1e-8)

def upscale(m, lr):
    a = np.asarray(lr, np.float32) / 255; t0 = time.time()
    y = run_tiled(sess[m], a, TILE[m]) if m in TILE else run_whole(sess[m], a)
    return Image.fromarray((np.clip(y, 0, 1) * 255 + 0.5).astype(np.uint8)), time.time() - t0

def psnr(a, b): return float(10 * np.log10(255 ** 2 / np.mean((np.asarray(a, np.float64) - np.asarray(b, np.float64)) ** 2)))
def box(x, r=3):
    k = 2 * r + 1; p = np.pad(x, ((r + 1, r), (r + 1, r)), mode="edge"); c = p.cumsum(0).cumsum(1); H, W = x.shape
    return (c[k:k + H, k:k + W] - c[:H, k:k + W] - c[k:k + H, :W] + c[:H, :W]) / (k * k)
def ssim(a, b):
    a = np.asarray(Image.fromarray(np.asarray(a)).convert("L"), np.float64); b = np.asarray(Image.fromarray(np.asarray(b)).convert("L"), np.float64)
    ma, mb = box(a), box(b); va = box(a * a) - ma * ma; vb = box(b * b) - mb * mb; cov = box(a * b) - ma * mb; c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    return float(np.mean(((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))))
def hf(a): g = Image.fromarray(np.asarray(a)).convert("L"); return float(np.std(np.asarray(g, np.float64) - np.asarray(g.filter(ImageFilter.GaussianBlur(2)), np.float64)))
def blend(base, model, s): return Image.fromarray(np.clip(np.asarray(base, np.float64) + (np.asarray(model, np.float64) - np.asarray(base, np.float64)) * s / 100 + 0.5, 0, 255).astype(np.uint8))

STR = [0, 30, 50, 70, 100]; res = {"ideal": {}, "realistic": {}}; sheets = {"ideal": [], "realistic": []}
for f in files:
    g = Image.open(f"{PH}/{f}").convert("RGB"); s = 1000 / max(g.size); g = g.resize((round(g.width * s), round(g.height * s)), Image.LANCZOS)
    g = g.crop((0, 0, g.width // 4 * 4, g.height // 4 * 4)); href = hf(g)
    for deg in ("ideal", "realistic"):
        src = g.filter(ImageFilter.GaussianBlur(0.8)) if deg == "realistic" else g
        lr = src.resize((g.width // 4, g.height // 4), Image.BICUBIC)
        if deg == "realistic": b = io.BytesIO(); lr.save(b, "JPEG", quality=50); lr = Image.open(io.BytesIO(b.getvalue())).convert("RGB")
        lan = lr.resize(g.size, Image.LANCZOS); row = {"clean": g, "lanczos": lan}
        r = {"lanczos": {"psnr": psnr(lan, g), "ssim": ssim(lan, g), "hf": hf(lan) / href}}
        for m in MODELS:
            up, dt = upscale(m, lr); row[m] = up
            r[m] = {"psnr": psnr(up, g), "ssim": ssim(up, g), "hf": hf(up) / href, "s": dt,
                    "blend": {str(x): [psnr(blend(lan, up, x), g), ssim(blend(lan, up, x), g), hf(blend(lan, up, x)) / href] for x in STR}}
            print(deg, f[:22], m, {k: round(v, 3) for k, v in r[m].items() if k != "blend"}, flush=True)
        res[deg][f] = r; sheets[deg].append((f, row))
json.dump(res, open(f"{OUT}/results.json", "w"), indent=1)
for deg in res:
    ks = ["lanczos"] + MODELS
    print("MEAN", deg, {k: tuple(round(float(np.mean([res[deg][f][k][q] for f in files])), 3) for q in ("psnr", "ssim", "hf")) for k in ks})
    for m in MODELS: print("  blend", deg, m, {x: tuple(round(float(np.mean([res[deg][f][m]["blend"][x][i] for f in files])), 3) for i in range(3)) for x in map(str, STR)})
for deg in sheets:  # crop sheets: 100x100 HR-px centre crop (shown 3x) of clean | lanczos | each model
    for f, row in sheets[deg][:8]:
        w, h = row["clean"].size; bx = (w // 2 - 80, h // 2 - 80, w // 2 + 80, h // 2 + 80); names = ["clean", "lanczos"] + MODELS
        tiles = [row[k].crop(bx).resize((320, 320), Image.NEAREST) for k in names]; sh = Image.new("RGB", (320 * len(tiles), 320))
        [sh.paste(t, (i * 320, 0)) for i, t in enumerate(tiles)]; sh.save(f"{OUT}/crop_{deg}_{os.path.splitext(f)[0][:24]}.jpg", quality=88)

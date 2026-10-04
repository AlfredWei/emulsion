"""M6 slice 0: denoise (SCUNet real_psnr) and super-resolution (Real-ESRGAN general-x4v3) quality on CC0 photos with a known clean reference.
Denoise: clean = photo downscaled to 1000 px long side; synthetic heteroscedastic noise (variance = a*x + b, x = clean value 0..1) at 3 levels.
Super-resolution: clean = same; LR = clean / 4 (bicubic); compare Real-ESRGAN x4 vs bicubic x4 against clean. PSNR/SSIM only; see crops for perceptual judgement.
usage: quality_denoise_sr.py MODELS_DIR PHOTOS_DIR OUT_DIR [photo files...]"""
import sys, os, json, time, numpy as np, onnxruntime as ort
from PIL import Image, ImageFilter
M, PH, OUT = sys.argv[1:4]; files = sys.argv[4:]; os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(0)
sc = ort.InferenceSession(f"{M}/scunet_color_real_psnr.onnx", providers=["CPUExecutionProvider"])
sr = ort.InferenceSession(f"{M}/realesr-general-x4v3.onnx", providers=["CPUExecutionProvider"])
def run(sess, im, mult=1):  # PIL RGB -> PIL RGB, float 0..1 NCHW; mult: reflect-pad to a multiple (SCUNet's ONNX needs 64)
    a = np.asarray(im, np.float32) / 255; H, W = a.shape[:2]; ph, pw = (-H) % mult, (-W) % mult
    if ph or pw: a = np.pad(a, ((0, ph), (0, pw), (0, 0)), mode="reflect")
    x = a.transpose(2, 0, 1)[None]
    y = sess.run(None, {sess.get_inputs()[0].name: x})[0][0].transpose(1, 2, 0)
    k = y.shape[0] // (H + ph); y = y[:H * k, :W * k]  # output scale k (1 for denoise, 4 for x4 SR); drop the padding
    return Image.fromarray((np.clip(y, 0, 1) * 255 + 0.5).astype(np.uint8))
def psnr(a, b): return float(10 * np.log10(255 ** 2 / np.mean((np.asarray(a, np.float64) - np.asarray(b, np.float64)) ** 2)))
def box(x, r=3):
    k = 2 * r + 1; p = np.pad(x, ((r + 1, r), (r + 1, r)), mode="edge"); c = p.cumsum(0).cumsum(1); H, W = x.shape
    return (c[k:k + H, k:k + W] - c[:H, k:k + W] - c[k:k + H, :W] + c[:H, :W]) / (k * k)
def ssim(a, b):  # mean SSIM on luma, 7x7 box window (approximation of the Gaussian-window SSIM)
    a = np.asarray(Image.fromarray(np.asarray(a)).convert("L"), np.float64); b = np.asarray(Image.fromarray(np.asarray(b)).convert("L"), np.float64)
    ma, mb = box(a), box(b); va = box(a * a) - ma * ma; vb = box(b * b) - mb * mb; cov = box(a * b) - ma * mb
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    return float(np.mean(((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))))
levels = {"low": (0.002, 0.0002), "medium": (0.006, 0.001), "high": (0.02, 0.004)}
res = {"denoise": {}, "sr": {}}; sheets = []
for f in files:
    g = Image.open(f"{PH}/{f}").convert("RGB"); s = 1000 / max(g.size); g = g.resize((round(g.width * s), round(g.height * s)), Image.LANCZOS)
    g = g.crop((0, 0, g.width // 4 * 4, g.height // 4 * 4)); ga = np.asarray(g, np.float64) / 255
    row = {"clean": g}
    for lv, (a, b) in levels.items():
        n = np.clip(ga + rng.normal(0, 1, ga.shape) * np.sqrt(a * ga + b), 0, 1); noisy = Image.fromarray((n * 255 + 0.5).astype(np.uint8))
        t = time.time(); den = run(sc, noisy, 64); dt = time.time() - t; blur = noisy.filter(ImageFilter.GaussianBlur(1.2))
        r = {"noisy": (psnr(noisy, g), ssim(noisy, g)), "scunet": (psnr(den, g), ssim(den, g)), "blur1.2": (psnr(blur, g), ssim(blur, g)), "scunet_s": dt}
        res["denoise"].setdefault(lv, {})[f] = r; row[lv] = noisy; row[lv + "_den"] = den
        print("denoise", lv, f[:28], {k: (tuple(round(x, 3) for x in v) if isinstance(v, tuple) else round(v, 1)) for k, v in r.items()}, flush=True)
    lr = g.resize((g.width // 4, g.height // 4), Image.BICUBIC); t = time.time(); up = run(sr, lr); dt = time.time() - t; bic = lr.resize(g.size, Image.BICUBIC)
    res["sr"][f] = {"bicubic": (psnr(bic, g), ssim(bic, g)), "realesrgan": (psnr(up, g), ssim(up, g)), "sr_s": dt}
    print("sr", f[:28], {k: (tuple(round(x, 3) for x in v) if isinstance(v, tuple) else round(v, 1)) for k, v in res["sr"][f].items()}, flush=True)
    row["lr_bicubic"] = bic; row["lr_esrgan"] = up; sheets.append((f, row))
json.dump(res, open(f"{OUT}/results.json", "w"), indent=1)
for lv in levels:  # means
    d = res["denoise"][lv]; print("MEAN denoise", lv, {k: tuple(round(float(np.mean([d[f][k][i] for f in d])), 3) for i in (0, 1)) for k in ("noisy", "scunet", "blur1.2")})
print("MEAN sr", {k: tuple(round(float(np.mean([res["sr"][f][k][i] for f in res["sr"]])), 3) for i in (0, 1)) for k in ("bicubic", "realesrgan")})
# crop sheets: 200x200 detail crop (centre) per image: clean | noisy(high) | scunet | blur   and   clean | bicubic | real-esrgan
for f, row in sheets:
    w, h = row["clean"].size; box_ = (w // 2 - 100, h // 2 - 100, w // 2 + 100, h // 2 + 100)
    names = [("clean", "clean"), ("high", "noisy high"), ("high_den", "scunet"), ("lr_bicubic", "bicubic x4"), ("lr_esrgan", "real-esrgan x4")]
    tiles = [row[k].crop(box_).resize((300, 300), Image.NEAREST) for k, _ in names]
    sheet = Image.new("RGB", (300 * len(tiles), 300)); [sheet.paste(t, (i * 300, 0)) for i, t in enumerate(tiles)]
    sheet.save(f"{OUT}/crop_{os.path.splitext(f)[0][:30]}.jpg", quality=88)

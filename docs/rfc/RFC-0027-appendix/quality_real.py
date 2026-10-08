"""Bake-off, real camera noise: SCUNet color_real_psnr vs NAFNet-SIDD-width32 on NIND ISO 6400 (reference: ISO 200 of the same scene).
Same protocol as lr_replace docs/rfc/RFC-0023-appendix/quality_real_noise.py (central 1280 window, phase-correlation alignment, 1024 crop, per-channel tone match),
plus: tone shift (mean |blur8(out) - blur8(noisy)| in 8-bit levels), per-channel mean shift, and high-frequency retention (std of img - blur2 vs the ISO 200 reference).
usage: quality_real.py DATA_DIR OUT_DIR scene [scene...]"""
import sys, os, glob, json, time, numpy as np, onnxruntime as ort
from PIL import Image, ImageFilter
ND, OUT = sys.argv[1:3]; scenes = sys.argv[3:]; os.makedirs(OUT, exist_ok=True)
MODELS = {"scunet": "models/scunet_color_real_psnr.onnx", "nafnet": "models/nafnet_sidd_w32.onnx"}
SESS = {k: ort.InferenceSession(v, providers=["CPUExecutionProvider"]) for k, v in MODELS.items()}
def den(k, im):
    s = SESS[k]; a = np.asarray(im, np.float32) / 255; H, W = a.shape[:2]; ph, pw = (-H) % 64, (-W) % 64
    a = np.pad(a, ((0, ph), (0, pw), (0, 0)), mode="reflect")
    y = s.run(None, {s.get_inputs()[0].name: a.transpose(2, 0, 1)[None]})[0][0].transpose(1, 2, 0)[:H, :W]
    return Image.fromarray((np.clip(y, 0, 1) * 255 + 0.5).astype(np.uint8))
def hf(im): a = np.asarray(im.convert("L"), np.float64); b = np.asarray(im.convert("L").filter(ImageFilter.GaussianBlur(2)), np.float64); return float((a - b).std())
def bl(im): return np.asarray(im.filter(ImageFilter.GaussianBlur(8)), np.float64)
def psnr(a, b): return float(10 * np.log10(255 ** 2 / np.mean((np.asarray(a, np.float64) - np.asarray(b, np.float64)) ** 2)))
def box(x, r=3):
    k = 2 * r + 1; p = np.pad(x, ((r + 1, r), (r + 1, r)), mode="edge"); c = p.cumsum(0).cumsum(1); H, W = x.shape
    return (c[k:k + H, k:k + W] - c[:H, k:k + W] - c[k:k + H, :W] + c[:H, :W]) / (k * k)
def ssim(a, b):
    a = np.asarray(Image.fromarray(np.asarray(a)).convert("L"), np.float64); b = np.asarray(Image.fromarray(np.asarray(b)).convert("L"), np.float64)
    ma, mb = box(a), box(b); va = box(a * a) - ma * ma; vb = box(b * b) - mb * mb; cov = box(a * b) - ma * mb; c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    return float(np.mean(((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))))
def shift(ref, noisy):  # integer (dy, dx) such that noisy shifted by it matches ref; phase correlation on blurred luma
    f = lambda im: np.asarray(im.convert("L").filter(ImageFilter.GaussianBlur(2)), np.float64)
    A, B = f(ref), f(noisy); A -= A.mean(); B -= B.mean()
    R = np.fft.fft2(A) * np.conj(np.fft.fft2(B)); R /= np.abs(R) + 1e-9; r = np.fft.ifft2(R).real
    dy, dx = np.unravel_index(np.argmax(r), r.shape); H, W = A.shape
    return int(dy - H if dy > H // 2 else dy), int(dx - W if dx > W // 2 else dx), float(r.max())
res = {}; rows = []
for s in scenes:
    ref0 = Image.open(glob.glob(f"{ND}/{s}_ISO200_*.jpg")[0]).convert("RGB"); no0 = Image.open(glob.glob(f"{ND}/{s}_ISO6400_*.jpg")[0]).convert("RGB")
    W, H = ref0.size; S = 1280; box_ = (W // 2 - S // 2, H // 2 - S // 2, W // 2 + S // 2, H // 2 + S // 2)
    ref, no = ref0.crop(box_), no0.crop(box_); dy, dx, peak = shift(ref, no)
    no = Image.fromarray(np.roll(np.asarray(no), (dy, dx), (0, 1))); c = (128, 128, 128 + 1024, 128 + 1024)
    ref, no = ref.crop(c), no.crop(c)
    rb = np.asarray(ref.filter(ImageFilter.GaussianBlur(8)), np.float64).reshape(-1, 3); nb = np.asarray(no.filter(ImageFilter.GaussianBlur(8)), np.float64).reshape(-1, 3); na = np.asarray(no, np.float64)
    for ch in range(3):
        g, o = np.polyfit(nb[:, ch], rb[:, ch], 1); na[..., ch] = na[..., ch] * g + o
    no = Image.fromarray(np.clip(na, 0, 255).astype(np.uint8))
    r = {"noisy": {"psnr": psnr(no, ref), "ssim": ssim(no, ref), "hf": hf(no)}, "ref_hf": hf(ref)}; outs = {}
    for k in MODELS:
        t = time.time(); d = den(k, no); dt = time.time() - t; outs[k] = d
        diff = bl(d) - bl(no)
        r[k] = {"psnr": psnr(d, ref), "ssim": ssim(d, ref), "hf": hf(d), "tone_shift": float(np.abs(diff).mean()), "mean_shift_rgb": [float(x) for x in diff.reshape(-1, 3).mean(0)], "s": round(dt, 1)}
    b20 = no.filter(ImageFilter.GaussianBlur(2.0)); r["blur2.0"] = {"psnr": psnr(b20, ref), "ssim": ssim(b20, ref), "hf": hf(b20)}
    res[s] = r; print(s, {k: ({m: round(v, 3) for m, v in x.items() if not isinstance(v, list)} if isinstance(x, dict) else x) for k, x in r.items()}, flush=True)
    rows.append((s, ref, no, outs["scunet"], outs["nafnet"]))
json.dump(res, open(f"{OUT}/real_noise_results.json", "w"), indent=1)
for k in ("noisy", "scunet", "nafnet", "blur2.0"):
    print("MEAN", k, {m: round(float(np.mean([res[s][k][m] for s in res])), 3) for m in res[scenes[0]][k] if m in ("psnr", "ssim", "hf", "tone_shift", "s")})
print("MEAN ref_hf", round(float(np.mean([res[s]["ref_hf"] for s in res])), 3))
for s, ref, no, a, b in rows:
    cb = (384, 384, 384 + 256, 384 + 256); tiles = [im.crop(cb).resize((384, 384), Image.NEAREST) for im in (ref, no, a, b)]
    sh = Image.new("RGB", (384 * 4, 384)); [sh.paste(t, (i * 384, 0)) for i, t in enumerate(tiles)]; sh.save(f"{OUT}/real_{s}.jpg", quality=90)

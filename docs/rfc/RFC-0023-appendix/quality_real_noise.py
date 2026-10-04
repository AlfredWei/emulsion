"""M6 slice 0: SCUNet on REAL camera noise. NIND (Natural Image Noise Dataset, CC BY 4.0, Wikimedia Commons): same scene at ISO 200 (reference) and ISO 6400.
Per scene: central 1024x1024 crop, integer shift alignment by phase correlation, per-channel gain/offset tone match of the noisy shot to the reference
(fitted on 16x-blurred crops), then PSNR/SSIM against the ISO 200 reference for: noisy, SCUNet, Gaussian blur sigma 1.2 / 2.0. The ISO 200 reference still has its own noise.
usage: quality_real_noise.py MODELS_DIR NOISE_DIR OUT_DIR scene [scene...]   (files NOISE_DIR/{scene}_ISO200_*.jpg and _ISO6400_*.jpg)"""
import sys, os, glob, json, time, numpy as np, onnxruntime as ort
from PIL import Image, ImageFilter
M, ND, OUT = sys.argv[1:4]; scenes = sys.argv[4:]; os.makedirs(OUT, exist_ok=True)
sc = ort.InferenceSession(f"{M}/scunet_color_real_psnr.onnx", providers=["CPUExecutionProvider"])
def den(im):
    a = np.asarray(im, np.float32) / 255; H, W = a.shape[:2]; ph, pw = (-H) % 64, (-W) % 64
    a = np.pad(a, ((0, ph), (0, pw), (0, 0)), mode="reflect")
    y = sc.run(None, {"image": a.transpose(2, 0, 1)[None]})[0][0].transpose(1, 2, 0)[:H, :W]
    return Image.fromarray((np.clip(y, 0, 1) * 255 + 0.5).astype(np.uint8))
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
    W, H = ref0.size; S = 1280; box_ = (W // 2 - S // 2, H // 2 - S // 2, W // 2 + S // 2, H // 2 + S // 2)  # larger window for alignment, then crop 1024 after shifting
    ref, no = ref0.crop(box_), no0.crop(box_); dy, dx, peak = shift(ref, no)
    no = Image.fromarray(np.roll(np.asarray(no), (dy, dx), (0, 1))); c = (128, 128, 128 + 1024, 128 + 1024)
    ref, no = ref.crop(c), no.crop(c)
    # tone match (per channel linear fit on blurred crops)
    rb = np.asarray(ref.filter(ImageFilter.GaussianBlur(8)), np.float64).reshape(-1, 3); nb = np.asarray(no.filter(ImageFilter.GaussianBlur(8)), np.float64).reshape(-1, 3); na = np.asarray(no, np.float64)
    for ch in range(3):
        g, o = np.polyfit(nb[:, ch], rb[:, ch], 1); na[..., ch] = na[..., ch] * g + o
    no = Image.fromarray(np.clip(na, 0, 255).astype(np.uint8))
    t = time.time(); d = den(no); dt = time.time() - t
    b12, b20 = no.filter(ImageFilter.GaussianBlur(1.2)), no.filter(ImageFilter.GaussianBlur(2.0))
    r = {"shift": (dy, dx), "corr_peak": round(peak, 3), "noisy": (psnr(no, ref), ssim(no, ref)), "scunet": (psnr(d, ref), ssim(d, ref)), "blur1.2": (psnr(b12, ref), ssim(b12, ref)), "blur2.0": (psnr(b20, ref), ssim(b20, ref)), "scunet_s": round(dt, 1)}
    res[s] = r; print(s, {k: (tuple(round(x, 3) for x in v) if isinstance(v, tuple) and k != "shift" else v) for k, v in r.items()}, flush=True)
    rows.append((s, ref, no, d, b20))
json.dump(res, open(f"{OUT}/real_noise_results.json", "w"), indent=1)
print("MEAN", {k: tuple(round(float(np.mean([res[s][k][i] for s in res])), 3) for i in (0, 1)) for k in ("noisy", "scunet", "blur1.2", "blur2.0")})
for s, ref, no, d, b20 in rows:  # 100% crops: reference | noisy | scunet | blur 2.0
    cb = (384, 384, 384 + 256, 384 + 256); tiles = [im.crop(cb).resize((384, 384), Image.NEAREST) for im in (ref, no, d, b20)]
    sh = Image.new("RGB", (384 * 4, 384)); [sh.paste(t, (i * 384, 0)) for i, t in enumerate(tiles)]; sh.save(f"{OUT}/real_{s}.jpg", quality=90)

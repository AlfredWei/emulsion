"""Time and peak RSS of one denoiser on ORT CPU, random input. usage: bench.py MODEL.onnx SIZE [runs]  (one model per process so RSS is its own)"""
import sys, time, resource, numpy as np, onnxruntime as ort
m, S = sys.argv[1], int(sys.argv[2]); runs = int(sys.argv[3]) if len(sys.argv) > 3 else 3
t = time.time(); s = ort.InferenceSession(m, providers=["CPUExecutionProvider"]); load = time.time() - t
name = s.get_inputs()[0].name; x = np.random.rand(1, 3, S, S).astype(np.float32); ts = []
for _ in range(runs + 1):
    t = time.time(); s.run(None, {name: x}); ts.append(time.time() - t)
rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 2**20
print(f"{m.split('/')[-1]} {S}x{S}: load {load:.2f}s first {ts[0]:.2f}s median-of-rest {sorted(ts[1:])[len(ts[1:])//2]:.2f}s  peak RSS {rss:.0f} MB")

import onnxruntime as ort, numpy as np, time, sys, json
rng=np.random.default_rng(0)
def run(path, feeds, providers, n=5, label=""):
    so=ort.SessionOptions(); so.log_severity_level=3
    t0=time.time()
    try:
        s=ort.InferenceSession(path, so, providers=providers)
    except Exception as e:
        return {"err":str(e)[:160]}
    load=time.time()-t0
    used=s.get_providers()
    try:
        t=time.time(); s.run(None,feeds); first=time.time()-t
        ts=[]
        for _ in range(n):
            t=time.time(); s.run(None,feeds); ts.append(time.time()-t)
    except Exception as e:
        return {"err":"run: "+str(e)[:160], "providers":used}
    return {"load_s":round(load,2),"first_s":round(first,3),"median_s":round(float(np.median(ts)),3),"min_s":round(min(ts),3),"providers":used[:1]}
CPU=["CPUExecutionProvider"]; ML=[("CoreMLExecutionProvider",{"MLComputeUnits":"ALL"}),"CPUExecutionProvider"]
f32=lambda *s: rng.random(s,dtype=np.float32)
cases=[
 ("mobile_sam encoder 1024x1024", "models/mobile_sam/mobile_sam.encoder.onnx", {"input_image":(f32(1024,1024,3)*255)}),
 ("sam2 tiny encoder 1024", "models/sam2_hiera_tiny.encoder.onnx", {"image":f32(1,3,1024,1024)}),
 ("skyseg 320", "models/skyseg.onnx", {"input.1":f32(1,3,320,320)}),
 ("scunet real_psnr 512", "models/scunet_color_real_psnr.onnx", {"image":f32(1,3,512,512)}),
 ("scunet real_psnr 1024", "models/scunet_color_real_psnr.onnx", {"image":f32(1,3,1024,1024)}),
 ("realesr-general-x4v3 256->1024", "models/realesr-general-x4v3.onnx", {"input":f32(1,3,256,256)}),
 ("realesr-general-x4v3 512->2048", "models/realesr-general-x4v3.onnx", {"input":f32(1,3,512,512)}),
]
res={}
for name,path,feeds in cases:
    res[name]={"cpu":run(path,feeds,CPU), "coreml":run(path,feeds,ML)}
    print(name, json.dumps(res[name]), flush=True)
json.dump(res,open("bench_ort.json","w"),indent=1)

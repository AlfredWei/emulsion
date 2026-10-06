import numpy as np, onnxruntime as ort, sys, os
from PIL import Image
R="/Users/weishih-yi/src/claude_cowork_projects/lr_replace/test_image/"
out=sys.argv[1]; os.makedirs(out,exist_ok=True)
enc=ort.InferenceSession("models/sam2_hiera_tiny.encoder.onnx",providers=["CPUExecutionProvider"])
dec=ort.InferenceSession("models/sam2_hiera_tiny.decoder.onnx",providers=["CPUExecutionProvider"])
print([o.name for o in enc.get_outputs()],[o.name for o in dec.get_outputs()])
mean=np.array([0.485,0.456,0.406],np.float32); std=np.array([0.229,0.224,0.225],np.float32)
cases=[("Field-corn-Liechtenstein-landscape.jpg",0.5,0.75),("Field-corn-Liechtenstein-landscape.jpg",0.3,0.2),
 ("Szechenyi-Chain-Bridge-Budapest-night.jpg",0.37,0.72),("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg",0.28,0.3),
 ("Smiling-woman-pink-shirt-portrait.jpg",0.42,0.82),("Smiling-woman-pink-shirt-portrait.jpg",0.5,0.35)]
cache={}
for i,(f,fx,fy) in enumerate(cases):
    im=Image.open(R+f).convert("RGB")
    if f not in cache:
        x=(np.asarray(im.resize((1024,1024),Image.BILINEAR),np.float32)/255-mean)/std
        cache[f]=enc.run(None,{"image":x.transpose(2,0,1)[None].astype(np.float32)})
    hr0,hr1,emb=cache[f]
    pc=np.array([[[fx*1024,fy*1024],[0,0]]],np.float32); pl=np.array([[1,-1]],np.float32)
    masks,iou=dec.run(None,{"image_embed":emb,"high_res_feats_0":hr0,"high_res_feats_1":hr1,"point_coords":pc,"point_labels":pl,"mask_input":np.zeros((1,1,256,256),np.float32),"has_mask_input":np.zeros(1,np.float32)})
    masks[0].astype(np.float32).tofile(f"{out}/case{i}.logits.f32"); iou[0].astype(np.float32).tofile(f"{out}/case{i}.iou.f32")
    print(i,f,fx,fy,iou[0].round(3))

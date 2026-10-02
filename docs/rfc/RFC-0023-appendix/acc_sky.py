import numpy as np, onnxruntime as ort, sys
from PIL import Image
R="/Users/weishih-yi/src/claude_cowork_projects/lr_replace/test_image/"
imgs=["Field-corn-Liechtenstein-landscape.jpg","Cavenagh-Bridge-Singapore-panorama-frame-a.jpg","Szechenyi-Chain-Bridge-Budapest-night.jpg","Smiling-woman-pink-shirt-portrait.jpg"]
s=ort.InferenceSession("models/skyseg.onnx",providers=["CPUExecutionProvider"])
mean=np.array([0.485,0.456,0.406],np.float32); std=np.array([0.229,0.224,0.225],np.float32)
tiles=[]
for f in imgs:
    im=Image.open(R+f).convert("RGB"); W,H=im.size
    x=np.asarray(im.resize((320,320),Image.BILINEAR),np.float32)/255
    x=((x-mean)/std).transpose(2,0,1)[None].astype(np.float32)
    o=s.run(None,{"input.1":x})[0].squeeze()
    mn,mx=o.min(),o.max(); m=(o-mn)/(mx-mn)
    print(f, im.size, "raw out range %.3f..%.3f"%(mn,mx), "sky frac(>0.5) = %.3f"%(m>0.5).mean())
    mm=np.asarray(Image.fromarray((m*255).astype(np.uint8)).resize((W,H),Image.BILINEAR),np.float32)/255
    base=np.asarray(im,np.float32)
    ov=base.copy(); ov[...,0]=np.where(mm>0.5, base[...,0]*0.4+255*0.6, base[...,0]); ov[...,1]*=np.where(mm>0.5,0.4,1); ov[...,2]*=np.where(mm>0.5,0.4,1)
    t=np.concatenate([base,ov],axis=1)
    th=Image.fromarray(t.astype(np.uint8)); th=th.resize((int(th.width*360/th.height),360)); tiles.append(np.asarray(th))
w=max(t.shape[1] for t in tiles)
tiles=[np.pad(t,((0,0),(0,w-t.shape[1]),(0,0))) for t in tiles]
Image.fromarray(np.concatenate(tiles,axis=0)).save("accuracy-skyseg.jpg")

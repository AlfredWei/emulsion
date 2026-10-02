import numpy as np, onnxruntime as ort, time
from PIL import Image
R="/Users/weishih-yi/src/claude_cowork_projects/lr_replace/test_image/"
enc=ort.InferenceSession("models/sam2_hiera_tiny.encoder.onnx",providers=["CPUExecutionProvider"])
dec=ort.InferenceSession("models/sam2_hiera_tiny.decoder.onnx",providers=["CPUExecutionProvider"])
mean=np.array([0.485,0.456,0.406],np.float32); std=np.array([0.229,0.224,0.225],np.float32)
cases=[("Field-corn-Liechtenstein-landscape.jpg","corn field",(0.5,0.75)),
       ("Field-corn-Liechtenstein-landscape.jpg","clouds/sky",(0.3,0.2)),
       ("Szechenyi-Chain-Bridge-Budapest-night.jpg","bridge tower",(0.37,0.72)),
       ("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg","white bank building",(0.28,0.3)),
       ("Smiling-woman-pink-shirt-portrait.jpg","pink shirt",(0.42,0.82)),
       ("Smiling-woman-pink-shirt-portrait.jpg","face",(0.5,0.35))]
cache={}; tiles=[]
for f,label,(fx,fy) in cases:
    im=Image.open(R+f).convert("RGB"); W,H=im.size
    if f not in cache:
        x=(np.asarray(im.resize((1024,1024),Image.BILINEAR),np.float32)/255-mean)/std
        x=x.transpose(2,0,1)[None].astype(np.float32)
        t=time.time(); o=enc.run(None,{"image":x}); cache[f]=(o,time.time()-t)
    (hr0,hr1,emb),et=cache[f]
    pc=np.array([[[fx*1024,fy*1024],[0,0]]],np.float32); pl=np.array([[1,-1]],np.float32)
    t=time.time()
    masks,iou=dec.run(None,{"image_embed":emb,"high_res_feats_0":hr0,"high_res_feats_1":hr1,"point_coords":pc,"point_labels":pl,"mask_input":np.zeros((1,1,256,256),np.float32),"has_mask_input":np.zeros(1,np.float32)})
    dt=time.time()-t
    k=int(np.argmax(iou[0])); mk=masks[0,k]
    mm=np.asarray(Image.fromarray(mk.astype(np.float32)).resize((W,H),Image.BILINEAR))>0
    areas=[float((np.asarray(Image.fromarray(masks[0,j].astype(np.float32)).resize((W,H)))>0).mean()) for j in range(masks.shape[1])]
    print(f"{f[:28]:28s} {label:18s} enc {et*1000:.0f}ms dec {dt*1000:.0f}ms masks {masks.shape} iou {iou[0].round(2)} best#{k} areas {[round(a,3) for a in areas]}")
    base=np.asarray(im,np.float32); ov=base.copy()
    ov[...,0]=np.where(mm,base[...,0]*0.4+255*0.6,base[...,0]); ov[...,1]=np.where(mm,base[...,1]*0.4,base[...,1]); ov[...,2]=np.where(mm,base[...,2]*0.4,base[...,2])
    cx,cy=int(fx*W),int(fy*H); r=max(4,W//150); ov[max(0,cy-r):cy+r,max(0,cx-r):cx+r]=[0,255,0]
    th=Image.fromarray(ov.astype(np.uint8)).resize((400,300)); tiles.append(np.asarray(th))
rows=[np.concatenate(tiles[i:i+3],axis=1) for i in range(0,6,3)]
Image.fromarray(np.concatenate(rows,axis=0)).save("accuracy-sam2-tiny.jpg")

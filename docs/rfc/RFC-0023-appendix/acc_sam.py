import numpy as np, onnxruntime as ort, time
from PIL import Image
R="/Users/weishih-yi/src/claude_cowork_projects/lr_replace/test_image/"
enc=ort.InferenceSession("models/mobile_sam/mobile_sam.encoder.onnx",providers=["CPUExecutionProvider"])
dec=ort.InferenceSession("models/mobile_sam/sam_vit_h_4b8939.decoder.onnx",providers=["CPUExecutionProvider"])
# (file, label, click in original-image fractional coords (x,y))
cases=[("Field-corn-Liechtenstein-landscape.jpg","corn field",(0.5,0.75)),
       ("Field-corn-Liechtenstein-landscape.jpg","clouds/sky",(0.3,0.2)),
       ("Szechenyi-Chain-Bridge-Budapest-night.jpg","bridge tower",(0.37,0.72)),
       ("Cavenagh-Bridge-Singapore-panorama-frame-a.jpg","white bank building",(0.28,0.3)),
       ("Smiling-woman-pink-shirt-portrait.jpg","pink shirt",(0.42,0.82)),
       ("Smiling-woman-pink-shirt-portrait.jpg","face",(0.5,0.35))]
cache={}; tiles=[]
for f,label,(fx,fy) in cases:
    im=Image.open(R+f).convert("RGB"); W,H=im.size
    sc=1024/max(W,H); nw,nh=round(W*sc),round(H*sc)
    if f not in cache:
        x=np.asarray(im.resize((nw,nh),Image.BILINEAR),np.float32)
        t=time.time(); emb=enc.run(None,{"input_image":x})[0]; cache[f]=(emb,time.time()-t)
    emb,et=cache[f]
    pc=np.array([[[fx*W*sc,fy*H*sc],[0,0]]],np.float32); pl=np.array([[1,-1]],np.float32)
    t=time.time()
    masks,iou,low=dec.run(None,{"image_embeddings":emb,"point_coords":pc,"point_labels":pl,"mask_input":np.zeros((1,1,256,256),np.float32),"has_mask_input":np.zeros(1,np.float32),"orig_im_size":np.array([H,W],np.float32)})
    dt=time.time()-t
    k=int(np.argmax(iou[0])); m=masks[0,k]>0
    print(f"{f[:28]:28s} {label:18s} enc {et*1000:.0f}ms dec {dt*1000:.0f}ms masks {masks.shape} iou {iou[0].round(2)} chosen#{k} area {m.mean():.3f}")
    base=np.asarray(im,np.float32); ov=base.copy()
    ov[...,0]=np.where(m,base[...,0]*0.4+255*0.6,base[...,0]); ov[...,1]=np.where(m,base[...,1]*0.4,base[...,1]); ov[...,2]=np.where(m,base[...,2]*0.4,base[...,2])
    cx,cy=int(fx*W),int(fy*H); r=max(4,W//150); ov[max(0,cy-r):cy+r,max(0,cx-r):cx+r]=[0,255,0]
    th=Image.fromarray(ov.astype(np.uint8)); th=th.resize((int(th.width*300/th.height),300)); tiles.append(np.asarray(th))
w=max(t.shape[1] for t in tiles); rows=[]
for i in range(0,len(tiles),3):
    row=[np.pad(t,((0,0),(0,w-t.shape[1]),(0,0))) for t in tiles[i:i+3]]
    while len(row)<3: row.append(np.zeros_like(row[0]))
    rows.append(np.concatenate(row,axis=1))
Image.fromarray(np.concatenate(rows,axis=0)).save("accuracy-mobilesam.jpg")

import numpy as np, onnxruntime as ort, json, glob
from PIL import Image
meta=json.load(open("photos/meta.json"))
sk=ort.InferenceSession("models/skyseg.onnx",providers=["CPUExecutionProvider"])
enc=ort.InferenceSession("models/sam2_hiera_tiny.encoder.onnx",providers=["CPUExecutionProvider"])
dec=ort.InferenceSession("models/sam2_hiera_tiny.decoder.onnx",providers=["CPUExecutionProvider"])
mean=np.array([0.485,0.456,0.406],np.float32); std=np.array([0.229,0.224,0.225],np.float32)
# sky click per photo (fx,fy) chosen by eye from the photo's layout: a point that is unambiguously sky
clicks={1:(0.5,0.12),2:(0.5,0.1),3:(0.5,0.12),4:(0.5,0.1),5:(0.5,0.1),6:(0.5,0.1),7:(0.5,0.08),8:(0.6,0.2)}
def overlay(base,m,color=(255,0,0)):
    ov=base.copy()
    for c in range(3): ov[...,c]=np.where(m,base[...,c]*0.45+color[c]*0.55,base[...,c])
    return ov
rows=[]
for i,mt in enumerate(meta,1):
    im=Image.open(mt["file"]).convert("RGB"); W,H=im.size
    if max(W,H)>2000: im=im.resize((2000,round(2000*H/W)) if W>=H else (round(2000*W/H),2000)); W,H=im.size
    x=((np.asarray(im.resize((320,320),Image.BILINEAR),np.float32)/255-mean)/std).transpose(2,0,1)[None].astype(np.float32)
    o=sk.run(None,{"input.1":x})[0].squeeze()
    ms=np.asarray(Image.fromarray(o.astype(np.float32)).resize((W,H),Image.BILINEAR))>0.5
    xs=((np.asarray(im.resize((1024,1024),Image.BILINEAR),np.float32)/255-mean)/std).transpose(2,0,1)[None].astype(np.float32)
    hr0,hr1,emb=enc.run(None,{"image":xs})
    fx,fy=clicks[i]
    masks,iou=dec.run(None,{"image_embed":emb,"high_res_feats_0":hr0,"high_res_feats_1":hr1,"point_coords":np.array([[[fx*1024,fy*1024],[0,0]]],np.float32),"point_labels":np.array([[1,-1]],np.float32),"mask_input":np.zeros((1,1,256,256),np.float32),"has_mask_input":np.zeros(1,np.float32)})
    k=int(np.argmax(iou[0]))
    up=lambda j: np.asarray(Image.fromarray(masks[0,j].astype(np.float32)).resize((W,H),Image.BILINEAR))>0
    m2=up(k)
    inter=(ms&m2).sum(); uni=(ms|m2).sum()
    # sky fraction per candidate and in each model
    print(f"{i} {mt['category']:13s} skyseg {ms.mean():.3f} | sam2 best#{k} {m2.mean():.3f} (areas {[round(float(up(j).mean()),3) for j in range(3)]} iou {iou[0].round(2)}) | agreement IoU {inter/max(uni,1):.2f}")
    base=np.asarray(im,np.float32); h=240
    tiles=[]
    for t in (base,overlay(base,ms,(255,0,0)),overlay(base,m2,(0,160,255))):
        t=Image.fromarray(t.astype(np.uint8)); tiles.append(np.array(t.resize((int(t.width*h/t.height),h))))
    tiles[2][max(0,int(fy*h)-3):int(fy*h)+3, max(0,int(fx*tiles[2].shape[1])-3):int(fx*tiles[2].shape[1])+3]=[0,255,0]
    rows.append(tiles)
for part,(a,b) in enumerate([(0,4),(4,7)]):  # photo 8 (CC BY-SA) is analysed but never put in a committed sheet
    rs=[]
    for tiles in rows[a:b]:
        w=sum(t.shape[1] for t in tiles); rs.append(np.pad(np.concatenate(tiles,axis=1),((0,0),(0,max(0,1500-w)),(0,0)))[:,:1500])
    Image.fromarray(np.concatenate(rs,axis=0)).save(f"accuracy-sky-eight-photos-part{part+1}.jpg",quality=82)

"""RFC-0029 slice 4a: export the candidate super-resolution models to ONNX (4x, input NCHW float 0..1, output NCHW float 0..1).
usage: convert.py MODEL WEIGHTS_DIR OUT_DIR [SIZE]
MODEL: general_x4v3 | x4plus | swinir_gan | swinir_psnr
The Real-ESRGAN networks are re-written here from the upstream definitions (SRVGGNetCompact, RRDBNet) so basicsr is not needed.
SwinIR's network_swinir.py is the upstream file (Apache-2.0); timm's three helpers it imports are shimmed. load_state_dict is strict."""
import sys, os, types, math, importlib.util
import torch, torch.nn as nn, torch.nn.functional as F

class SRVGGNetCompact(nn.Module):  # xinntao/Real-ESRGAN, realesrgan/archs/srvgg_arch.py
    def __init__(self, num_in_ch=3, num_out_ch=3, num_feat=64, num_conv=16, upscale=4):
        super().__init__(); self.upscale = upscale
        self.body = nn.ModuleList([nn.Conv2d(num_in_ch, num_feat, 3, 1, 1), nn.PReLU(num_parameters=num_feat)])
        for _ in range(num_conv): self.body += [nn.Conv2d(num_feat, num_feat, 3, 1, 1), nn.PReLU(num_parameters=num_feat)]
        self.body.append(nn.Conv2d(num_feat, num_out_ch * upscale * upscale, 3, 1, 1)); self.upsampler = nn.PixelShuffle(upscale)
    def forward(self, x):
        out = x
        for layer in self.body: out = layer(out)
        return self.upsampler(out) + F.interpolate(x, scale_factor=self.upscale, mode="nearest")

class ResidualDenseBlock(nn.Module):  # basicsr/archs/rrdbnet_arch.py
    def __init__(self, nf=64, gc=32):
        super().__init__()
        self.conv1 = nn.Conv2d(nf, gc, 3, 1, 1); self.conv2 = nn.Conv2d(nf + gc, gc, 3, 1, 1); self.conv3 = nn.Conv2d(nf + 2 * gc, gc, 3, 1, 1)
        self.conv4 = nn.Conv2d(nf + 3 * gc, gc, 3, 1, 1); self.conv5 = nn.Conv2d(nf + 4 * gc, nf, 3, 1, 1); self.lrelu = nn.LeakyReLU(0.2, True)
    def forward(self, x):
        x1 = self.lrelu(self.conv1(x)); x2 = self.lrelu(self.conv2(torch.cat((x, x1), 1))); x3 = self.lrelu(self.conv3(torch.cat((x, x1, x2), 1)))
        x4 = self.lrelu(self.conv4(torch.cat((x, x1, x2, x3), 1))); x5 = self.conv5(torch.cat((x, x1, x2, x3, x4), 1)); return x5 * 0.2 + x

class RRDB(nn.Module):
    def __init__(self, nf, gc=32):
        super().__init__(); self.rdb1 = ResidualDenseBlock(nf, gc); self.rdb2 = ResidualDenseBlock(nf, gc); self.rdb3 = ResidualDenseBlock(nf, gc)
    def forward(self, x): return self.rdb3(self.rdb2(self.rdb1(x))) * 0.2 + x

class RRDBNet(nn.Module):  # scale 4 only (no pixel-unshuffle path)
    def __init__(self, num_in_ch=3, num_out_ch=3, num_feat=64, num_block=23, num_grow_ch=32):
        super().__init__()
        self.conv_first = nn.Conv2d(num_in_ch, num_feat, 3, 1, 1); self.body = nn.Sequential(*[RRDB(num_feat, num_grow_ch) for _ in range(num_block)])
        self.conv_body = nn.Conv2d(num_feat, num_feat, 3, 1, 1); self.conv_up1 = nn.Conv2d(num_feat, num_feat, 3, 1, 1); self.conv_up2 = nn.Conv2d(num_feat, num_feat, 3, 1, 1)
        self.conv_hr = nn.Conv2d(num_feat, num_feat, 3, 1, 1); self.conv_last = nn.Conv2d(num_feat, num_out_ch, 3, 1, 1); self.lrelu = nn.LeakyReLU(0.2, True)
    def forward(self, x):
        feat = self.conv_first(x); feat = feat + self.conv_body(self.body(feat))
        feat = self.lrelu(self.conv_up1(F.interpolate(feat, scale_factor=2, mode="nearest")))
        feat = self.lrelu(self.conv_up2(F.interpolate(feat, scale_factor=2, mode="nearest")))
        return self.conv_last(self.lrelu(self.conv_hr(feat)))

def shim_timm():
    for n in ("timm", "timm.models", "timm.models.layers"): sys.modules[n] = types.ModuleType(n)
    class DropPath(nn.Module):  # inference only: identity
        def __init__(self, p=0.): super().__init__()
        def forward(self, x): return x
    def to_2tuple(x): return tuple(x) if isinstance(x, (list, tuple)) else (x, x)
    def trunc_normal_(t, std=1., **k): return nn.init.normal_(t, std=std)  # weights are loaded over it
    m = sys.modules["timm.models.layers"]; m.DropPath, m.to_2tuple, m.trunc_normal_ = DropPath, to_2tuple, trunc_normal_

def build(model):
    if model == "general_x4v3": return SRVGGNetCompact(num_conv=32), "realesr-general-x4v3.pth"
    if model == "x4plus": return RRDBNet(), "RealESRGAN_x4plus.pth"
    shim_timm()
    spec = importlib.util.spec_from_file_location("network_swinir", os.path.join(os.path.dirname(os.path.abspath(__file__)), "src", "network_swinir.py"))
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    net = m.SwinIR(upscale=4, in_chans=3, img_size=64, window_size=8, img_range=1., depths=[6] * 6, embed_dim=180, num_heads=[6] * 6, mlp_ratio=2, upsampler="nearest+conv", resi_connection="1conv")
    return net, f"003_realSR_BSRGAN_DFO_s64w8_SwinIR-M_x4_{'GAN' if model == 'swinir_gan' else 'PSNR'}.pth"

if __name__ == "__main__":
    model, wdir, out = sys.argv[1:4]; size = int(sys.argv[4]) if len(sys.argv) > 4 else 64
    net, fn = build(model); ck = torch.load(os.path.join(wdir, fn), map_location="cpu", weights_only=True)
    key = next((k for k in ("params_ema", "params") if k in ck), None); sd = ck[key] if key else ck
    print(model, "checkpoint key:", key, "|", net.load_state_dict(sd, strict=True)); net.eval()
    print("params", sum(p.numel() for p in net.parameters()))
    x = torch.rand(1, 3, size, size); os.makedirs(out, exist_ok=True); path = os.path.join(out, f"{model}.onnx")
    dyn = {"image": {2: "h", 3: "w"}, "out": {2: "h", 3: "w"}} if model in ("general_x4v3", "x4plus") else None  # SwinIR pads with python ints: fixed size
    with torch.no_grad(): ref = net(x)
    torch.onnx.export(net, x, path, input_names=["image"], output_names=["out"], opset_version=17, dynamic_axes=dyn, dynamo=False)
    import onnxruntime as ort, numpy as np
    s = ort.InferenceSession(path, providers=["CPUExecutionProvider"]); y = s.run(None, {"image": x.numpy()})[0]
    print("onnx vs torch max abs diff", float(np.abs(y - ref.numpy()).max()), "out shape", y.shape, "size MB", round(os.path.getsize(path) / 1e6, 1))

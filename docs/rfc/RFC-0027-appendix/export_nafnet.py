"""Export NAFNet-SIDD-width32 (megvii-research/NAFNet, MIT) to ONNX with dynamic H, W.
Only LayerNorm2d is needed from basicsr's arch_util, so basicsr.utils is stubbed rather than installed.
usage: export_nafnet.py   (reads models/NAFNet-SIDD-width32.pth, writes models/nafnet_sidd_w32.onnx)"""
import sys, types, importlib.util, torch
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path); m = importlib.util.module_from_spec(spec); sys.modules[name] = m; spec.loader.exec_module(m); return m
for n in ("basicsr", "basicsr.utils", "basicsr.models", "basicsr.models.archs"): sys.modules[n] = types.ModuleType(n)
sys.modules["basicsr.utils"].get_root_logger = lambda *a, **k: None
load("basicsr.models.archs.arch_util", "nafnet_src/arch_util.py"); load("basicsr.models.archs.local_arch", "nafnet_src/local_arch.py")
arch = load("NAFNet_arch", "nafnet_src/NAFNet_arch.py")
net = arch.NAFNet(img_channel=3, width=32, middle_blk_num=12, enc_blk_nums=[2, 2, 4, 8], dec_blk_nums=[2, 2, 2, 2])
ck = torch.load("models/NAFNet-SIDD-width32.pth", map_location="cpu", weights_only=True)
sd = ck.get("params", ck); print(net.load_state_dict(sd, strict=True)); net.eval()
print("params", sum(p.numel() for p in net.parameters()))
x = torch.rand(1, 3, 512, 512)
torch.onnx.export(net, x, "models/nafnet_sidd_w32.onnx", input_names=["image"], output_names=["out"], opset_version=17,
                  dynamic_axes={"image": {2: "h", 3: "w"}, "out": {2: "h", 3: "w"}}, dynamo=False)
print("exported")

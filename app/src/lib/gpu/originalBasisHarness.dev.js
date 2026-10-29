// @ts-nocheck -- a throwaway dev harness, not app code
// RFC-0036 R2: compiles the shipped fs_mask in a real WebGPU device and checks a luminance mask with
// basis "original" against the trapezoid formula and against edits (global exposure -1..+3 EV, an
// earlier mask, a modifier). Not part of CI (needs a Chromium with WebGPU).
// Run: `npm run dev`, open http://localhost:1420/ in a Chromium with WebGPU and, in its console:
//   (await import("/src/lib/gpu/originalBasisHarness.dev.js")).runHarness()
// Nothing imports it, so it is not in the app bundle.
import { WGSL } from "/src/lib/gpu/shaders/index.js";
import { packMasks } from "/src/lib/gpu/maskPack.js";

export async function runHarness() {
const f16 = (() => { const f = new Float32Array(1), u = new Uint32Array(f.buffer); return (v) => {
  f[0] = v; const x = u[0]; const sign = (x >>> 16) & 0x8000; let e = ((x >> 23) & 0xff) - 127 + 15; let m = x & 0x7fffff;
  if (e <= 0) return sign; if (e >= 31) return sign | 0x7c00; return sign | (e << 10) | ((m + 0x1000) >> 13); }; })();
const W = 64, H = 1, N = W;
const adapter = await navigator.gpu.requestAdapter();
const device = await adapter.requestDevice();
const errors = []; device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
const module = device.createShaderModule({ code: WGSL });
const info = await module.getCompilationInfo();
const problems = info.messages.filter((m) => m.type !== "info").map((m) => `line ${m.lineNum}: ${m.message}`);
const pipeline = device.createRenderPipeline({ layout: "auto", vertex: { module, entryPoint: "vs_main" },
  fragment: { module, entryPoint: "fs_mask", targets: [{ format: "rgba32float" }] }, primitive: { topology: "triangle-list" } });
const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
const mkTex = (px) => { const t = device.createTexture({ size: [W, H], format: "rgba16float", usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  const d = new Uint16Array(W * 4); for (let x = 0; x < W; x++) { d[x*4] = f16(px[x][0]); d[x*4+1] = f16(px[x][1]); d[x*4+2] = f16(px[x][2]); d[x*4+3] = f16(1); }
  device.queue.writeTexture({ texture: t }, d, { bytesPerRow: W * 8 }, [W, H]); return t; };
const brush = device.createTexture({ size: [4, 4, 2], format: "r8unorm", usage: GPUTextureUsage.TEXTURE_BINDING });
const ubuf = (arr) => { const b = device.createBuffer({ size: Math.max(256, arr.byteLength), usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST }); device.queue.writeBuffer(b, 0, arr); return b; };
const out = device.createTexture({ size: [W, H], format: "rgba32float", usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC });

const luma = (c) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
const trap = (l, min, max, feather) => { const s = Math.min(Math.max(feather / 100, 0), 0.999), fw = s * 0.5, d = Math.max(fw, 0.001);
  return Math.min(Math.max(Math.min((l - (min / 100 - fw)) / d, (max / 100 + fw - l) / d), 0), 1); };

// bytesPerRow must be a multiple of 256: 64 px * 16 B = 1024, fine.
async function render(orig, pre, masks, which) {
  const { maskData, modData } = packMasks(masks, () => 0);
  const adj = new Float32Array(16); adj[3] = masks.length; adj[4] = which;
  const bg = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [
    { binding: 0, resource: sampler }, { binding: 2, resource: { buffer: ubuf(adj) } },
    { binding: 3, resource: { buffer: ubuf(maskData) } }, { binding: 63, resource: { buffer: ubuf(modData) } },
    { binding: 4, resource: brush.createView({ dimension: "2d-array" }) },
    { binding: 26, resource: { buffer: ubuf(new Float32Array(4)) } },
    { binding: 27, resource: mkTex(pre).createView() }, { binding: 64, resource: mkTex(orig).createView() } ] });
  const enc = device.createCommandEncoder();
  const pass = enc.beginRenderPass({ colorAttachments: [{ view: out.createView(), loadOp: "clear", storeOp: "store", clearValue: [0,0,0,1] }] });
  pass.setPipeline(pipeline); pass.setBindGroup(0, bg); pass.draw(3); pass.end();
  const rb = device.createBuffer({ size: W * 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  enc.copyTextureToBuffer({ texture: out }, { buffer: rb, bytesPerRow: W * 16 }, [W, H]);
  device.queue.submit([enc.finish()]); await rb.mapAsync(GPUMapMode.READ);
  const f = new Float32Array(rb.getMappedRange().slice(0)); rb.unmap();
  return Array.from({ length: W }, (_, x) => f[x*4]);
}
// overlay on the selected mask: out_r = p_r + w*0.55*(1-p_r), p = the red the mask entered with (pre, or the output of the masks before it)
async function weights(orig, pre, masks, which = 0, before = null) {
  const o = await render(orig, pre, masks, which);
  const p = before ? await render(orig, pre, before, 99) : pre.map((c) => c[0]);
  return o.map((v, x) => (v - p[x]) / (0.55 * (1 - p[x])));
}
const orig = Array.from({ length: W }, (_, x) => { const v = 0.02 + 0.5 * x / (W - 1); return [v, v * 0.9, v * 0.8]; });   // luma 0.02..0.5ish
const edit = (ev) => orig.map((c) => c.map((v) => Math.min(1, v * 2 ** ev)));
const lum = (extra = {}) => ({ op: "luminance_range_mask", id: "L", rangeMin: 0, rangeMax: 25, feather: 30, invert: false, exposure: 0, contrast: 0, saturation: 0, ...extra });
const maxErr = (got, want) => Math.max(...got.map((g, i) => Math.abs(g - want[i])));
const res = {};
const want = orig.map((c) => trap(luma(c), 0, 25, 30));
res.compile = { problems, errorsAfterPipeline: errors.slice() };
// p_r < 1 needed for the readback; edit(3) saturates r at 1 for bright pixels -> those are skipped (NaN) and only compared where p_r < 0.97
const cmp = (got, want, pre) => { let m = 0, n = 0; got.forEach((g, i) => { if (pre[i][0] < 0.97) { m = Math.max(m, Math.abs(g - want[i])); n++; } }); return { maxErr: +m.toFixed(4), compared: n }; };
// A. original basis: same weights at every global edit
res.A_original = {}; for (const ev of [-1, 0, 1, 2, 3]) { const pre = edit(ev); const g = await weights(orig, pre, [lum({ basis: "original" })]); res.A_original[ev] = cmp(g, want, pre); }
// B. default basis moves with the edit and matches the formula on the EDITED luma
res.B_default = {}; for (const ev of [0, 1, 2]) { const pre = edit(ev); const g = await weights(orig, pre, [lum()]); res.B_default[ev] = cmp(g, pre.map((c) => trap(luma(c), 0, 25, 30)), pre); }
// C. a +2 EV full-weight mask before it cannot move an original-basis selection
const lifter = { op: "luminance_range_mask", id: "U", rangeMin: 0, rangeMax: 100, feather: 0, invert: false, exposure: 1, contrast: 0, saturation: 0 };
{ const want2 = orig.map((c) => trap(luma(c), 0, 10, 30));
  const p = await render(orig, orig, [lifter], 99);          // what the selected mask enters with
  const gO = await weights(orig, orig, [lifter, lum({ basis: "original", rangeMax: 10, id: "S" })], 1, [lifter]);
  const gD = await weights(orig, orig, [lifter, lum({ rangeMax: 10, id: "S" })], 1, [lifter]);
  let mO = 0, mD = 0, ramp = 0, n = 0;
  gO.forEach((g, i) => { if (p[i] < 0.97) { n++; mO = Math.max(mO, Math.abs(g - want2[i])); mD = Math.max(mD, Math.abs(gD[i] - want2[i])); if (want2[i] > 0.05 && want2[i] < 0.95) ramp++; } });
  res.C_after_lifter = { compared: n, rampPixels: ramp, originalBasis_maxErr: +mO.toFixed(4), defaultBasis_maxDeviation: +mD.toFixed(3) }; }
// D. modifier path: base covers everything, an original-basis luminance modifier intersects
{ const pre = edit(2); const base = lum({ rangeMin: 0, rangeMax: 100, feather: 0, id: "M", modifiers: [{ id: "mod", combine: "intersect", shape: { op: "luminance_range_mask", rangeMin: 0, rangeMax: 25, feather: 30, invert: false, basis: "original" } }] });
  const g = await weights(orig, pre, [base]); res.D_modifier_original = cmp(g, want.map((w, i) => w * trap(luma(pre[i]), 0, 100, 0)), pre); }
const sel = want.map((w) => +w.toFixed(3));
res.sample = { luma: orig.filter((_, i) => i % 9 === 0).map((c) => +luma(c).toFixed(3)), weight: sel.filter((_, i) => i % 9 === 0) };
res.errors = errors;
  return res;
}

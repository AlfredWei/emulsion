// RFC-0025 verification: cost of the modifier loop in the app's REAL fs_mask on a real GPU.
// Renders a 2048x1365 frame (the Develop preview size) repeatedly with: no masks; 8 masks and no
// modifiers; the same 8 masks carrying 16 modifiers in total (the cap); and a brush-heavy variant.
// Wall-clock per frame = submit + onSubmittedWorkDone over a batch, median of 5 batches.
// How to run: `npm --prefix app run dev`, open http://localhost:1420, paste into the console (or
// copy this file to app/static/ and eval(await (await fetch('/gpu_mask_timing.js')).text())).
(async () => {
  const { WGSL } = await import('/src/lib/gpu/shaders/index.js');
  const { packMasks } = await import('/src/lib/gpu/maskPack.js');
  const { MAX_MASKS, MAX_MODIFIERS } = await import('/src/lib/gpu/gpuHandles.js');
  const W = 2048, H = 1365;
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const info = adapter.info ?? {};
  const module = device.createShaderModule({ code: WGSL });
  const errors = (await module.getCompilationInfo()).messages.filter((m) => m.type === 'error');
  if (errors.length) throw new Error('WGSL compile error: ' + errors.map((m) => `${m.lineNum}: ${m.message}`).join('; '));
  const pipeline = device.createRenderPipeline({
    layout: 'auto', vertex: { module, entryPoint: 'vs_main' },
    fragment: { module, entryPoint: 'fs_mask', targets: [{ format: 'rgba8unorm' }] },
  });
  const mk = (size) => device.createBuffer({ size, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const adjBuf = mk(64), masksBuf = mk(MAX_MASKS * 16 * 4), modsBuf = mk(MAX_MODIFIERS * 12 * 4), clipBuf = mk(16);
  device.queue.writeBuffer(clipBuf, 0, new Float32Array(4));
  const sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  const brushTex = device.createTexture({ size: [W, H, MAX_MASKS], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING });
  const preTex = device.createTexture({ size: [W, H], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  const src = new Uint8Array(W * H * 4); for (let i = 0; i < src.length; i++) src[i] = (i * 2654435761 >>> 24) & 255;
  device.queue.writeTexture({ texture: preTex }, src, { bytesPerRow: W * 4 }, { width: W, height: H });
  const out = device.createTexture({ size: [W, H], format: 'rgba8unorm', usage: GPUTextureUsage.RENDER_ATTACHMENT });
  const bind = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [
    { binding: 0, resource: sampler }, { binding: 2, resource: { buffer: adjBuf } }, { binding: 3, resource: { buffer: masksBuf } },
    { binding: 63, resource: { buffer: modsBuf } }, { binding: 4, resource: brushTex.createView({ dimension: '2d-array' }) },
    { binding: 26, resource: { buffer: clipBuf } }, { binding: 27, resource: preTex.createView() } ] });

  const adj = { exposure: 0.5, contrast: 10, saturation: 5 };
  const radial = (id, extra = {}) => ({ op: 'radial_gradient_mask', id, center: { x: .5, y: .5 }, radiusX: .4, radiusY: .4, feather: 30, invert: true, ...adj, ...extra });
  const linear = (id, extra = {}) => ({ op: 'linear_gradient_mask', id, start: { x: .1, y: .5 }, end: { x: .9, y: .5 }, feather: 20, invert: false, ...adj, ...extra });
  const lum = (id, extra = {}) => ({ op: 'luminance_range_mask', id, rangeMin: 20, rangeMax: 80, feather: 30, invert: false, ...adj, ...extra });
  const col = (id, extra = {}) => ({ op: 'color_range_mask', id, refColor: { r: .5, g: .5, b: .5 }, range: 40, feather: 30, invert: false, ...adj, ...extra });
  const brush = (id, extra = {}) => ({ op: 'brush_mask', id, dabs: [], invert: false, ...adj, ...extra });
  const shapeOf = (m) => { const { id, exposure, contrast, saturation, modifiers, ...s } = m; return s; };
  const mod = (kind, i, combine) => ({ id: `${kind}${i}`, combine, shape: shapeOf({ radial, linear, lum, col, brush }[kind](`x${i}`)) });
  const eight = () => [radial('r0'), linear('l0'), brush('b0'), lum('u0'), col('c0'), radial('r1'), linear('l1'), lum('u1')];
  const withMods = (masks, kinds) => { let n = 0; masks.forEach((m, i) => { m.modifiers = []; for (let k = 0; k < 2 && n < 16; k++, n++) m.modifiers.push(mod(kinds[(n) % kinds.length], n, ['add', 'subtract', 'intersect'][n % 3])); }); return masks; };
  const scenes = {
    'no masks': [],
    '8 masks, no modifiers': eight(),
    '8 masks, 16 modifiers (radial/linear/luminance/colour)': withMods(eight(), ['radial', 'linear', 'lum', 'col']),
    '8 masks, 16 modifiers (all brush: 8 layers shared)': withMods(eight(), ['brush']),
  };

  async function frame(n) {
    const enc = device.createCommandEncoder();
    for (let i = 0; i < n; i++) {
      const pass = enc.beginRenderPass({ colorAttachments: [{ view: out.createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
      pass.setPipeline(pipeline); pass.setBindGroup(0, bind); pass.draw(3); pass.end();
    }
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
  }
  const results = [];
  for (const [name, masks] of Object.entries(scenes)) {
    const { maskData, modData, modCount } = packMasks(masks, (id) => (id.startsWith('b') ? 0 : 1 + (id.length % 7)));
    device.queue.writeBuffer(masksBuf, 0, maskData); device.queue.writeBuffer(modsBuf, 0, modData);
    const a = new Float32Array(16); a[3] = masks.length; a[4] = -1; device.queue.writeBuffer(adjBuf, 0, a);
    await frame(5);                                   // warm up
    const per = [];
    for (let b = 0; b < 5; b++) { const t0 = performance.now(); await frame(20); per.push((performance.now() - t0) / 20); }
    per.sort((x, y) => x - y);
    results.push({ scene: name, modifiers: modCount, msPerFrameMedian: +per[2].toFixed(2), min: +per[0].toFixed(2), max: +per[4].toFixed(2) });
  }
  return { adapter: info.description || info.vendor || 'unknown', architecture: info.architecture, frame: `${W}x${H}`, results };
})();

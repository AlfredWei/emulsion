// RFC-0025 slice 1a appendix: render the app's REAL `fs_mask` (the shipped WGSL, via the real
// `packMasks`) on a real GPU and compare it with the CPU path's pixels, for stacks with composable
// modifiers.
//
// How to run:
//   1. cd app/src-tauri && COMPOSABLE_MASK_DUMP=../static/composable_mask_reference.json \
//        cargo test --lib composable_mask_reference_dump -- --ignored
//      (writes the CPU pixels; the file is a throwaway, do not commit it)
//   2. `npm --prefix app run dev`, open http://localhost:1420 in a browser with WebGPU (the Claude
//      desktop Browser pane works), paste this whole file into the devtools console.
// It is an async IIFE returning one JSON object. It does not touch app state.
(async () => {
  const { WGSL } = await import('/src/lib/gpu/shaders/index.js');
  const { packMasks, rasterTargets } = await import('/src/lib/gpu/maskPack.js');
  const { rasterizeDab } = await import('/src/lib/gpu/brushRaster.js');
  const { MAX_MASKS, MAX_MODIFIERS } = await import('/src/lib/gpu/gpuHandles.js');
  const ref = await (await fetch('/composable_mask_reference.json')).json();
  const N = ref.size;

  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const info = adapter.info ?? {};

  const module = device.createShaderModule({ code: WGSL });
  // Shader errors surface only asynchronously and then silently render zeros -- fail loudly.
  const errors = (await module.getCompilationInfo()).messages.filter((m) => m.type === 'error');
  if (errors.length) throw new Error('WGSL compile error: ' + errors.map((m) => `${m.lineNum}: ${m.message}`).join('; '));

  const pipeline = device.createRenderPipeline({
    layout: 'auto',
    vertex: { module, entryPoint: 'vs_main' },
    fragment: { module, entryPoint: 'fs_mask', targets: [{ format: 'rgba8unorm' }] },
    primitive: { topology: 'triangle-list' },
  });

  const mkBuf = (size) => device.createBuffer({ size, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  const adjBuf = mkBuf(64), masksBuf = mkBuf(MAX_MASKS * 16 * 4), modsBuf = mkBuf(MAX_MODIFIERS * 12 * 4), clipBuf = mkBuf(16);
  device.queue.writeBuffer(clipBuf, 0, new Float32Array(4)); // show_clipping = 0
  const brushTex = device.createTexture({ size: [N, N, MAX_MASKS], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  const preTex = device.createTexture({ size: [N, N], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  const out = device.createTexture({ size: [N, N], format: 'rgba8unorm', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC });
  const readBuf = device.createBuffer({ size: N * 256 * Math.ceil(4 * N / 256), usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });

  // The same source image as the Rust dump (dump_image).
  const src = new Uint8Array(N * N * 4);
  for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) {
    const i = (y * N + x) * 4;
    src[i] = x * 4; src[i + 1] = y * 4; src[i + 2] = (x + y) * 2; src[i + 3] = 255;
  }
  device.queue.writeTexture({ texture: preTex }, src, { bytesPerRow: N * 4 }, { width: N, height: N });

  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: sampler },
      { binding: 2, resource: { buffer: adjBuf } },
      { binding: 3, resource: { buffer: masksBuf } },
      { binding: 63, resource: { buffer: modsBuf } },
      { binding: 4, resource: brushTex.createView({ dimension: '2d-array' }) },
      { binding: 26, resource: { buffer: clipBuf } },
      { binding: 27, resource: preTex.createView() },
    ],
  });

  const results = [];
  for (const c of ref.cases) {
    // Rasterize brush layers exactly as syncMaskRasterization does (black init, rasterizeDab per dab).
    const layers = new Map();
    for (const t of rasterTargets(c.ops)) {
      const layer = layers.size;
      layers.set(t.id, layer);
      const canvas = new OffscreenCanvas(N, N);
      const ctx = canvas.getContext('2d');
      ctx.fillStyle = 'black'; ctx.fillRect(0, 0, N, N);
      for (const d of t.dabs) rasterizeDab(ctx, N, N, d);
      device.queue.writeTexture({ texture: brushTex, origin: { x: 0, y: 0, z: layer } }, ctx.getImageData(0, 0, N, N).data, { bytesPerRow: N * 4, rowsPerImage: N }, { width: N, height: N });
    }
    const { maskData, modData } = packMasks(c.ops, (id) => layers.get(id));
    device.queue.writeBuffer(masksBuf, 0, maskData);
    device.queue.writeBuffer(modsBuf, 0, modData);
    const adj = new Float32Array(16);
    adj[3] = c.ops.length; adj[4] = -1; // mask_count, selected_mask_index
    device.queue.writeBuffer(adjBuf, 0, adj);

    const enc = device.createCommandEncoder();
    const pass = enc.beginRenderPass({ colorAttachments: [{ view: out.createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
    pass.setPipeline(pipeline); pass.setBindGroup(0, bind); pass.draw(3); pass.end();
    const bpr = Math.ceil(4 * N / 256) * 256;
    enc.copyTextureToBuffer({ texture: out }, { buffer: readBuf, bytesPerRow: bpr }, { width: N, height: N });
    device.queue.submit([enc.finish()]);
    await readBuf.mapAsync(GPUMapMode.READ);
    const raw = new Uint8Array(readBuf.getMappedRange().slice(0));
    readBuf.unmap();

    let max = 0, sum = 0, n = 0, over2 = 0;
    for (let y = 0; y < N; y++) for (let x = 0; x < N; x++) for (let k = 0; k < 3; k++) {
      const d = Math.abs(raw[y * bpr + x * 4 + k] - c.pixels[(y * N + x) * 3 + k]);
      max = Math.max(max, d); sum += d; n++; if (d > 2) over2++;
    }
    // How much of the image the modifiers actually changed (a probe that cannot tell the cases apart proves nothing).
    let changed = 0;
    for (let i = 0; i < N * N; i++) for (let k = 0; k < 3; k++) if (c.pixels[i * 3 + k] !== src[i * 4 + k]) { changed++; break; }
    results.push({ name: c.name, maxDiff: max, meanDiff: +(sum / n).toFixed(3), channelsOver2: over2, pixelsChangedByCpu: changed });
  }
  return { adapter: info.description || info.vendor || 'unknown', architecture: info.architecture, cases: results };
})();

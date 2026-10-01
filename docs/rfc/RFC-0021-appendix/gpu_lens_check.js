// RFC-0021 appendix: run the app's REAL fs_lens_correct WGSL on a real GPU and
// compare its output pixels with the CPU path's (`apply_lens_correction`).
//
// 1. Generate the CPU reference (a temporary Rust test writes /tmp/lens_ref.json:
//    for four cases -- 24 mm all corrections, 70 mm (out-of-frame corners),
//    24 mm vignetting on a uniform 128 gray, manual distortion+CA -- the
//    `apply_lens_correction` output at 49 pixels each, using the Canon 24-70 profile
//    from lensfun's bundled database). See the RFC's implementation PR for the test.
// 2. Serve it to the dev page:  cp /tmp/lens_ref.json app/static/_lens_ref.json
// 3. `npm --prefix app run dev`, open http://localhost:1420 in a WebGPU browser, paste
//    this file into the devtools console. It prints, per case, the max/mean absolute
//    per-channel difference (0-255) and any pixel more than 4 levels off.
(async () => {
  const cases = await (await fetch('/_lens_ref.json')).json();
  const { WGSL } = await import('/src/lib/gpu/shaders/index.js');
  const dev = await import('/src/lib/api/develop.js');
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const module = device.createShaderModule({ code: WGSL });
  const info = await module.getCompilationInfo();
  const errors = info.messages.filter((m) => m.type === 'error').map((m) => `${m.lineNum}: ${m.message}`);
  if (errors.length) return { compileErrors: errors };

  const pipeline = device.createRenderPipeline({
    layout: 'auto',
    vertex: { module, entryPoint: 'vs_main' },
    fragment: { module, entryPoint: 'fs_lens_correct', targets: [{ format: 'rgba8unorm' }] },
    primitive: { topology: 'triangle-list' },
  });

  const out = { adapter: adapter.info?.architecture, cases: [] };
  for (const c of cases) {
    const { w, h } = c;
    // source image, same integer formulas as the Rust reference
    const src = new Uint8Array(w * h * 4);
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (c.kind === 'gradient') {
        src[i] = Math.floor((x * 255) / (w - 1));
        src[i + 1] = Math.floor((y * 255) / (h - 1));
        src[i + 2] = Math.floor(((x + y) * 255) / (w + h - 2));
      } else { src[i] = src[i + 1] = src[i + 2] = 128; }
      src[i + 3] = 255;
    }
    const tex = device.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
    device.queue.writeTexture({ texture: tex }, src, { bytesPerRow: w * 4 }, [w, h]);
    const target = device.createTexture({ size: [w, h], format: 'rgba8unorm', usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC });

    const lc = dev.getLensCorrection({ schema_version: 1, ops: [{ op: 'lens_correction', ...c.op }] });
    const ubuf = device.createBuffer({ size: 256, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(ubuf, 0, dev.buildLensCorrectionUniformData(lc));
    const bg = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: device.createSampler({ magFilter: 'linear', minFilter: 'linear' }) },
        { binding: 1, resource: tex.createView() },
        { binding: 25, resource: { buffer: ubuf } },
      ],
    });
    const bpr = Math.ceil((w * 4) / 256) * 256;
    const rb = device.createBuffer({ size: bpr * h, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc = device.createCommandEncoder();
    const pass = enc.beginRenderPass({ colorAttachments: [{ view: target.createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
    pass.setPipeline(pipeline); pass.setBindGroup(0, bg); pass.draw(3); pass.end();
    enc.copyTextureToBuffer({ texture: target }, { buffer: rb, bytesPerRow: bpr }, [w, h]);
    device.queue.submit([enc.finish()]);
    await rb.mapAsync(GPUMapMode.READ);
    const data = new Uint8Array(rb.getMappedRange().slice(0));
    rb.unmap();

    let maxDiff = 0, sum = 0, n = 0; const bad = [];
    for (const [x, y, r, g, b] of c.points) {
      const o = y * bpr + x * 4;
      const d = [Math.abs(data[o] - r), Math.abs(data[o + 1] - g), Math.abs(data[o + 2] - b)];
      for (const v of d) { maxDiff = Math.max(maxDiff, v); sum += v; n++; }
      if (Math.max(...d) > 4) bad.push({ x, y, cpu: [r, g, b], gpu: [data[o], data[o + 1], data[o + 2]] });
    }
    out.cases.push({ name: c.name, maxDiff, meanDiff: +(sum / n).toFixed(2), over4: bad.length, firstBad: bad.slice(0, 4) });
  }
  return out;
})()

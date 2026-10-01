// RFC-0022 slices 1-3 appendix: check the app's REAL WGSL grain (frame-relative
// Size + footprint compensation, tonal weight, colour grain) against the CPU on a REAL GPU.
//
// How to run: `npm --prefix app run dev`, open http://localhost:1420 in a
// browser with WebGPU (the Claude desktop Browser pane works; so does Chrome),
// paste this whole file into the devtools console. It is an async IIFE and
// returns one JSON object. It does not touch app state: it imports the shader
// string the app ships (gradeUniforms.js), slices out the grain code
// verbatim, and runs it in a private compute pipeline.
//
// GOLDEN must equal GRAIN_GOLDEN_CASES / GRAIN_GOLDEN_VALUES in
// app/src-tauri/src/develop_engine/tests/effects.rs (regenerate with
// `cargo test --lib grain_reference_dump -- --ignored --nocapture`).
(async () => {
  const { gradeUniforms: src } = await import('/src/lib/gpu/shaders/gradeUniforms.js');
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const info = adapter.info ?? {};

  // The grain block is the tail of the shader string: struct Grain, its
  // uniform binding, the constants, hash/noise, grainDelta, grainDeltaRgb and
  // grainToneWeight (assert the tail ends there, so a later addition cannot
  // be silently sliced in or out).
  const start = src.indexOf('struct Grain');
  const grainWgsl = src.slice(start);
  if (!grainWgsl.includes('fn grainDeltaRgb') || !grainWgsl.includes('fn grainToneWeight') || /fn (?!grain|lowbias32|particleHash)/.test(grainWgsl.replace(/fn (lowbias32|particleHash)/g, ''))) {
    throw new Error('grain block is not the tail of gradeUniforms.js; update the probe slice');
  }

  const wgsl = `
    ${grainWgsl}
    struct Job { n: u32, ox: u32, oy: u32, mode: u32, longEdge: f32, a: u32, b: u32, c: u32 };
    @group(0) @binding(0) var<storage, read_write> outv: array<f32>;
    @group(0) @binding(1) var<uniform> job: Job;
    @group(0) @binding(2) var<storage, read> inv: array<f32>;
    @compute @workgroup_size(8, 8)
    fn main(@builtin(global_invocation_id) id: vec3<u32>) {
      if (id.x >= job.n || id.y >= job.n) { return; }
      let i = id.y * job.n + id.x;
      if (job.mode == 1u) {            // mode 1: grainToneWeight at supplied lumas
        outv[i] = grainToneWeight(inv[i]);
        return;
      }
      let c = vec2<f32>(f32(id.x + job.ox), f32(id.y + job.oy));
      if (job.mode >= 2u) {            // mode 2/3/4: channel R/G/B of grainDeltaRgb
        let d = grainDeltaRgb(c, job.longEdge);
        outv[i] = select(select(d.b, d.g, job.mode == 3u), d.r, job.mode == 2u);
        return;
      }
      outv[i] = grainDelta(c, job.longEdge);
    }`;
  const module = device.createShaderModule({ code: wgsl });
  // WebGPU reports shader errors only asynchronously and then silently produces
  // zeros -- fail loudly instead (this caught `shared`, a reserved WGSL word).
  const errors = (await module.getCompilationInfo()).messages.filter((m) => m.type === 'error');
  if (errors.length) throw new Error('WGSL compile error: ' + errors.map((m) => `${m.lineNum}: ${m.message}`).join('; '));
  const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module, entryPoint: 'main' } });

  async function run({ n, longEdge = 2048, amount = 100, size = 25, roughness = 50, tone = 0, chroma = 0, ox = 0, oy = 0, mode = 0, input = new Float32Array(4) }) {
    const out = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const rd = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const g = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(g, 0, new Float32Array([amount, size, roughness, tone, chroma, 0, 0, 0]));
    const j = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const jb = new ArrayBuffer(32);
    new Uint32Array(jb, 0, 4).set([n, ox, oy, mode]);
    new Float32Array(jb, 16, 1).set([longEdge]);
    device.queue.writeBuffer(j, 0, jb);
    const inb = device.createBuffer({ size: Math.max(16, input.byteLength), usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(inb, 0, input);
    const bg = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: out } },
        { binding: 1, resource: { buffer: j } },
        { binding: 2, resource: { buffer: inb } },
        { binding: 16, resource: { buffer: g } },
      ],
    });
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bg);
    pass.dispatchWorkgroups(Math.ceil(n / 8), Math.ceil(n / 8)); pass.end();
    enc.copyBufferToBuffer(out, 0, rd, 0, n * n * 4);
    device.queue.submit([enc.finish()]);
    await rd.mapAsync(GPUMapMode.READ);
    const res = new Float32Array(rd.getMappedRange().slice(0));
    rd.unmap();
    return res;
  }

  // [longEdge, size, roughness, x, y, expected CPU value]
  const GOLDEN = [
    [6000, 25, 50, 123, 456, 0.042875215],
    [2048, 25, 50, 700, 31, -0.011109176],
    [2048, 0, 0, 5, 9, -0.026815541],
    [2048, 100, 100, 1500, 1000, -0.042142715],
    [6144, 60, 30, 3001, 777, 0.025983753],
    [1000, 25, 50, 10, 10, -0.016688382],
    [2048, 40, 100, 333, 222, 0.00022242409],
    [4000, 10, 0, 1234, 2345, 0.050142936],
  ];
  const golden = [];
  for (const [longEdge, size, roughness, x, y, want] of GOLDEN) {
    const v = (await run({ n: 1, longEdge, size, roughness, ox: x, oy: y }))[0];
    golden.push({ longEdge, size, roughness, x, y, gpu: v, cpu: want, absDiff: Math.abs(v - want) });
  }

  // Slice 2: grainToneWeight against the Rust hand values (tests/effects.rs
  // grain_tone_weight_hand_values); [tone, luma, expected weight].
  const WEIGHTS = [
    [100, 0, 0], [100, 0.25, 0.75], [100, 0.5, 1], [100, 1, 0],
    [50, 0, 0.5], [50, 0.25, 0.875], [50, 0.5, 1], [50, 1, 0.5],
    [25, 0.1, 0.75 + 0.25 * 0.36], [100, -0.3, 0], [100, 1.4, 0], [0, 0.3, 1],
  ];
  const weights = [];
  for (const [tone, luma, want] of WEIGHTS) {
    const v = (await run({ n: 1, tone, mode: 1, input: new Float32Array([luma, 0, 0, 0]) }))[0];
    weights.push({ tone, luma, gpu: v, expected: want, absDiff: Math.abs(v - want) });
  }

  // Slice 3: grainDeltaRgb against the Rust golden table (GRAIN_RGB_GOLDEN_CASES /
  // GRAIN_RGB_GOLDEN_VALUES); [longEdge, size, roughness, chroma, x, y, [r, g, b]].
  const RGB_GOLDEN = [
    [6000, 25, 50, 100, 123, 456, [0.022601508, 0.09157415, -0.10566195]],
    [6000, 25, 50, 80, 123, 456, [0.03938978, 0.10108078, -0.075332545]],
    [2048, 25, 50, 50, 700, 31, [0.02536841, -0.019924292, -0.049818635]],
    [2048, 100, 100, 100, 1500, 1000, [0.0054202266, -0.0062589725, -0.04917158]],
    [6144, 60, 30, 20, 3001, 777, [0.032594442, 0.042263743, 0.014614766]],
    [2048, 0, 0, 100, 5, 9, [0.012423921, 0.0022487536, -0.00325335]],
    [4000, 10, 0, 65, 1234, 2345, [0.042844504, 0.002495056, 0.020142565]],
  ];
  const rgbGolden = [];
  for (const [longEdge, size, roughness, chroma, x, y, want] of RGB_GOLDEN) {
    const got = [];
    for (const mode of [2, 3, 4]) got.push((await run({ n: 1, longEdge, size, roughness, chroma, ox: x, oy: y, mode }))[0]);
    rgbGolden.push({ longEdge, size, roughness, chroma, x, y, gpu: got, cpu: want, absDiff: Math.max(...got.map((v, k) => Math.abs(v - want[k]))) });
  }

  // Colour-grain statistics on the GPU: per-channel std and R-G correlation at
  // chroma 0 / 50 / 100 over a 256 x 256 patch of a 24 MP frame (want std
  // ~0.0511 each, corr 1 - chroma/100).
  const stat = async (chroma) => {
    const ch = [];
    for (const mode of [2, 3]) ch.push(await run({ n: 256, longEdge: 6000, chroma, mode, ox: 300, oy: 40 }));
    const m = (a) => a.reduce((p, q) => p + q, 0) / a.length;
    const [ma, mb] = [m(ch[0]), m(ch[1])];
    let va = 0, vb = 0, cab = 0;
    for (let k = 0; k < ch[0].length; k++) { const a = ch[0][k] - ma, b = ch[1][k] - mb; va += a * a; vb += b * b; cab += a * b; }
    return { chroma, stdR: Math.sqrt(va / ch[0].length), stdG: Math.sqrt(vb / ch[1].length), corrRG: cab / Math.sqrt(va * vb) };
  };
  const chromaStats = [await stat(0), await stat(50), await stat(100)];

  // Cost on the GPU: a 2048 x 2048 (4.2 MP) compute pass of grainDeltaRgb,
  // 20 dispatches in one submission, wall-clock until the queue drains, no
  // readback (a compute stand-in for the premask fragment pass; it includes
  // the 16 MB storage write the real pass does not have).
  const timeIt = async (chroma) => {
    const n = 2048;
    const out = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.STORAGE });
    const g = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(g, 0, new Float32Array([100, 25, 50, 0, chroma, 0, 0, 0]));
    const j = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const jb = new ArrayBuffer(32);
    new Uint32Array(jb, 0, 4).set([n, 0, 0, 2]);
    new Float32Array(jb, 16, 1).set([2048]);
    device.queue.writeBuffer(j, 0, jb);
    const inb = device.createBuffer({ size: 16, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
    const bg = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [
      { binding: 0, resource: { buffer: out } }, { binding: 1, resource: { buffer: j } },
      { binding: 2, resource: { buffer: inb } }, { binding: 16, resource: { buffer: g } } ] });
    const submit = (reps) => {
      const enc = device.createCommandEncoder();
      for (let k = 0; k < reps; k++) {
        const pass = enc.beginComputePass();
        pass.setPipeline(pipeline); pass.setBindGroup(0, bg);
        pass.dispatchWorkgroups(n / 8, n / 8); pass.end();
      }
      device.queue.submit([enc.finish()]);
      return device.queue.onSubmittedWorkDone();
    };
    await submit(2); // warm-up
    const t0 = performance.now();
    await submit(20);
    return (performance.now() - t0) / 20;
  };
  const timing = { chroma0_msPer4MP: await timeIt(0), chroma50_msPer4MP: await timeIt(50), chroma100_msPer4MP: await timeIt(100) };

  // Calibration on the GPU itself: std at Amount 100, default Size/Roughness,
  // 24 MP frame -> the pre-RFC-0020 measured 0.0511.
  const N = 256;
  const field = await run({ n: N, longEdge: 6000 });
  const mean = field.reduce((a, b) => a + b, 0) / field.length;
  const std = Math.sqrt(field.reduce((a, b) => a + (b - mean) ** 2, 0) / field.length);

  // Preview vs export on the GPU: 2048-px frame vs a 3x (6144-px) frame
  // box-downsampled to the same grid, per-pixel std ratio (RFC-0022 §3.4).
  const M = 64, F = 3, ox = 200, oy = 120, size = 25;
  const lo = await run({ n: M, longEdge: 2048, size, ox, oy });
  const hiN = M * F;
  const hi = await run({ n: hiN, longEdge: 2048 * F, size, ox: ox * F, oy: oy * F });
  const avg = new Float32Array(M * M);
  for (let y = 0; y < M; y++) for (let x = 0; x < M; x++) {
    let a = 0;
    for (let dy = 0; dy < F; dy++) for (let dx = 0; dx < F; dx++) a += hi[(y * F + dy) * hiN + x * F + dx];
    avg[y * M + x] = a / (F * F);
  }
  const sd = (a) => { const m = a.reduce((p, q) => p + q, 0) / a.length; return Math.sqrt(a.reduce((p, q) => p + (q - m) ** 2, 0) / a.length); };

  return JSON.stringify({
    adapter: `${info.vendor ?? ''} ${info.architecture ?? ''} ${info.description ?? ''}`.trim(),
    maxGoldenAbsDiff: Math.max(...golden.map((g) => g.absDiff)),
    golden,
    maxRgbGoldenAbsDiff: Math.max(...rgbGolden.map((g) => g.absDiff)),
    rgbGolden,
    chromaStats,
    timing,
    maxToneWeightAbsDiff: Math.max(...weights.map((w) => w.absDiff)),
    weights,
    stdAt24MpDefaults: std,
    previewVsExport: { previewStd: sd(lo), downsampledExportStd: sd(avg), ratio: sd(avg) / sd(lo) },
  }, null, 1);
})();

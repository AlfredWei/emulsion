// RFC-0022 slice 1 appendix: check the app's REAL WGSL grain (frame-relative
// Size + footprint compensation) against the CPU on a REAL GPU.
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

  const start = src.indexOf('struct Grain');
  const fnAt = src.indexOf('fn grainDelta');
  let i = src.indexOf('{', fnAt), depth = 0;
  for (; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}' && --depth === 0) break;
  }
  const grainWgsl = src.slice(start, i + 1);

  const wgsl = `
    ${grainWgsl}
    struct Job { n: u32, ox: u32, oy: u32, longEdge: f32 };
    @group(0) @binding(0) var<storage, read_write> outv: array<f32>;
    @group(0) @binding(1) var<uniform> job: Job;
    @compute @workgroup_size(8, 8)
    fn main(@builtin(global_invocation_id) id: vec3<u32>) {
      if (id.x >= job.n || id.y >= job.n) { return; }
      let c = vec2<f32>(f32(id.x + job.ox), f32(id.y + job.oy));
      outv[id.y * job.n + id.x] = grainDelta(c, job.longEdge);
    }`;
  const module = device.createShaderModule({ code: wgsl });
  const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module, entryPoint: 'main' } });

  async function run({ n, longEdge, amount = 100, size = 25, roughness = 50, ox = 0, oy = 0 }) {
    const out = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const rd = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const g = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(g, 0, new Float32Array([amount, size, roughness, 0]));
    const j = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const jb = new ArrayBuffer(16);
    new Uint32Array(jb, 0, 3).set([n, ox, oy]);
    new Float32Array(jb, 12, 1).set([longEdge]);
    device.queue.writeBuffer(j, 0, jb);
    const bg = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: out } },
        { binding: 1, resource: { buffer: j } },
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
    stdAt24MpDefaults: std,
    previewVsExport: { previewStd: sd(lo), downsampledExportStd: sd(avg), ratio: sd(avg) / sd(lo) },
  }, null, 1);
})();

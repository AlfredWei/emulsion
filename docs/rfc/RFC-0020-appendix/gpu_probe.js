// RFC-0020 appendix: measure the app's REAL WGSL grain on a REAL GPU.
//
// How to run: `npm --prefix app run dev`, open http://localhost:1420 in a
// browser with WebGPU (the Claude desktop Browser pane works; so does Chrome),
// then paste this whole file into the devtools console (it is an async IIFE,
// so top-level await is not needed). It prints/returns one JSON object.
//
// It does not touch app state: it imports the shader-source string the app
// itself ships (gradeUniforms.js), slices out the grain functions unmodified,
// and runs them in a private compute pipeline. Re-run it after the RFC's
// implementation lands: the same code then measures the NEW WGSL, because it
// slices whatever `grainDelta` is in that file at that time.
(async () => {
  const { gradeUniforms: src } = await import('/src/lib/gpu/shaders/gradeUniforms.js');
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const info = adapter.info ?? {};

  // ---- 1. slice the grain code out of the app's shader string, verbatim -----
  const start = src.indexOf('struct Grain');
  const fnAt = src.indexOf('fn grainDelta');
  let i = src.indexOf('{', fnAt), depth = 0;
  for (; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}' && --depth === 0) break;
  }
  const grainWgsl = src.slice(start, i + 1);   // struct, uniform @binding(16), consts, hash, noise, grainDelta

  // ---- 2. a compute pass: N x N grid of grainDelta values (+ optional raw hash)
  const wgsl = `
    ${grainWgsl}
    struct Job { n: u32, mode: u32, ox: u32, oy: u32 };
    @group(0) @binding(0) var<storage, read_write> outv: array<f32>;
    @group(0) @binding(1) var<uniform> job: Job;
    @group(0) @binding(2) var<storage, read> inv: array<f32>;
    @compute @workgroup_size(8, 8)
    fn main(@builtin(global_invocation_id) id: vec3<u32>) {
      if (id.x >= job.n || id.y >= job.n) { return; }
      let c = vec2<f32>(f32(id.x + job.ox), f32(id.y + job.oy));
      var v: f32;
      if (job.mode == 0u) { v = grainDelta(c); }                     // the whole effect
      else if (job.mode == 1u) { v = grainHash(c); }                  // hash on integer coords
      else { v = sin(inv[id.y * job.n + id.x]); }                     // mode 2: bare sin() of supplied args
      outv[id.y * job.n + id.x] = v;
    }`;
  const module = device.createShaderModule({ code: wgsl });
  const pipeline = device.createComputePipeline({ layout: 'auto', compute: { module, entryPoint: 'main' } });

  async function run({ n, mode, amount = 100, size = 25, roughness = 50, ox = 0, oy = 0, input = new Float32Array(1) }) {
    const out = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const rd = device.createBuffer({ size: n * n * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const g = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(g, 0, new Float32Array([amount, size, roughness, 0]));
    const j = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(j, 0, new Uint32Array([n, mode, ox, oy]));
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
    const data = new Float32Array(rd.getMappedRange().slice(0));
    rd.unmap();
    return data;
  }

  const out = { adapter: { vendor: info.vendor, architecture: info.architecture, description: info.description } };
  const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[s.length >> 1]; };
  const pct = (a, p) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(p * (s.length - 1))]; };

  // ---- 3. §1.1 -- GPU sin() error and CPU<->GPU hash agreement vs coordinate --
  // (LEGACY sin hash; after the RFC lands this section is skipped -- the
  // integer hash has no sin -- and the golden-vector check replaces it.)
  const f32 = Math.fround;
  const hashArg = (x, y) => f32(f32(x * f32(12.9898)) + f32(y * f32(78.233)));   // Rust: separate mul, add
  const cpuSinHash = (x, y) => {                      // accurate-sin, float32-rounded, as Rust does
    const v = f32(f32(Math.sin(hashArg(x, y))) * f32(43758.5453123));
    return v - Math.floor(v);
  };
  if (src.includes('43758.5453123')) {
    out.hashVsCoordinate = [];
    for (const maxc of [2, 16, 256, 4096]) {
      const n = 128, diffs = [], sinErr = [];
      for (let k = 0; k < 6; k++) {
        // random window of lattice coords, all < maxc (for maxc < n: every coord in [0,maxc))
        const lim = Math.min(n, maxc);
        const ox = Math.floor(Math.random() * Math.max(1, maxc - lim + 1)), oy = Math.floor(Math.random() * Math.max(1, maxc - lim + 1));
        const gpuHash = await run({ n, mode: 1, ox, oy });
        const args = new Float32Array(n * n);
        for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) args[y * n + x] = hashArg(x + ox, y + oy);
        const gpuSin = await run({ n, mode: 2, input: args });
        for (let y = 0; y < lim; y++) for (let x = 0; x < lim; x++) {
          const d = Math.abs(gpuHash[y * n + x] - cpuSinHash(x + ox, y + oy));
          diffs.push(Math.min(d, 1 - d));               // wrapped difference on [0,1)
          sinErr.push(Math.abs(gpuSin[y * n + x] - Math.sin(args[y * n + x])));
        }
      }
      out.hashVsCoordinate.push({
        maxCoordApprox: maxc,
        gpuSinAbsErr: { median: +median(sinErr).toPrecision(2), p99: +pct(sinErr, 0.99).toPrecision(2) },
        hashDiff: { median: +median(diffs).toFixed(3), p99: +pct(diffs, 0.99).toFixed(3) },  // 0.25 == two independent uniforms
      });
    }
  }

  // ---- 4. §1.2 -- the repeat: max |autocorrelation| at lags >= 12 px ---------
  function autocorrMax(f, n, minLag = 12, maxLag = 60) {
    let mean = 0; for (const v of f) mean += v; mean /= f.length;
    const g = Float64Array.from(f, (v) => v - mean);
    let varr = 0; for (const v of g) varr += v * v; varr /= g.length;
    let best = { v: 0, lag: null };
    for (let dy = 0; dy <= maxLag; dy++) for (let dx = -maxLag; dx <= maxLag; dx++) {
      if (dy === 0 && dx <= 0) continue;
      if (Math.hypot(dx, dy) < minLag) continue;
      let s = 0, cnt = 0;
      const x0 = Math.max(0, -dx), x1 = Math.min(n, n - dx);
      for (let y = 0; y < n - dy; y++) {
        const r0 = y * n, r1 = (y + dy) * n + dx;
        for (let x = x0; x < x1; x++) { s += g[r0 + x] * g[r1 + x]; cnt++; }
      }
      const c = s / cnt / varr;
      if (Math.abs(c) > Math.abs(best.v)) best = { v: +c.toFixed(3), lag: [dx, dy] };
    }
    return best;
  }
  out.repeat = [];
  for (const size of [0, 25, 50]) {
    const f = await run({ n: 256, mode: 0, amount: 100, size, roughness: 50 });
    let mean = 0, m2 = 0; for (const v of f) mean += v; mean /= f.length; for (const v of f) m2 += (v - mean) ** 2;
    out.repeat.push({ size, deltaStd: +Math.sqrt(m2 / f.length).toFixed(4), maxAutocorr: autocorrMax(f, 256) });
  }
  return out;
})()

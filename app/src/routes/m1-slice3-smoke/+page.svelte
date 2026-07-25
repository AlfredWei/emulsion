<script>
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  // Throwaway manual verification for M1 Slice 3 (Develop pipeline) --
  // same pattern as /m0-spike, /m1-smoke, /m1-slice2-smoke. Two things
  // here, matching the approved plan's verification section:
  //   1. Backend wiring: import a real RAW file, decode a Develop preview,
  //      confirm the PNG is real and fetchable, round-trip an edit stack
  //      through the catalog.
  //   2. Shader correctness: the exact WGSL DevelopCanvas.svelte ships,
  //      run against a synthetic flat-color texture with known adjustment
  //      values, read back a pixel, and compare against hand-computed
  //      expected output -- not just "it rendered something".

  let status = $state("running...");
  let detail = $state("");

  // Copied verbatim from DevelopCanvas.svelte -- this test exists to catch
  // bugs in that exact shader, so it must not be a reimplementation that
  // could silently diverge from what ships.
  const WGSL = `
    struct VertexOut {
      @builtin(position) position: vec4<f32>,
      @location(0) uv: vec2<f32>,
    };

    @vertex
    fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
      var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
      );
      var out: VertexOut;
      out.position = vec4<f32>(pos[i], 0.0, 1.0);
      out.uv = vec2<f32>((pos[i].x + 1.0) * 0.5, (1.0 - pos[i].y) * 0.5);
      return out;
    }

    struct Adjustments {
      exposure_ev: f32,
      contrast: f32,
      saturation: f32,
      _pad: f32,
    };

    @group(0) @binding(0) var srcSampler: sampler;
    @group(0) @binding(1) var srcTexture: texture_2d<f32>;
    @group(0) @binding(2) var<uniform> adj: Adjustments;

    @fragment
    fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
      var rgb = textureSample(srcTexture, srcSampler, in.uv).rgb;

      rgb = rgb * pow(2.0, adj.exposure_ev);
      rgb = (rgb - 0.5) * (1.0 + adj.contrast / 100.0) + 0.5;

      let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
      rgb = mix(vec3<f32>(luma), rgb, 1.0 + adj.saturation / 100.0);

      rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
      return vec4<f32>(rgb, 1.0);
    }
  `;

  async function verifyBackendWiring(/** @type {Record<string, any>} */ report) {
    const params = new URLSearchParams(window.location.search);
    const dir = params.get("dir");
    if (!dir) throw new Error("missing ?dir= query param");

    await invoke("import_folder", { path: dir });
    const images = await invoke("list_images");
    if (images.length === 0) throw new Error("no images found after import");
    const image = images[0];

    const preview = await invoke("get_develop_preview", { path: image.path });
    report.previewInfo = preview;

    const resp = await fetch(convertFileSrc(preview.path));
    if (!resp.ok) throw new Error(`preview PNG fetch failed: HTTP ${resp.status}`);
    const bitmap = await createImageBitmap(await resp.blob());
    report.bitmapDims = { width: bitmap.width, height: bitmap.height };
    report.dimsMatchReportedPreviewInfo =
      bitmap.width === preview.width && bitmap.height === preview.height;

    const testStack = {
      schema_version: 1,
      ops: [
        { op: "exposure", value: 0.5 },
        { op: "contrast", value: 10 },
        { op: "saturation", value: 30 },
      ],
    };
    await invoke("set_edit_stack", { versionId: image.version_id, stack: testStack });
    const roundTripped = await invoke("get_edit_stack", { versionId: image.version_id });
    report.roundTrippedStack = roundTripped;
    report.editStackRoundTrips = JSON.stringify(roundTripped) === JSON.stringify(testStack);
  }

  async function verifyShaderCorrectness(/** @type {Record<string, any>} */ report) {
    if (!("gpu" in navigator)) throw new Error("navigator.gpu is undefined in this webview");
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("requestAdapter() returned null");
    const device = await adapter.requestDevice();

    const size = 64; // 64 * 4 bytes/px = 256 = WebGPU's minimum bytesPerRow alignment
    const inputByte = { r: 153, g: 51, b: 51 }; // exact bytes: 0.6, 0.2, 0.2

    const srcTexture = device.createTexture({
      size: [size, size],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    const rowData = new Uint8Array(size * 4);
    for (let x = 0; x < size; x++) {
      rowData.set([inputByte.r, inputByte.g, inputByte.b, 255], x * 4);
    }
    const data = new Uint8Array(size * size * 4);
    for (let y = 0; y < size; y++) data.set(rowData, y * size * 4);
    device.queue.writeTexture(
      { texture: srcTexture },
      data,
      { bytesPerRow: size * 4 },
      [size, size],
    );

    const outTexture = device.createTexture({
      size: [size, size],
      format: "rgba8unorm",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });

    const module = device.createShaderModule({ code: WGSL });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs_main" },
      fragment: { module, entryPoint: "fs_main", targets: [{ format: "rgba8unorm" }] },
      primitive: { topology: "triangle-list" },
    });

    const sampler = device.createSampler({ magFilter: "linear", minFilter: "linear" });
    const uniformBuffer = device.createBuffer({
      size: 16,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    const exposureEv = 0.5, contrast = 10, saturation = 30;
    device.queue.writeBuffer(
      uniformBuffer,
      0,
      new Float32Array([exposureEv, contrast, saturation, 0]),
    );

    const bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: srcTexture.createView() },
        { binding: 2, resource: { buffer: uniformBuffer } },
      ],
    });

    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: outTexture.createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(3);
    pass.end();

    const bytesPerRow = size * 4;
    const readbackBuffer = device.createBuffer({
      size: bytesPerRow * size,
      usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });
    encoder.copyTextureToBuffer({ texture: outTexture }, { buffer: readbackBuffer, bytesPerRow }, [
      size,
      size,
    ]);
    device.queue.submit([encoder.finish()]);
    await device.queue.onSubmittedWorkDone();

    await readbackBuffer.mapAsync(GPUMapMode.READ);
    const pixels = new Uint8Array(readbackBuffer.getMappedRange());
    const centerIdx = (Math.floor(size / 2) * size + Math.floor(size / 2)) * 4;
    const actual = {
      r: pixels[centerIdx],
      g: pixels[centerIdx + 1],
      b: pixels[centerIdx + 2],
      a: pixels[centerIdx + 3],
    };
    readbackBuffer.unmap();

    // Hand-computed: base (0.6, 0.2, 0.2) -> *2^0.5 -> contrast(+10) ->
    // saturation(+30), then clamp to [0,1]. See m1-slice3-smoke README math
    // in the plan/PR description for the derivation.
    const expected = { r: 255, g: 56, b: 56 };
    report.shaderTest = {
      input: inputByte,
      adjustments: { exposure_ev: exposureEv, contrast, saturation },
      expected,
      actual,
      matches:
        Math.abs(actual.r - expected.r) <= 2 &&
        Math.abs(actual.g - expected.g) <= 2 &&
        Math.abs(actual.b - expected.b) <= 2 &&
        actual.a === 255,
    };
  }

  async function run() {
    /** @type {Record<string, any>} */
    const report = { error: null };
    try {
      await verifyBackendWiring(report);
      await verifyShaderCorrectness(report);
    } catch (/** @type {any} */ e) {
      report.error = String(e && e.stack ? e.stack : e);
    }

    status = report.error ? "FAILED" : "DONE";
    detail = JSON.stringify(report, null, 2);
    await invoke("report_spike_result", { resultJson: JSON.stringify(report) });
  }

  onMount(() => {
    run();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem; white-space: pre-wrap;">
  <h1>M1 Slice 3 smoke test</h1>
  <p>Status: {status}</p>
  <pre>{detail}</pre>
</main>

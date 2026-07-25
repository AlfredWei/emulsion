<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let status = $state("running...");
  let detail = $state("");

  // Mirrors the "decode once, edit reactively" split from ADR-0004:
  // this shader stands in for a real Develop adjustment (exposure),
  // applied in WGSL to a stand-in "decoded pixel" value, entirely
  // inside the webview process — no IPC round trip per edit.
  const WGSL = `
    struct VertexOut {
      @builtin(position) position: vec4<f32>,
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
      return out;
    }

    @fragment
    fn fs_main() -> @location(0) vec4<f32> {
      // Stand-in for a decoded linear-light RAW sample.
      let base = vec3<f32>(0.3, 0.3, 0.3);
      // Stand-in for a hardcoded Develop "exposure +1EV" adjustment.
      let exposure_ev = 1.0;
      let adjusted = base * pow(2.0, exposure_ev);
      return vec4<f32>(adjusted, 1.0);
    }
  `;

  async function runSpike() {
    /** @type {Record<string, any>} */
    const report = {
      hasNavigatorGpu: false,
      adapter: null,
      deviceAcquired: false,
      renderSubmitted: false,
      readback: null,
      expected: 153, // round(0.6 * 255), see WGSL: 0.3 * 2^1 = 0.6
      colorCorrect: false,
      error: null,
    };

    try {
      if (!("gpu" in navigator)) {
        throw new Error("navigator.gpu is undefined in this webview");
      }
      report.hasNavigatorGpu = true;

      const adapter = await navigator.gpu.requestAdapter();
      if (!adapter) throw new Error("requestAdapter() returned null");
      report.adapter = adapter.info
        ? JSON.stringify(adapter.info)
        : "adapter-acquired-no-info";

      const device = await adapter.requestDevice();
      report.deviceAcquired = true;

      const size = 64; // 64 * 4 bytes/px = 256 = WebGPU's minimum bytesPerRow alignment
      const texture = device.createTexture({
        size: [size, size],
        format: "rgba8unorm",
        usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
      });

      const module = device.createShaderModule({ code: WGSL });
      const pipeline = device.createRenderPipeline({
        layout: "auto",
        vertex: { module, entryPoint: "vs_main" },
        fragment: {
          module,
          entryPoint: "fs_main",
          targets: [{ format: "rgba8unorm" }],
        },
        primitive: { topology: "triangle-list" },
      });

      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: texture.createView(),
            clearValue: { r: 0, g: 0, b: 0, a: 1 },
            loadOp: "clear",
            storeOp: "store",
          },
        ],
      });
      pass.setPipeline(pipeline);
      pass.draw(3);
      pass.end();

      const bytesPerRow = size * 4;
      const readbackBuffer = device.createBuffer({
        size: bytesPerRow * size,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
      });
      encoder.copyTextureToBuffer(
        { texture },
        { buffer: readbackBuffer, bytesPerRow },
        [size, size],
      );

      device.queue.submit([encoder.finish()]);
      await device.queue.onSubmittedWorkDone();
      report.renderSubmitted = true;

      await readbackBuffer.mapAsync(GPUMapMode.READ);
      const pixels = new Uint8Array(readbackBuffer.getMappedRange());
      const r = pixels[0], g = pixels[1], b = pixels[2], a = pixels[3];
      readbackBuffer.unmap();

      report.readback = { r, g, b, a };
      report.colorCorrect = Math.abs(r - report.expected) <= 1
        && Math.abs(g - report.expected) <= 1
        && Math.abs(b - report.expected) <= 1
        && a === 255;
    } catch (/** @type {any} */ e) {
      report.error = String(e && e.stack ? e.stack : e);
    }

    status = report.error ? "FAILED" : "DONE";
    detail = JSON.stringify(report, null, 2);

    try {
      await invoke("report_spike_result", { resultJson: JSON.stringify(report) });
    } catch (e) {
      // invoke itself failing is its own useful signal, surface it too
      detail += `\n\ninvoke() failed: ${e}`;
    }
  }

  onMount(() => {
    runSpike();
  });
</script>

<main style="font-family: ui-monospace, monospace; padding: 2rem; white-space: pre-wrap;">
  <h1>M0 spike: in-webview WebGPU</h1>
  <p>Status: {status}</p>
  <pre>{detail}</pre>
</main>

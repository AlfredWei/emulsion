<script>
  // Library-mode histogram. Unlike DevelopCanvas.svelte's own live
  // histogram (fed by a real GPU render pass over the full-resolution
  // graded image), there's no GPU pipeline running in Library mode to
  // read back from -- this instead decodes the selected image's own
  // cataloged thumbnail file and bins ITS pixels directly via a 2D
  // canvas. The thumbnail is content-addressed and regenerated on every
  // edit (see import.rs's regenerate_edited_thumbnail), so this tracks
  // the current develop state, just at thumbnail resolution rather than
  // full-res -- a named, accepted scope cut, not an oversight.

  import { convertFileSrc } from "@tauri-apps/api/core";
  import Histogram from "$lib/components/Histogram.svelte";
  import { binHistogramPixels } from "$lib/histogramMath.js";

  /** @type {{ thumbnailPath: string | null }} */
  let { thumbnailPath } = $props();

  const SAMPLE_SIZE = 256;

  let histogramData = $state(
    /** @type {{r: Uint32Array, g: Uint32Array, b: Uint32Array} | null} */ (null),
  );

  $effect(() => {
    const path = thumbnailPath;
    if (!path) {
      histogramData = null;
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const response = await fetch(convertFileSrc(path));
        const bitmap = await createImageBitmap(await response.blob());
        if (cancelled) return;
        const canvas = document.createElement("canvas");
        canvas.width = SAMPLE_SIZE;
        canvas.height = SAMPLE_SIZE;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        // Stretched to a fixed square sample, same as the Develop
        // histogram's own fixed 256x256 GPU render target -- a histogram
        // only cares about per-pixel VALUE distribution, not spatial
        // layout, so the aspect-ratio distortion this introduces doesn't
        // affect the result.
        ctx.drawImage(bitmap, 0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
        const { data } = ctx.getImageData(0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
        if (cancelled) return;
        // Canvas ImageData is always RGBA byte order (unlike WebGPU's
        // presentationFormat, which can be bgra8unorm) -- no order
        // ambiguity to resolve here, unlike DevelopCanvas's own call site.
        histogramData = binHistogramPixels(new Uint8Array(data), "rgba");
      } catch {
        if (!cancelled) histogramData = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<Histogram data={histogramData} />

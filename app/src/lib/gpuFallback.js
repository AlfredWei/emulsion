// Pure classification for a WebGPU initialization failure -- kept separate
// from DevelopCanvas.svelte's async device/adapter orchestration so "which
// fallback reason is this" is independently testable, matching this
// project's stepMath.js/cropMath.js/histogramMath.js pure-module convention.

/**
 * @typedef {Object} GpuFallbackInfo
 * @property {"no-webgpu" | "adapter-unavailable" | "device-request-failed"} reason
 * @property {string} message
 */

/** Maps one of `initGpu`'s own three throw/rejection sites
 * (DevelopCanvas.svelte) to a user-facing reason/message pair. Falls back to
 * "device-request-failed" for anything unrecognized -- covers
 * `adapter.requestDevice()`'s own rejection, whose message text isn't
 * standardized across WebGPU implementations, but is still an accurate
 * label since by that point `navigator.gpu` and an adapter were both
 * already confirmed present.
 * @returns {GpuFallbackInfo} */
export function classifyGpuFailure(/** @type {unknown} */ error) {
  const raw = error && typeof error === "object" && "message" in error
    ? String(/** @type {{message: unknown}} */ (error).message)
    : String(error);
  if (raw.includes("navigator.gpu is undefined")) {
    return { reason: "no-webgpu", message: "This webview doesn't support WebGPU." };
  }
  if (raw.includes("requestAdapter() returned null")) {
    return { reason: "adapter-unavailable", message: "No compatible GPU adapter was found." };
  }
  return { reason: "device-request-failed", message: raw || "The GPU device could not be initialized." };
}

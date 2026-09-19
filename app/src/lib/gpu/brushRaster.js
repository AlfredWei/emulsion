// Canvas2D rasterization of brush and spot-heal dabs into per-mask weight canvases.
// Moved verbatim out of DevelopCanvas.svelte; each function draws onto the context it
// is handed and reads no component state.

/** Draws ONE dab onto a persistent per-mask OffscreenCanvas, white-on-
 * black (luminance-as-weight), matching develop_engine.rs's
 * `dab_falloff`/`brush_mask_weight` exactly so the GPU preview and CPU
 * export agree: "add" dabs use `"lighter"` compositing (additive,
 * clamped at full white -- matches the CPU side's
 * `(weight + falloff).min(1.0)`); "erase" dabs use `"multiply"`
 * compositing with a gradient from `(1-flow)` at the dab's center to
 * `1.0` at its edge (matches the CPU side's `weight *= 1.0 - falloff`).
 * `radius`/`hardness`/`flow` are baked into the dab itself at paint time
 * (the current brush tool settings when it was placed), not read from
 * live props here. */
export function rasterizeDab(
  /** @type {OffscreenCanvasRenderingContext2D} */ ctx,
  /** @type {number} */ canvasWidth,
  /** @type {number} */ canvasHeight,
  /** @type {import('$lib/api/develop.js').Dab} */ dab,
) {
  const cx = dab.x * canvasWidth;
  const cy = dab.y * canvasHeight;
  // radius is a fraction of WIDTH only -- ctx.arc()'s single radius
  // parameter then produces a true circle in this canvas's own native
  // pixel space regardless of the image's aspect ratio, unlike the
  // radial mask's separate radiusX/radiusY (needed there because that
  // geometry is evaluated analytically in normalized UV space, where
  // width/height asymmetry genuinely matters).
  const r = dab.radius * canvasWidth;
  const hardStop = Math.min(Math.max(dab.hardness / 100, 0), 1);
  const flow = Math.min(Math.max(dab.flow, 0), 1);
  // At least a 0.5px gap between the inner (hardness) stop and the outer
  // edge -- avoids a degenerate r0===r1 radial gradient (unreliable
  // across Canvas2D implementations) when hardness is at/near 100.
  const outerR = Math.max(r, 0.5);
  const innerR = Math.min(hardStop * outerR, outerR - 0.5);

  ctx.beginPath();
  ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
  if (dab.mode === "erase") {
    ctx.globalCompositeOperation = "multiply";
    const floor = Math.round((1 - flow) * 255);
    const gradient = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
    gradient.addColorStop(0, `rgb(${floor},${floor},${floor})`);
    gradient.addColorStop(1, "rgb(255,255,255)");
    ctx.fillStyle = gradient;
  } else {
    ctx.globalCompositeOperation = "lighter";
    const peak = Math.round(flow * 255);
    const gradient = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
    gradient.addColorStop(0, `rgb(${peak},${peak},${peak})`);
    gradient.addColorStop(1, "rgb(0,0,0)");
    ctx.fillStyle = gradient;
  }
  ctx.fill();
}

/** M4 Slice 2: a spot dab's own weight shape, rasterized to MATCH
 * `develop_engine.rs`'s `spot_mask_weight` formula exactly (full weight
 * out to `(1-softness)*radius`, linearly fading to 0 at
 * `(1+softness)*radius`) rather than reusing `rasterizeDab`'s
 * hardness/flow model, which has different feather semantics (fades
 * INSIDE the dab's own radius, not beyond it). Unlike brush dabs, EVERY
 * spot dab in a stroke shares one softness (the mask's own `feather`,
 * not a per-dab setting baked in at paint time), so this needs to be
 * passed in explicitly rather than read off the dab itself -- see
 * `syncMaskRasterization`'s own `featherDrawn` tracking for why a
 * feather change forces a full re-rasterization of every dab, unlike
 * brush's append-only dabs. Always "add" (`lighter`) compositing -- spot
 * removal has no erase-mode concept. */
export function rasterizeSpotDab(
  /** @type {OffscreenCanvasRenderingContext2D} */ ctx,
  /** @type {number} */ canvasWidth,
  /** @type {number} */ canvasHeight,
  /** @type {import('$lib/api/develop.js').SpotDab} */ dab,
  /** @type {number} */ feather,
) {
  const cx = dab.x * canvasWidth;
  const cy = dab.y * canvasHeight;
  const r = dab.radius * canvasWidth;
  const softness = Math.min(Math.max(feather / 100, 0), 0.999);
  const innerR = Math.max(r * (1 - softness), 0);
  const outerR = Math.max(r * (1 + softness), innerR + 0.5);

  ctx.globalCompositeOperation = "lighter";
  ctx.beginPath();
  ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
  const gradient = ctx.createRadialGradient(cx, cy, innerR, cx, cy, outerR);
  gradient.addColorStop(0, "rgb(255,255,255)");
  gradient.addColorStop(1, "rgb(0,0,0)");
  ctx.fillStyle = gradient;
  ctx.fill();
}

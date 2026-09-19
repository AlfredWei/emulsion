// Pass-size planning for the dehaze atmospheric-light reduction chain. Moved verbatim
// out of DevelopCanvas.svelte; reads no component state.

/** Every GPU-resource-side-effect of "a decoded bitmap is now the active
 * source image" -- shared between loadImage (a genuinely new image) and
 * upgradeToFullTier (the SAME image, a higher-resolution decode of it),
 * so the ~70 lines of WebGPU setup below exist in exactly one place
 * rather than two copies that could drift out of sync. */
/** The atmospheric-light reduction chain's own successive sizes (M3
 * Dehaze) -- each pass does an 8x8-block reduction (see fs_atm_reduce's
 * own doc comment), so each step's size is ceil(previous/8), stopping
 * once BOTH dimensions reach 1. `do...while` (not `while`) guarantees at
 * least one entry even for a degenerate 1x1 source, so
 * atmLightChain[atmLightChain.length - 1] is never read from an empty
 * array. */
export function buildAtmLightChainSizes(/** @type {number} */ width, /** @type {number} */ height) {
  const sizes = [];
  let w = width;
  let h = height;
  do {
    w = Math.max(1, Math.ceil(w / 8));
    h = Math.max(1, Math.ceil(h / 8));
    sizes.push([w, h]);
  } while (w > 1 || h > 1);
  return sizes;
}

// Per-adjustment views of the open image's edit stack (RFC-0009 §3.4, P5a). Each field is its own
// `$derived` -- deliberately NOT one `$derived(readAdjustments(editStack))` object: with separate
// fields, a consumer that reads only `exposure` is not re-run when `contrast` changes, so a slider
// drag does not re-run unrelated effects. Every expression is exactly the one the page used to
// have, reading `develop.editStack`.

import {
  opValue,
  getToneCurvePoints,
  IDENTITY_TONE_CURVE,
  getHslBands,
  IDENTITY_HSL_BANDS,
  getSplitToning,
  IDENTITY_SPLIT_TONING,
  getVignette,
  IDENTITY_VIGNETTE,
  getGrain,
  IDENTITY_GRAIN,
  getSharpen,
  IDENTITY_SHARPEN,
  getLumaNr,
  IDENTITY_LUMA_NR,
  getColorNr,
  IDENTITY_COLOR_NR,
  getCrop,
  IDENTITY_CROP,
  getLensCorrection,
  IDENTITY_LENS_CORRECTION,
  getPerspective,
  IDENTITY_PERSPECTIVE,
} from "$lib/api/develop.js";
import { develop } from "./develop.svelte.js";

export class DevelopView {
  /** @param {import('./develop.svelte.js').DevelopStore} develop */
  constructor(develop) {
    // Set before any derived below is first read (deriveds are lazy).
    this.develop = develop;
  }

  exposure = $derived(opValue(this.develop.editStack, "exposure", 0));
  contrast = $derived(opValue(this.develop.editStack, "contrast", 0));
  saturation = $derived(opValue(this.develop.editStack, "saturation", 0));
  temperature = $derived(opValue(this.develop.editStack, "temperature", 0));
  tint = $derived(opValue(this.develop.editStack, "tint", 0));
  highlights = $derived(opValue(this.develop.editStack, "highlights", 0));
  shadows = $derived(opValue(this.develop.editStack, "shadows", 0));
  whites = $derived(opValue(this.develop.editStack, "whites", 0));
  blacks = $derived(opValue(this.develop.editStack, "blacks", 0));
  // Tone Curve (M3): a global-only adjustment (applied after exposure/
  // contrast/saturation, before any mask -- see develop_engine.rs/
  // DevelopCanvas.svelte's shared ordering comment), but its payload is a
  // structured `points` array, not upsertOp's single scalar -- same
  // reason masks needed their own dedicated handler shape.
  toneCurvePoints = $derived(getToneCurvePoints(this.develop.editStack, IDENTITY_TONE_CURVE));
  // HSL / Color Mixer (M3): same global-only, structured-payload shape as
  // Tone Curve above -- band-keyed, not upsertOp's single scalar.
  hslBands = $derived(getHslBands(this.develop.editStack, IDENTITY_HSL_BANDS));
  // Split Toning (M3): same global-only shape as Tone Curve/HSL above, but
  // nested per-zone -- a per-zone UI control patches just that zone's
  // hue/saturation, leaving the other zone and balance untouched.
  splitToning = $derived(getSplitToning(this.develop.editStack, IDENTITY_SPLIT_TONING));
  // Dehaze (M3): a single global scalar op (dark-channel-prior haze
  // removal), the SAME shape exposure/contrast/saturation already use --
  // reuses opValue/upsertOp/handleAdjustmentChange directly rather than a
  // dedicated getter/handler pair, since there's nothing structured about
  // its payload the generic single-scalar op model doesn't already cover.
  dehaze = $derived(opValue(this.develop.editStack, "dehaze", 0));
  // Texture & Clarity (M3): same generic single-scalar op model as Dehaze
  // above -- -100..100, no dedicated getter/handler pair needed.
  texture = $derived(opValue(this.develop.editStack, "texture", 0));
  clarity = $derived(opValue(this.develop.editStack, "clarity", 0));
  // Vignette (M3): a structured 3-field payload (amount/midpoint/feather)
  // -- same getSplitToning/upsertX shape Split Toning already established
  // for a global-only, non-single-scalar op, not the generic opValue
  // model Texture/Clarity/Dehaze use.
  vignette = $derived(getVignette(this.develop.editStack, IDENTITY_VIGNETTE));
  // Lens Corrections (M3): same structured, own-getter/handler shape as
  // Vignette/Grain above, PLUS a separate profile-baking step (below,
  // called from openDevelop) -- see develop.js's own doc comment on
  // `setLensProfile` for why that's not a user-facing "change" at all,
  // and doesn't go through this handler or scheduleFlush's history label.
  lensCorrection = $derived(getLensCorrection(this.develop.editStack, IDENTITY_LENS_CORRECTION));
  // Perspective Correction (M4): same structured, own-getter/handler shape
  // as Lens Corrections/Vignette above.
  perspective = $derived(getPerspective(this.develop.editStack, IDENTITY_PERSPECTIVE));
  // Grain (M3): same structured, own-getter/handler shape as Vignette
  // above.
  grain = $derived(getGrain(this.develop.editStack, IDENTITY_GRAIN));
  // Sharpening / Noise Reduction (M3): same structured, own-getter/
  // handler shape as Vignette/Grain above -- three independent ops.
  sharpen = $derived(getSharpen(this.develop.editStack, IDENTITY_SHARPEN));
  lumaNR = $derived(getLumaNr(this.develop.editStack, IDENTITY_LUMA_NR));
  colorNR = $derived(getColorNr(this.develop.editStack, IDENTITY_COLOR_NR));
  // Crop & Straighten (M3): same structured, own-getter/handler shape as
  // every other multi-field op above -- see develop_engine.rs's own
  // `apply_crop` doc comment for why this one has no WGSL/uniform twin.
  crop = $derived(getCrop(this.develop.editStack, IDENTITY_CROP));
}

/** @param {import('./develop.svelte.js').DevelopStore} dev */
export function createDevelopView(dev) {
  return new DevelopView(dev);
}

export const developView = createDevelopView(develop);

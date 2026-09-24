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
  effectiveEditStack as computeEffectiveEditStack,
  isPanelHidden as checkPanelHidden,
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

  // RFC-0013: every field ABOVE is deliberately UI-facing and keeps
  // reading the RAW `develop.editStack` -- a hidden panel's sliders must
  // stay showing (and editing) their real stored values, exactly like
  // real Lightroom's own panel switch (dimmed, not reset). `isPanelHidden`
  // below reads the raw stack for the same reason. Everything BELOW this
  // line is the separate, render-facing counterpart: the same fields,
  // sourced from `effectiveEditStack` instead, for DevelopCanvas's GPU
  // uniform-buffer inputs only -- DevelopPanel must never read these.
  effectiveEditStack = $derived(computeEffectiveEditStack(this.develop.editStack));

  /** Reads the RAW editStack (not `effectiveEditStack`, which never
   * contains a `panel_hidden` marker by construction -- checking against
   * the filtered view would always answer false). A plain method, not a
   * $derived, so DevelopPanel can call it per-panel without a $derived
   * field per panel. */
  isPanelHidden(/** @type {string} */ panel) {
    return checkPanelHidden(this.develop.editStack, panel);
  }

  renderExposure = $derived(opValue(this.effectiveEditStack, "exposure", 0));
  renderContrast = $derived(opValue(this.effectiveEditStack, "contrast", 0));
  renderSaturation = $derived(opValue(this.effectiveEditStack, "saturation", 0));
  renderTemperature = $derived(opValue(this.effectiveEditStack, "temperature", 0));
  renderTint = $derived(opValue(this.effectiveEditStack, "tint", 0));
  renderHighlights = $derived(opValue(this.effectiveEditStack, "highlights", 0));
  renderShadows = $derived(opValue(this.effectiveEditStack, "shadows", 0));
  renderWhites = $derived(opValue(this.effectiveEditStack, "whites", 0));
  renderBlacks = $derived(opValue(this.effectiveEditStack, "blacks", 0));
  renderToneCurvePoints = $derived(getToneCurvePoints(this.effectiveEditStack, IDENTITY_TONE_CURVE));
  renderHslBands = $derived(getHslBands(this.effectiveEditStack, IDENTITY_HSL_BANDS));
  renderSplitToning = $derived(getSplitToning(this.effectiveEditStack, IDENTITY_SPLIT_TONING));
  renderDehaze = $derived(opValue(this.effectiveEditStack, "dehaze", 0));
  renderTexture = $derived(opValue(this.effectiveEditStack, "texture", 0));
  renderClarity = $derived(opValue(this.effectiveEditStack, "clarity", 0));
  renderVignette = $derived(getVignette(this.effectiveEditStack, IDENTITY_VIGNETTE));
  renderLensCorrection = $derived(getLensCorrection(this.effectiveEditStack, IDENTITY_LENS_CORRECTION));
  renderPerspective = $derived(getPerspective(this.effectiveEditStack, IDENTITY_PERSPECTIVE));
  renderGrain = $derived(getGrain(this.effectiveEditStack, IDENTITY_GRAIN));
  renderSharpen = $derived(getSharpen(this.effectiveEditStack, IDENTITY_SHARPEN));
  renderLumaNR = $derived(getLumaNr(this.effectiveEditStack, IDENTITY_LUMA_NR));
  renderColorNR = $derived(getColorNr(this.effectiveEditStack, IDENTITY_COLOR_NR));
  // Crop has no render* counterpart -- out of scope for panel visibility
  // (RFC-0013 §2), so DevelopCanvas keeps reading the plain `crop` field.
}

/** @param {import('./develop.svelte.js').DevelopStore} dev */
export function createDevelopView(dev) {
  return new DevelopView(dev);
}

export const developView = createDevelopView(develop);

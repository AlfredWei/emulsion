// Local-adjustment mask tool state (RFC-0009 §3.3, P6a): which tool is active, which mask is
// selected, the brush/spot tool options, the overlay switches, and the two "waiting for the next
// canvas click" targets (colour-range re-sample, eyedropper). All of it is pure view state, never
// persisted into the edit stack. `list`/`selectedMask` read the edit stack from the `develop` store
// (store DAG: masks -> develop); the store takes it as a constructor argument so tests can build an
// isolated pair.
//
// The two self-cleaning effects (a resample or eyedropper target is only valid while its tool is
// active) live in `install()`, which the owning component calls once from its <script>: a `$effect`
// cannot be created at module load. Workflows that also write the edit stack (mask create/delete/
// update, re-sample commit, eyedropper routing) are actions, not methods here.

import { listMasks, listModifiers } from "$lib/api/develop.js";
import { develop } from "./develop.svelte.js";

export class MaskStore {
  /** @param {import('./develop.svelte.js').DevelopStore} develop */
  constructor(develop) {
    // Set before any derived below is first read (deriveds are lazy).
    this.develop = develop;
  }

  // M3 Slice 5: local adjustment masks. `activeTool` drives DevelopCanvas's
  // hard-branched pointer routing (mask placement vs. pan/zoom);
  // `selectedMaskId` drives which mask (if any) MaskEditorPanel shows.
  // Both are pure view state, not persisted -- reset whenever Develop is
  // left, matching the same reasoning DevelopCanvas's own zoomMode uses.
  list = $derived(listMasks(this.develop.editStack));
  activeTool = $state(/** @type {string | null} */ (null));
  selectedMaskId = $state(/** @type {string | null} */ (null));
  selectedMask = $derived(this.list.find((m) => m.id === this.selectedMaskId) ?? null);

  // Composable masking (RFC-0025 slice 1b). `selectedShapeId` is the modifier ("shape") of the
  // selected mask being edited, or null for the base shape; it is view state like the selection
  // above, and self-cleans when the mask changes or the shape is removed (see install()).
  // `shapeTarget` is the "next placement creates a shape, not a mask" arming: set by Add shape
  // with the tool for the chosen kind, consumed by the next placement (handleMaskCreated).
  selectedShapeId = $state(/** @type {string | null} */ (null));
  shapeTarget = $state(/** @type {{ maskId: string, combine: import('$lib/api/develop.js').Combine, tool: string } | null} */ (null));
  selectedShape = $derived(
    this.selectedMask && this.selectedShapeId !== null
      ? (listModifiers(this.selectedMask).find((x) => x.id === this.selectedShapeId) ?? null)
      : null,
  );
  /** The selected shape as a mask-shaped object (its own id), for the canvas overlay only: the
   * handles and brush painting treat it like any mask. Never rendered by the GPU. */
  overlayShape = $derived(this.selectedShape ? /** @type {any} */ ({ ...this.selectedShape.shape, id: this.selectedShape.id }) : null);
  /** What the canvas highlights and paints into: the shape when one is selected, else the mask. */
  editingId = $derived(this.selectedShapeId ?? this.selectedMaskId);

  // M3 Slice 7: brush TOOL options -- unlike a mask's own exposure/
  // contrast/saturation (edited per-mask via MaskEditorPanel), these are
  // baked into each dab at the moment it's painted (real Lightroom's own
  // brush-options model: Size/Feather/Flow apply to whatever gets painted
  // NEXT), so they live here as plain view state, not per-mask, and are
  // never persisted or reset on module switch -- a user's preferred brush
  // size should survive across strokes/masks within one session.
  brushSize = $state(0.05);
  brushHardness = $state(70);
  brushFlow = $state(1);
  eraseMode = $state(false);
  // M4 Slice 2: spot removal's own brush-size TOOL option -- same "plain
  // view state, never persisted, survives across strokes/masks within one
  // session" treatment as brushSize above, kept separate (not shared with
  // brushSize) since a user's preferred adjustment-brush size and preferred
  // spot-removal size are independent preferences.
  spotBrushSize = $state(0.02);
  // M4 Slice 2: hides every mask's overlay chrome (handles, pins, link
  // lines, brush/spot cursors) so a user can review the actual graded
  // result underneath without edit-tool UI in the way -- per explicit user
  // request ("the UI pivot points will block user's review"). Distinct
  // from `showMaskOverlay` (that one only toggles the SELECTED
  // mask's own soft colored highlight fill); this one is a global
  // visibility switch for every mask's interactive chrome, toggleable via
  // MaskToolStrip's eye-icon button or the H hotkey (handleGlobalKeydown).
  maskOverlaysVisible = $state(true);

  // Mask UI polish: soft colored overlay for the SELECTED no-geometry mask
  // (brush, luminance range), toggleable via a MaskEditorPanel checkbox or
  // the "O" hotkey. Grouped with the brush TOOL options above, not with
  // activeTool/selectedMaskId -- deliberately NEVER force-reset on
  // openDevelop/switchModule, same "a user's preferred setting should
  // survive across strokes/masks/images within one session" reasoning
  // those already document. Defaults true: these mask kinds are otherwise
  // invisible until a nonzero adjustment is set, a real discoverability
  // gap this directly fixes.
  showMaskOverlay = $state(true);

  // Color range's "change select color" action: re-uses the SAME
  // click-to-sample canvas gesture that CREATES a color-range mask
  // (activeTool === "color_range" in DevelopCanvas.svelte), but points it
  // at an existing mask's `refColor` instead of creating a new mask.
  // `colorRangeResampleTarget` is the mask id awaiting its next canvas
  // click; kept separate from `selectedMaskId`/`activeTool` (rather than
  // overloading either) since NEITHER of those two states alone can tell
  // "the tool is active AND it's specifically in re-sample-into-an-
  // existing-mask mode, targeting THIS mask" apart from "the tool is
  // active to place a brand new mask."
  colorRangeResampleTarget = $state(/** @type {string | null} */ (null));
  isResamplingColor = $derived(this.colorRangeResampleTarget !== null && this.colorRangeResampleTarget === this.editingId);

  // Eyedropper pickers (M3): Tone Curve point-insert, HSL band-identify,
  // Split Toning zone-tint all share ONE click-to-sample canvas gesture
  // (activeTool === "eyedropper" in DevelopCanvas.svelte), generalizing the
  // color-range resample pattern above. `eyedropperTarget` names WHICH
  // of the four destinations is waiting for the next canvas click -- kept
  // separate from `activeTool` for the same reason `colorRangeResampleTarget`
  // is: `activeTool` alone can't distinguish "eyedropper active for Split
  // Toning Shadows" from "for HSL band-identify." Deliberately NOT threaded
  // into DevelopCanvas as a prop (unlike colorRangeResampleTarget): none of
  // these four destinations change how DevelopCanvas itself samples or
  // reports a click, only where +page.svelte routes the result afterward.
  eyedropperTarget = $state(
    /** @type {"split_toning_shadows" | "split_toning_highlights" | "hsl_band" | "tone_curve_point" | "white_balance" | null} */ (
      null
    ),
  );

  /** Registers the two self-cleaning effects. Call once, from a component's <script>. */
  install() {
    // Self-cleaning rather than patched into every place activeTool/
    // selectedMaskId can change (tool-strip toggle, panel close, mask
    // delete, module switch, selecting a different mask...): resample mode
    // is only ever valid while the color-range tool is active AND its
    // target is still the selected mask -- the instant either goes false,
    // there is no correct target left to resample into.
    $effect(() => {
      if (this.colorRangeResampleTarget !== null && (this.activeTool !== "color_range" || this.editingId !== this.colorRangeResampleTarget)) {
        this.colorRangeResampleTarget = null;
      }
    });
    // A selected shape is only valid while it is still one of the selected mask's modifiers; a
    // shape-placement arming only while ITS tool is the active one and its mask is still selected.
    $effect(() => {
      if (this.selectedShapeId !== null && this.selectedShape === null) this.selectedShapeId = null;
    });
    $effect(() => {
      if (this.shapeTarget !== null && (this.activeTool !== this.shapeTarget.tool || this.selectedMaskId !== this.shapeTarget.maskId)) {
        this.shapeTarget = null;
      }
    });
    // Self-cleaning, same reasoning as colorRangeResampleTarget's own effect
    // above -- including the two blanket `activeTool = null` resets on image
    // switch / module switch, which need no separate edit because this effect
    // already reacts to either of them.
    $effect(() => {
      if (this.eyedropperTarget !== null && this.activeTool !== "eyedropper") {
        this.eyedropperTarget = null;
      }
    });
  }
}

/** @param {import('./develop.svelte.js').DevelopStore} dev */
export function createMaskStore(dev) {
  return new MaskStore(dev);
}

export const masks = createMaskStore(develop);

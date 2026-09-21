// Soft-proof settings and preview (RFC-0009 §3.3, P6a): whether proofing is on, its target
// profile / intent / gamut warning, and the debounced preview it produces. Ephemeral view state,
// never persisted into the edit stack. The preview effect reads `develop` (store DAG:
// softProof -> develop) and writes only this store's own fields and timer; it lives in `install()`,
// which the owning component calls once from its <script>.

import { getSoftProofPreview } from "$lib/api/develop.js";
import { convertFileSrc } from "@tauri-apps/api/core";
import { develop } from "./develop.svelte.js";

export class SoftProofStore {
  /** @param {import('./develop.svelte.js').DevelopStore} develop */
  constructor(develop) {
    this.develop = develop;
  }

  // M4 Soft Proofing: ephemeral view state, never persisted into the edit
  // stack -- same "plain view state, not saved via handleAdjustmentChange/
  // scheduleFlush" treatment as maskOverlaysVisible/showOriginal.
  // `softProofPreviewUrl`/`softProofLoading` are populated by the debounced
  // effect in `install()` and passed straight through to DevelopCanvas for display.
  enabled = $state(false);
  target = $state(
    /** @type {"srgb" | "adobe-rgb" | "prophoto-rgb" | "custom"} */ ("srgb"),
  );
  customProfilePath = $state(/** @type {string | null} */ (null));
  intent = $state(
    /** @type {"perceptual" | "relative" | "saturation" | "absolute"} */ ("relative"),
  );
  gamutWarning = $state(false);
  previewUrl = $state(/** @type {string | null} */ (null));
  loading = $state(false);
  profileLabel = $derived(
    this.target === "adobe-rgb"
      ? "Adobe RGB"
      : this.target === "prophoto-rgb"
        ? "ProPhoto RGB"
        : this.target === "custom"
          ? (this.customProfilePath?.split(/[\\/]/).pop() ?? "Custom Profile")
          : "sRGB",
  );

  /** @type {ReturnType<typeof setTimeout> | null} */
  #timer = null;

  /** Debounced (same 250ms settle as scheduleFlush, a separate timer
   * for a separate purpose -- this refetches a PREVIEW, it never writes
   * anything) fetch of the soft-proofed preview whenever proofing is on and
   * anything it depends on changes: the edit stack (so proofing reflects
   * the CURRENT graded look, not a stale one), which image is open, or the
   * proof settings themselves. Off (or no image open) just clears the
   * preview -- DevelopCanvas falls back to its own live WGSL render then.
   * Call once, from a component's <script>. */
  install() {
    $effect(() => {
      void this.develop.editStack;
      const versionId = this.develop.versionId;
      const enabled = this.enabled;
      const target = this.target;
      const customPath = this.customProfilePath;
      const intent = this.intent;
      const gamutWarning = this.gamutWarning;

      if (this.#timer) clearTimeout(this.#timer);

      if (!enabled || versionId === null) {
        this.previewUrl = null;
        this.loading = false;
        return;
      }

      this.#timer = setTimeout(() => {
        this.loading = true;
        getSoftProofPreview(versionId, {
          target,
          custom_profile_path: customPath,
          intent,
          gamut_warning: gamutWarning,
        })
          .then((preview) => {
            this.previewUrl = convertFileSrc(preview.path);
          })
          .catch(() => {
            this.previewUrl = null;
          })
          .finally(() => {
            this.loading = false;
          });
      }, 250);

      return () => {
        if (this.#timer) clearTimeout(this.#timer);
      };
    });
  }
}

/** @param {import('./develop.svelte.js').DevelopStore} dev */
export function createSoftProofStore(dev) {
  return new SoftProofStore(dev);
}

export const softProof = createSoftProofStore(develop);

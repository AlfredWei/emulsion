// People/Faces state (RFC-0009 §3.3, P3c; feature design in RFC-0005). Detected faces for the
// CURRENTLY SELECTED photo show up in MetadataPanel's "People" section, and face rectangles
// overlay the Loupe view when `showFaceRects` is on. `people` also backs the tag popover's
// "search existing people" suggestions in MetadataPanel and CatalogRail's collapsible People
// section.
//
// The IPC-backed operations on this state (refresh people, rename, reassign, create-and-tag,
// mark not-a-face, cancel detection) are in lib/actions/faceActions.js.

export class FacesStore {
  people = $state(/** @type {import('$lib/api/faces.js').PersonRow[]} */ ([]));
  currentImageFaces = $state(/** @type {import('$lib/api/faces.js').FaceRow[]} */ ([]));
  showFaceRects = $state(false);
  hoveredFaceId = $state(/** @type {number | null} */ (null));

  // Shared by all three detection entry points (MetadataPanel's per-photo "Face" button,
  // Library's multi-select batch action, and "Detect Faces in Folder") -- only one
  // `detect_faces_for_images` job is ever meaningfully in flight at a time in this single-window
  // app (see lib.rs's `AppState.face_detection_cancel` doc comment), so one shared in-flight flag
  // is enough; `detectingFaces` also gates the "Face" button so a per-photo click can't race a
  // folder-wide job sharing the same backend cancel flag.
  detectingFaces = $state(false);
  scanProgress = $state(/** @type {{ current: number, total: number } | null} */ (null));
  detectionCancelable = $state(false);

  // CatalogRail's People section avatar crop source, keyed by SOURCE PATH (not person id --
  // several people can share a cover photo). Decoding is the expensive part; the per-person crop
  // itself is a pure CSS background-position/size trick done in CatalogRail.
  avatarSourceUrls = $state(/** @type {Record<string, string | null>} */ ({}));

  // Import-time face-detection opt-in (Library-integration redesign, 2026-09-17): detection used
  // to run silently on every import; now the user is asked first, via the same promise-bridge
  // pattern BackupPromptDialog established for a modal that a plain async function needs to
  // await mid-flow.
  confirmingDetectionOnImport = $state(false);
  pendingImportBatchSize = $state(0);
  /** @type {((run: boolean) => void) | null} */
  #resolveDetectionPrompt = null;

  /** Opens the "Detect faces?" prompt for a just-imported batch; resolves true/false with the
   * user's answer. */
  promptDetectionOnImport(/** @type {number} */ count) {
    this.pendingImportBatchSize = count;
    this.confirmingDetectionOnImport = true;
    return /** @type {Promise<boolean>} */ (
      new Promise((resolve) => {
        this.#resolveDetectionPrompt = resolve;
      })
    );
  }

  // Arrow fields so the dialog can be handed `onConfirm={faces.confirmDetectionPrompt}` unbound.
  confirmDetectionPrompt = () => {
    this.confirmingDetectionOnImport = false;
    this.#resolveDetectionPrompt?.(true);
    this.#resolveDetectionPrompt = null;
  };

  cancelDetectionPrompt = () => {
    this.confirmingDetectionOnImport = false;
    this.#resolveDetectionPrompt?.(false);
    this.#resolveDetectionPrompt = null;
  };
}

export function createFacesStore() {
  return new FacesStore();
}

export const faces = createFacesStore();

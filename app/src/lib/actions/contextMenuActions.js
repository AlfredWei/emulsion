// Right-click entry points (RFC-0028 §3.1-3.2): which menu a right-click (or Shift+F10 / the Menu key) opens,
// the target rule that decides what it acts on, and the real commands the menu items call -- each one an
// existing action. The items themselves are built by the pure builders in lib/contextMenus.
//
// Target rule: on a photo that is part of the selection the menu acts on the whole selection and leaves it
// alone; on any other photo the selection first becomes just that photo (Lightroom / Finder). In Develop the
// filmstrip has no multi-selection yet (RFC-0028 slice 3), so a right-click there always acts on the one photo.

import { library } from "$lib/state/library.svelte.js";
import { selection } from "$lib/state/selection.svelte.js";
import { shell } from "$lib/state/shell.svelte.js";
import { develop } from "$lib/state/develop.svelte.js";
import { presets } from "$lib/state/presets.svelte.js";
import { exportFlow } from "$lib/state/exportFlow.svelte.js";
import { contextMenu } from "$lib/state/contextMenu.svelte.js";
import { faces } from "$lib/state/faces.svelte.js";
import { masks } from "$lib/state/masks.svelte.js";
import { handleSelectAll, handleDeselectAll } from "$lib/actions/selectionActions.js";
import { handleRatingChange, handleFlagChange, handleColorLabelChange } from "$lib/actions/metadataActions.js";
import { handleAddToCollectionSelect, handleRemoveFromCollection } from "$lib/actions/collectionsActions.js";
import {
  handleCopySettingsFromPhoto,
  handlePasteSettings,
  handlePasteSettingsToSelection,
  handleApplyPresetToSelection,
  handleResetSettingsForSelection,
  handleApplyPreset,
  handleExportPreset,
  handleDeletePresetRequest,
  handleSaveCurrentAsPresetRequest,
  handleImportPresetRequest,
  handleCopySettingsRequest,
} from "$lib/actions/presetActions.js";
import { handleDetectFacesForSelected, handleDetectFacesForSelection } from "$lib/actions/faceActions.js";
import { handleImportFolder, handleImportFiles } from "$lib/actions/importActions.js";
import { prioritizeThumbnail, selectCollection, selectPerson, handleDeleteCollection } from "$lib/actions/libraryActions.js";
import { handleRestoreSnapshot, handleUndo, handleRedo } from "$lib/actions/historyActions.js";
import {
  handleDeleteSnapshot,
  handleTogglePanelVisibility,
  handleResetPanel,
  handleSoloPanel,
  handleShowAllPanels,
  handleToggleClippingOverlay,
  PANEL_LABELS,
} from "$lib/actions/developActions.js";
import { handleMaskDeleted, handleMaskUpdated, handleShapeSelected, handleShapeRemoved } from "$lib/actions/maskActions.js";
import { isPanelHidden, PANEL_IDS, listModifiers, OVERLAY_CAPABLE_MASK_OPS } from "$lib/api/develop.js";
import { openDevelop, switchModule } from "$lib/actions/navigation.js";
import { revealInFileManager } from "$lib/api/system.js";
import { buildPhotoMenu } from "$lib/contextMenus/photoMenu.js";
import { buildGridBackgroundMenu } from "$lib/contextMenus/gridMenu.js";
import { buildPanelHeaderMenu, buildCanvasMenu, buildMaskMenu, buildShapeMenu } from "$lib/contextMenus/developMenus.js";
import { buildCollectionMenu, buildPersonMenu, buildSnapshotMenu, buildPresetMenu, buildPresetListMenu } from "$lib/contextMenus/railMenus.js";
import { matchesRules } from "$lib/collectionRules.js";

/** Elements whose own (native Cut / Copy / Paste) menu is kept.
 * @param {EventTarget | null} target */
function isTextField(target) {
  return target instanceof Element && target.closest("input, textarea, select, [contenteditable='true']") !== null;
}

/** Applies the target rule and returns the photos the menu acts on, in library order.
 * @param {number} versionId @param {"library" | "develop"} module */
export function resolveTargets(versionId, module) {
  const inSelection = module === "library" && selection.selectedIds.has(versionId);
  if (!inSelection) {
    selection.selectedId = versionId;
    selection.selectedIds = new Set([versionId]);
  }
  return library.images.filter((img) => selection.selectedIds.has(img.version_id));
}

/** What this platform calls its file manager, for "Show in ...". */
function fileManagerName() {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Windows/i.test(ua)) return "Explorer";
  if (/Mac/i.test(ua)) return "Finder";
  return "File Manager";
}

/** @returns {"library" | "develop"} */
function currentModule() {
  return shell.activeModule === "develop" ? "develop" : "library";
}

/** @param {number} versionId @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} [fromKeyboard] */
export function openPhotoMenu(versionId, x, y, opener, fromKeyboard = false) {
  const module = currentModule();
  const images = resolveTargets(versionId, module);
  const collection = library.activeCollection;
  const items = buildPhotoMenu(
    {
      images,
      module,
      openVersionId: module === "develop" ? develop.versionId : null,
      viewedCollection: collection && !collection.is_smart ? { id: collection.id, name: collection.name } : null,
      manualCollections: library.manualCollections.map((c) => ({ id: c.id, name: c.name })),
      presets: presets.list.map((p) => ({ id: p.id, name: p.name })),
      hasCopiedSettings: develop.copiedSettings !== null,
      shortcuts: shell.shortcuts,
      fileManager: fileManagerName(),
    },
    photoCommands(images, module),
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {number} x @param {number} y @param {HTMLElement | null} opener */
function openGridBackgroundMenu(x, y, opener) {
  const items = buildGridBackgroundMenu(
    { photoCount: library.filteredImages.length, selectedCount: selection.selectedIds.size },
    { selectAll: handleSelectAll, deselectAll: handleDeselectAll, importFolder: handleImportFolder, importFiles: handleImportFiles },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener });
}

/** The real commands of the photo menu: each is an existing action acting on the (already resolved) selection.
 * @param {import('$lib/api/catalog.js').ImageSummary[]} images @param {"library" | "develop"} module
 * @returns {import('$lib/contextMenus/photoMenu.js').PhotoMenuCommands} */
export function photoCommands(images, module) {
  return {
    openInDevelop: (versionId) => {
      openDevelop(versionId);
    },
    openInLoupe: async (versionId) => {
      if (shell.activeModule !== "library") await switchModule("library");
      selection.selectedId = versionId;
      selection.selectedIds = new Set([versionId]);
      library.libraryViewMode = "loupe";
      prioritizeThumbnail(versionId);
    },
    compare: () => {
      library.libraryViewMode = "compare";
    },
    survey: () => {
      library.libraryViewMode = "survey";
    },
    setRating: (rating) => handleRatingChange(undefined, rating),
    setFlag: (flag) => handleFlagChange(undefined, flag),
    setColorLabel: (colorLabel) => handleColorLabelChange(undefined, colorLabel),
    addToCollection: (id) => handleAddToCollectionSelect(id === "new" ? "__new__" : String(id)),
    removeFromCollection: () => handleRemoveFromCollection(),
    copySettings: (versionId) => handleCopySettingsFromPhoto(versionId),
    pasteSettings: () => {
      // The open Develop photo is pasted through Develop's own path (its in-memory stack, flushed under a label).
      if (module === "develop" && images.length === 1 && images[0].version_id === develop.versionId) return handlePasteSettings();
      return handlePasteSettingsToSelection();
    },
    applyPreset: (presetId) => handleApplyPresetToSelection(String(presetId)),
    resetSettings: () => handleResetSettingsForSelection(),
    exportPhotos: async () => {
      if (module === "develop") await develop.flushEditStack();
      exportFlow.items = images.map((img) => ({ path: img.path, version_id: img.version_id }));
    },
    reveal: (path) => {
      revealInFileManager(path);
    },
    copyPath: async (path) => {
      try {
        await navigator.clipboard.writeText(path);
        shell.notify("Path copied");
      } catch {
        shell.notify("Could not copy the path");
      }
    },
    detectFaces: () => (images.length > 1 ? handleDetectFacesForSelection() : handleDetectFacesForSelected()),
    removeFromCatalog: () => {
      library.confirmingRemoval = true;
    },
  };
}

/** Every surface that has a menu marks its element with one of these `data-ctx-*` attributes (the value is the
 * row's id where it has one). The nearest marked ancestor of the pointer decides which menu opens, so a photo cell
 * inside the grid opens the photo menu and the grid's own background the empty-area one. */
const SURFACE_SELECTOR =
  "[data-ctx-photo], [data-ctx-collection], [data-ctx-person], [data-ctx-snapshot], [data-ctx-preset], [data-ctx-presets], [data-ctx-panel], [data-ctx-canvas], [data-ctx-mask], [data-ctx-shape], [data-ctx-grid]";

/** @param {number} id @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openCollectionMenu(id, x, y, opener, fromKeyboard) {
  const collection = library.collections.find((c) => c.id === id);
  if (!collection) return;
  const count = collection.is_smart
    ? library.images.filter((img) => matchesRules(img, collection.rules ?? [], library.keywordIdsByImage)).length
    : (collection.count ?? 0);
  const items = buildCollectionMenu(
    { name: collection.name, isSmart: collection.is_smart, count },
    {
      open: () => showLibrarySource(() => selectCollection(id)),
      selectPhotos: () =>
        showLibrarySource(async () => {
          await selectCollection(id);
          handleSelectAll();
        }),
      remove: () => handleDeleteCollection(id),
    },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** Rail rows are shared with the People module; a "show" command makes sure Library is the module showing the result.
 * @param {() => void | Promise<void>} select */
async function showLibrarySource(select) {
  if (shell.activeModule !== "library" && shell.activeModule !== "people") await switchModule("library");
  await select();
}

/** @param {number} id @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openPersonMenu(id, x, y, opener, fromKeyboard) {
  const person = faces.people.find((p) => p.id === id);
  if (!person) return;
  const items = buildPersonMenu(
    { name: person.name, photoCount: person.photo_count },
    {
      showPhotos: () => showLibrarySource(() => selectPerson(id)),
      rename: () => {
        faces.renamingPersonId = id;
      },
    },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {number} id @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openSnapshotMenu(id, x, y, opener, fromKeyboard) {
  const snapshot = develop.snapshots.find((s) => s.id === id);
  if (!snapshot) return;
  const items = buildSnapshotMenu(
    { name: snapshot.name },
    { restore: () => handleRestoreSnapshot(id), remove: () => handleDeleteSnapshot(id) },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {number} id @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openPresetMenu(id, x, y, opener, fromKeyboard) {
  const preset = presets.list.find((p) => p.id === id);
  if (!preset) return;
  const items = buildPresetMenu(
    { name: preset.name },
    { apply: () => handleApplyPreset(id), exportPreset: () => handleExportPreset(id), remove: () => handleDeletePresetRequest(id) },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openPresetListMenu(x, y, opener, fromKeyboard) {
  const items = buildPresetListMenu({ saveCurrent: handleSaveCurrentAsPresetRequest, importPresets: handleImportPresetRequest });
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {HTMLElement} el the header @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openPanelHeaderMenu(el, x, y, opener, fromKeyboard) {
  const panel = el.dataset.ctxPanel ?? "";
  const stack = develop.editStack;
  const items = buildPanelHeaderMenu(
    {
      title: el.dataset.ctxTitle ?? PANEL_LABELS[panel] ?? panel,
      hidden: isPanelHidden(stack, panel),
      othersVisible: PANEL_IDS.some((p) => p !== panel && !isPanelHidden(stack, p)),
      anyHidden: PANEL_IDS.some((p) => isPanelHidden(stack, p)),
    },
    {
      toggleVisibility: () => handleTogglePanelVisibility(panel),
      reset: () => handleResetPanel(panel),
      solo: () => handleSoloPanel(panel),
      showAll: handleShowAllPanels,
    },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openCanvasMenu(x, y, opener, fromKeyboard) {
  const { history, historyIndex } = develop;
  const items = buildCanvasMenu(
    {
      undoLabel: develop.canUndo ? (history[historyIndex]?.label ?? "") : null,
      redoLabel: develop.canRedo ? (history[historyIndex + 1]?.label ?? "") : null,
      showOriginal: develop.showOriginal,
      clipping: develop.showClippingOverlay,
      maskOverlay: masks.showMaskOverlay,
      hasMasks: masks.list.length > 0,
      hasCopiedSettings: develop.copiedSettings !== null,
    },
    {
      undo: handleUndo,
      redo: handleRedo,
      toggleBeforeAfter: () => {
        develop.showOriginal = !develop.showOriginal;
      },
      // A fresh object each time: the canvas reacts to the object changing (see develop.zoomRequest).
      zoomFit: () => {
        develop.zoomRequest = { action: { type: "fit" } };
      },
      zoomActual: () => {
        develop.zoomRequest = { action: { type: "actual" } };
      },
      toggleClipping: handleToggleClippingOverlay,
      toggleMaskOverlay: () => {
        masks.showMaskOverlay = !masks.showMaskOverlay;
      },
      copySettings: handleCopySettingsRequest,
      pasteSettings: handlePasteSettings,
      resetSettings: () => {
        presets.confirmingReset = true;
      },
    },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {HTMLElement} el the mask panel @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openMaskMenu(el, x, y, opener, fromKeyboard) {
  const mask = masks.selectedMask;
  if (!mask) return;
  // Invert edits the selected shape (the base shape is the mask itself), exactly like the panel's checkbox.
  const shapeId = masks.selectedShapeId;
  const editShape = shapeId === null ? null : listModifiers(mask).find((m) => m.id === shapeId);
  const target = /** @type {any} */ (editShape ? editShape.shape : mask);
  const items = buildMaskMenu(
    {
      title: el.dataset.ctxTitle ?? "Mask",
      canInvert: mask.op !== "spot_mask" && mask.op !== "red_eye_mask",
      inverted: !!target.invert,
      overlayCapable: OVERLAY_CAPABLE_MASK_OPS.includes(mask.op) || listModifiers(mask).length > 0,
      overlayOn: masks.showMaskOverlay,
    },
    {
      toggleInvert: () => handleMaskUpdated(editShape ? editShape.id : mask.id, { invert: !target.invert }),
      toggleOverlay: () => {
        masks.showMaskOverlay = !masks.showMaskOverlay;
      },
      close: () => {
        masks.selectedMaskId = null;
      },
      remove: handleMaskDeleted,
    },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** @param {HTMLElement} el the shape row @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard */
function openShapeMenu(el, x, y, opener, fromKeyboard) {
  const id = el.dataset.ctxShape ?? "";
  const items = buildShapeMenu(
    { name: el.dataset.ctxTitle ?? "shape", selected: masks.selectedShapeId === id },
    { select: () => handleShapeSelected(id), remove: () => handleShapeRemoved(id) },
  );
  contextMenu.show({ items, x, y, returnFocusTo: opener, fromKeyboard });
}

/** Opens the menu of the surface `el` belongs to, if it has one. `fromKeyboard` starts the highlight on the first row.
 * @param {HTMLElement} el the marked element @param {number} x @param {number} y @param {HTMLElement | null} opener @param {boolean} fromKeyboard
 * @returns {boolean} whether a menu was opened */
function openSurfaceMenu(el, x, y, opener, fromKeyboard) {
  const d = el.dataset;
  if (d.ctxPhoto !== undefined) openPhotoMenu(Number(d.ctxPhoto), x, y, opener, fromKeyboard);
  else if (d.ctxCollection !== undefined) openCollectionMenu(Number(d.ctxCollection), x, y, opener, fromKeyboard);
  else if (d.ctxPerson !== undefined) openPersonMenu(Number(d.ctxPerson), x, y, opener, fromKeyboard);
  else if (d.ctxSnapshot !== undefined) openSnapshotMenu(Number(d.ctxSnapshot), x, y, opener, fromKeyboard);
  else if (d.ctxPreset !== undefined) openPresetMenu(Number(d.ctxPreset), x, y, opener, fromKeyboard);
  else if (d.ctxPresets !== undefined) openPresetListMenu(x, y, opener, fromKeyboard);
  else if (d.ctxPanel !== undefined) openPanelHeaderMenu(el, x, y, opener, fromKeyboard);
  else if (d.ctxShape !== undefined) openShapeMenu(el, x, y, opener, fromKeyboard);
  else if (d.ctxMask !== undefined) openMaskMenu(el, x, y, opener, fromKeyboard);
  else if (d.ctxCanvas !== undefined) openCanvasMenu(x, y, opener, fromKeyboard);
  else if (d.ctxGrid !== undefined && shell.activeModule === "library") openGridBackgroundMenu(x, y, opener);
  else return false;
  return true;
}

/** When the last menu was opened by a pointer (performance.now()), for `handleClickCapture`. */
let lastPointerMenuAt = -Infinity;

/** On macOS a Ctrl-click is a context click: WebKit fires `contextmenu` on mousedown and then a `click` that still
 * carries ctrlKey, which the Library reads as "toggle this photo in the selection" -- it would deselect the very photo
 * the menu was just opened for (found with a real mouse; the menu's commands act on the live selection). The window's
 * capture-phase click listener swallows that one trailing click.
 * @param {MouseEvent} e */
export function handleClickCapture(e) {
  if (!e.ctrlKey || !contextMenu.isOpen) return;
  if (performance.now() - lastPointerMenuAt > 600) return;
  e.preventDefault();
  e.stopPropagation();
}

/** The window's `contextmenu` listener: the webview's own menu is suppressed everywhere except text fields,
 * and the surfaces that have a menu open theirs. Anything else shows nothing.
 * @param {MouseEvent} e */
export function handleContextMenu(e) {
  if (isTextField(e.target)) return;
  e.preventDefault();
  const target = e.target instanceof Element ? e.target : null;
  contextMenu.close();
  const surface = target?.closest(SURFACE_SELECTOR);
  if (surface instanceof HTMLElement) openSurfaceMenu(surface, e.clientX, e.clientY, surface.dataset.ctxGrid !== undefined ? null : surface, false);
  if (contextMenu.isOpen) lastPointerMenuAt = performance.now();
}

/** Shift+F10 / the Menu key on a focused row opens its menu beside it (the keyboard route to the menu).
 * @param {KeyboardEvent} e @returns {boolean} whether it was handled */
export function handleContextMenuKey(e) {
  const isKey = (e.key === "F10" && e.shiftKey) || e.key === "ContextMenu";
  if (!isKey || isTextField(e.target)) return false;
  const el = document.activeElement;
  const surface = el instanceof HTMLElement ? el.closest(SURFACE_SELECTOR) : null;
  if (!(surface instanceof HTMLElement) || surface.dataset.ctxGrid !== undefined) return false;
  e.preventDefault();
  const box = surface.getBoundingClientRect();
  contextMenu.close();
  // Focus goes back to the element that had it (a row's inner button), not to the marked wrapper.
  return openSurfaceMenu(surface, box.left + 16, box.bottom - 12, /** @type {HTMLElement} */ (el), true);
}

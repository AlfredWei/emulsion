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
} from "$lib/actions/presetActions.js";
import { handleDetectFacesForSelected, handleDetectFacesForSelection } from "$lib/actions/faceActions.js";
import { handleImportFolder, handleImportFiles } from "$lib/actions/importActions.js";
import { prioritizeThumbnail, selectCollection, selectPerson, handleDeleteCollection } from "$lib/actions/libraryActions.js";
import { handleRestoreSnapshot } from "$lib/actions/historyActions.js";
import { handleDeleteSnapshot } from "$lib/actions/developActions.js";
import { openDevelop, switchModule } from "$lib/actions/navigation.js";
import { revealInFileManager } from "$lib/api/system.js";
import { buildPhotoMenu } from "$lib/contextMenus/photoMenu.js";
import { buildGridBackgroundMenu } from "$lib/contextMenus/gridMenu.js";
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
const SURFACE_SELECTOR = "[data-ctx-photo], [data-ctx-collection], [data-ctx-person], [data-ctx-snapshot], [data-ctx-preset], [data-ctx-presets], [data-ctx-grid]";

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
  else if (d.ctxGrid !== undefined && shell.activeModule === "library") openGridBackgroundMenu(x, y, opener);
  else return false;
  return true;
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

// The right-click menu of a photo (RFC-0028 §3.3): Library grid cell, Library filmstrip cell and Develop
// filmstrip cell. A pure function of what is under the pointer and the app state it reads, returning plain
// items; every `run` calls one of the injected commands (the real ones are in lib/actions/contextMenuActions.js,
// and each is an existing action), so what a menu offers is tested without a DOM or a store.

import { SEPARATOR, header, tidy } from "./model.js";
import { formatKeyDisplay } from "$lib/shortcuts.js";

/**
 * @typedef {Object} PhotoMenuInput
 * @property {import('$lib/api/catalog.js').ImageSummary[]} images  the photos the menu acts on (the selection when the clicked photo is in it)
 * @property {"library" | "develop"} module
 * @property {number | null} openVersionId  the photo open in Develop, if any
 * @property {{ id: number, name: string } | null} viewedCollection  the manual collection being viewed, if any
 * @property {{ id: number, name: string }[]} manualCollections
 * @property {{ id: number, name: string }[]} presets
 * @property {boolean} hasCopiedSettings
 * @property {Record<string, string>} shortcuts  the user's current key map
 * @property {string} [fileManager]  what the platform calls its file manager ("Finder", "Explorer"); default "Finder"
 */

/**
 * @typedef {Object} PhotoMenuCommands
 * @property {(versionId: number) => void} openInDevelop
 * @property {(versionId: number) => void} openInLoupe
 * @property {() => void} compare
 * @property {() => void} survey
 * @property {(rating: number) => void} setRating
 * @property {(flag: string) => void} setFlag
 * @property {(colorLabel: string) => void} setColorLabel
 * @property {(collectionId: number | "new") => void} addToCollection
 * @property {() => void} removeFromCollection
 * @property {(versionId: number) => void} copySettings
 * @property {() => void} syncSettings
 * @property {() => void} pasteSettings
 * @property {(presetId: number) => void} applyPreset
 * @property {() => void} resetSettings
 * @property {() => void} exportPhotos
 * @property {(path: string) => void} reveal
 * @property {(path: string) => void} copyPath
 * @property {() => void} detectFaces
 * @property {() => void} removeFromCatalog
 */

const COLOR_LABELS = [
  ["red", "Red"],
  ["yellow", "Yellow"],
  ["green", "Green"],
  ["blue", "Blue"],
  ["purple", "Purple"],
  ["none", "None"],
];

/** The shared value of `pick(image)` across the photos, or `undefined` when they differ (nothing is checked).
 * @template T @param {import('$lib/api/catalog.js').ImageSummary[]} images @param {(i: import('$lib/api/catalog.js').ImageSummary) => T} pick */
function shared(images, pick) {
  const first = pick(images[0]);
  return images.every((i) => pick(i) === first) ? first : undefined;
}

/** @param {number} n */
function plural(n) {
  return n === 1 ? "1 photo" : `${n} photos`;
}

/** @param {PhotoMenuInput} input @param {PhotoMenuCommands} cmd @returns {import('./model.js').MenuEntry[]} */
export function buildPhotoMenu(input, cmd) {
  const { images, module, openVersionId, viewedCollection, manualCollections, presets, hasCopiedSettings, shortcuts, fileManager = "Finder" } = input;
  if (images.length === 0) return [];
  const many = images.length > 1;
  const first = images[0];
  const key = (/** @type {string} */ id) => (shortcuts[id] ? formatKeyDisplay(shortcuts[id]) : undefined);

  const rating = shared(images, (i) => i.rating);
  const flag = shared(images, (i) => i.flag);
  const label = shared(images, (i) => i.color_label);
  const onlyOpenPhoto = !many && first.version_id === openVersionId;

  // Sync Settings (RFC-0028 §3.4) copies the open photo's settings to the other selected ones.
  const syncable = module === "develop" && many && images.some((i) => i.version_id === openVersionId);

  const singleOnly = many ? "Choose a single photo" : undefined;

  return tidy([
    many && header(plural(images.length)),

    !onlyOpenPhoto && { id: "open-develop", label: "Open in Develop", shortcut: key("viewDevelop"), run: () => cmd.openInDevelop(first.version_id) },
    { id: "open-loupe", label: module === "develop" ? "Open in Library" : "Open in Loupe", shortcut: module === "develop" ? undefined : key("viewLoupe"), run: () => cmd.openInLoupe(first.version_id) },
    module === "library" && many && {
      id: "compare-survey",
      label: "Compare / Survey",
      children: [
        { id: "compare", label: "Compare the first two", disabled: false, run: cmd.compare },
        { id: "survey", label: "Survey", shortcut: key("viewSurvey"), run: cmd.survey },
      ],
    },
    SEPARATOR,

    {
      id: "rating",
      label: "Rating",
      children: [5, 4, 3, 2, 1].map((n) => ({
        id: `rating-${n}`,
        label: "★".repeat(n),
        shortcut: key(`rate${n}`),
        checked: rating === n,
        run: () => cmd.setRating(n),
      })).concat([{ id: "rating-0", label: "No rating", shortcut: key("rate0"), checked: rating === 0, run: () => cmd.setRating(0) }]),
    },
    {
      id: "flag",
      label: "Flag",
      children: [
        { id: "flag-pick", label: "Pick", shortcut: key("flagPick"), checked: flag === "pick", run: () => cmd.setFlag("pick") },
        { id: "flag-reject", label: "Reject", shortcut: key("flagReject"), checked: flag === "reject", run: () => cmd.setFlag("reject") },
        { id: "flag-none", label: "Unflag", shortcut: key("flagUnflag"), checked: flag === "none", run: () => cmd.setFlag("none") },
      ],
    },
    {
      id: "color-label",
      label: "Colour label",
      children: COLOR_LABELS.map(([value, text]) => ({ id: `label-${value}`, label: text, checked: label === value, run: () => cmd.setColorLabel(value) })),
    },
    {
      id: "add-to-collection",
      label: "Add to Collection",
      children: tidy([
        ...manualCollections.map((c) => ({ id: `collection-${c.id}`, label: c.name, run: () => cmd.addToCollection(c.id) })),
        manualCollections.length > 0 && SEPARATOR,
        { id: "collection-new", label: "New collection…", run: () => cmd.addToCollection("new") },
      ]),
    },
    module === "library" && viewedCollection && {
      id: "remove-from-collection",
      label: `Remove from “${viewedCollection.name}”`,
      run: cmd.removeFromCollection,
    },
    SEPARATOR,

    {
      id: "develop-settings",
      label: "Develop Settings",
      children: tidy([
        syncable && {
          id: "sync-settings",
          label: "Sync Settings…",
          run: cmd.syncSettings,
        },
        syncable && SEPARATOR,
        { id: "copy-settings", label: "Copy Settings…", disabled: many, reason: singleOnly, run: () => cmd.copySettings(first.version_id) },
        {
          id: "paste-settings",
          label: many ? `Paste Settings to ${images.length} photos` : "Paste Settings",
          disabled: !hasCopiedSettings,
          reason: hasCopiedSettings ? undefined : "Copy settings from a photo first",
          run: cmd.pasteSettings,
        },
        presets.length > 0 && { id: "apply-preset", label: "Apply Preset", children: presets.map((p) => ({ id: `preset-${p.id}`, label: p.name, run: () => cmd.applyPreset(p.id) })) },
        SEPARATOR,
        {
          id: "reset-settings",
          label: many ? `Reset Settings (${images.length} photos)…` : "Reset Settings…",
          danger: true,
          confirm: {
            title: "Reset Settings",
            message: `Remove all edits from ${many ? plural(images.length) : "this photo"}? This can be undone from each photo's History in Develop.`,
            confirmLabel: "Reset",
          },
          run: cmd.resetSettings,
        },
      ]),
    },
    SEPARATOR,

    { id: "export", label: "Export…", run: cmd.exportPhotos },
    { id: "reveal", label: `Show in ${fileManager}`, disabled: many, reason: singleOnly, run: () => cmd.reveal(first.path) },
    { id: "copy-path", label: "Copy Path", disabled: many, reason: singleOnly, run: () => cmd.copyPath(first.path) },
    { id: "detect-faces", label: "Detect Faces", run: cmd.detectFaces },
    SEPARATOR,

    {
      id: "remove-from-catalog",
      label: "Remove from Catalog…",
      danger: true,
      // The existing removal dialog asks (it names what stays on disk), so no second confirmation here.
      run: cmd.removeFromCatalog,
    },
  ]);
}

// Context menu session state (RFC-0028 §3.1): the one menu that may be open (its items, where it was
// opened, which element gets focus back) and the confirmation an item asked for. Pure view state, never
// persisted. The component (ContextMenu.svelte) renders it; the builders (lib/contextMenus) and the
// right-click entry points (lib/actions/contextMenuActions.js) feed it.

export class ContextMenuStore {
  /** @type {import('$lib/contextMenus/model.js').MenuEntry[]} */
  items = $state([]);
  x = $state(0);
  y = $state(0);
  isOpen = $state(false);
  /** Opened from the keyboard (Shift+F10): the first row starts highlighted, as a native menu does. */
  fromKeyboard = false;
  /** Where focus goes when the menu closes (the photo that was right-clicked). Not reactive: only read on close.
   * @type {HTMLElement | null} */
  returnFocusTo = null;
  /** A destructive item waiting for the user's yes; `run` is called on confirm.
   * @type {null | { title: string, message: string, confirmLabel: string, run: () => void }} */
  pendingConfirm = $state(null);

  /**
   * @param {{ items: import('$lib/contextMenus/model.js').MenuEntry[], x: number, y: number, returnFocusTo?: HTMLElement | null, fromKeyboard?: boolean }} menu
   * An empty list opens nothing: a surface with no applicable items shows no menu rather than an empty box.
   */
  show({ items, x, y, returnFocusTo = null, fromKeyboard = false }) {
    if (items.length === 0) {
      this.close();
      return;
    }
    this.items = items;
    this.x = x;
    this.y = y;
    this.returnFocusTo = returnFocusTo;
    this.fromKeyboard = fromKeyboard;
    this.isOpen = true;
  }

  /** Closes the menu. `restoreFocus` puts focus back on the opener (keyboard close and after an action);
   * a click outside leaves focus where the click put it. */
  close(restoreFocus = false) {
    const target = this.returnFocusTo;
    this.isOpen = false;
    this.items = [];
    this.returnFocusTo = null;
    if (!restoreFocus || !target || typeof target.focus !== "function") return;
    target.focus({ preventScroll: true });
    // The opener may be re-created by the action the menu ran (a re-render replaces the cell): find its successor.
    const id = target.dataset?.ctxPhoto;
    if (id !== undefined && typeof document !== "undefined") {
      requestAnimationFrame(() => {
        if (document.activeElement && document.activeElement !== document.body) return;
        /** @type {HTMLElement | null} */ (document.querySelector(`[data-ctx-photo="${id}"]`))?.focus({ preventScroll: true });
      });
    }
  }

  /** @param {{ title: string, message: string, confirmLabel: string }} ask @param {() => void} run */
  askConfirm(ask, run) {
    this.pendingConfirm = { ...ask, run };
  }

  confirm() {
    const p = this.pendingConfirm;
    this.pendingConfirm = null;
    p?.run();
  }

  cancelConfirm() {
    this.pendingConfirm = null;
  }
}

export const contextMenu = new ContextMenuStore();

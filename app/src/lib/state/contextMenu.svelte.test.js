import { describe, it, expect, vi } from "vitest";
import { ContextMenuStore } from "./contextMenu.svelte.js";

const items = [{ id: "a", label: "A", run: () => {} }];

describe("ContextMenuStore", () => {
  it("opens with items at a position and closes", () => {
    const s = new ContextMenuStore();
    s.show({ items, x: 10, y: 20 });
    expect([s.isOpen, s.x, s.y, s.items.length]).toEqual([true, 10, 20, 1]);
    s.close();
    expect([s.isOpen, s.items.length]).toEqual([false, 0]);
  });

  it("an empty list opens nothing, and closes a menu that was open", () => {
    const s = new ContextMenuStore();
    s.show({ items: [], x: 1, y: 1 });
    expect(s.isOpen).toBe(false);
    s.show({ items, x: 1, y: 1 });
    s.show({ items: [], x: 2, y: 2 });
    expect(s.isOpen).toBe(false);
  });

  it("returns focus to the opener only when asked", () => {
    const s = new ContextMenuStore();
    const opener = /** @type {any} */ ({ focus: vi.fn() });
    s.show({ items, x: 0, y: 0, returnFocusTo: opener });
    s.close(false);
    expect(opener.focus).not.toHaveBeenCalled();
    s.show({ items, x: 0, y: 0, returnFocusTo: opener });
    s.close(true);
    expect(opener.focus).toHaveBeenCalledTimes(1);
    s.close(true); // already closed: nothing left to focus
    expect(opener.focus).toHaveBeenCalledTimes(1);
  });

  it("a confirmation runs its action only on confirm", () => {
    const s = new ContextMenuStore();
    const run = vi.fn();
    s.askConfirm({ title: "t", message: "m", confirmLabel: "Go" }, run);
    expect(s.pendingConfirm?.title).toBe("t");
    s.cancelConfirm();
    expect(run).not.toHaveBeenCalled();
    expect(s.pendingConfirm).toBeNull();
    s.askConfirm({ title: "t", message: "m", confirmLabel: "Go" }, run);
    s.confirm();
    expect(run).toHaveBeenCalledTimes(1);
    expect(s.pendingConfirm).toBeNull();
  });

  it("remembers whether it was opened from the keyboard", () => {
    const s = new ContextMenuStore();
    s.show({ items, x: 0, y: 0, fromKeyboard: true });
    expect(s.fromKeyboard).toBe(true);
    s.show({ items, x: 0, y: 0 });
    expect(s.fromKeyboard).toBe(false);
  });
});

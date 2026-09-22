import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync } from "svelte";
import { createShellStore } from "./shell.svelte.js";
import { DEFAULT_SHORTCUTS } from "$lib/shortcuts.js";

const initial = () => ({ panelWidths: { history: 200, develop: 240 }, shortcuts: { ...DEFAULT_SHORTCUTS } });

/** @param {Partial<PointerEvent> & { clientX: number }} o */
function pointer(o) {
  const currentTarget = { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn() };
  return /** @type {any} */ ({ pointerId: 1, button: 0, clientY: 0, preventDefault: vi.fn(), currentTarget, ...o });
}

describe("ShellStore basics", () => {
  it("starts in Library with no status and the given preferences", () => {
    const shell = createShellStore(initial());
    expect(shell.activeModule).toBe("library");
    expect(shell.statusMessage).toBe("");
    expect(shell.settingsOpen).toBe(false);
    expect(shell.panelWidths).toEqual({ history: 200, develop: 240 });
    expect(shell.shortcuts.rate1).toBe("1");
    expect(shell.panelResizeState).toBeNull();
  });

  it("notify() sets the status message", () => {
    const shell = createShellStore(initial());
    shell.notify("Exported 3 photos");
    expect(shell.statusMessage).toBe("Exported 3 photos");
    shell.notify("");
    expect(shell.statusMessage).toBe("");
  });

  it("instances are independent (the factory gives tests isolated state)", () => {
    const a = createShellStore(initial());
    const b = createShellStore(initial());
    a.activeModule = "develop";
    a.notify("hi");
    expect(b.activeModule).toBe("library");
    expect(b.statusMessage).toBe("");
  });

  it("fields are reactive: an effect re-runs when activeModule or the status changes", () => {
    const shell = createShellStore(initial());
    /** @type {string[]} */
    const seen = [];
    const stop = $effect.root(() => {
      $effect(() => {
        seen.push(`${shell.activeModule}|${shell.statusMessage}`);
      });
    });
    flushSync();
    shell.activeModule = "develop";
    flushSync();
    shell.notify("done");
    flushSync();
    stop();
    expect(seen).toEqual(["library|", "develop|", "develop|done"]);
  });
});

describe("panel resize", () => {
  /** @type {Map<string, string>} */
  let storage;
  beforeEach(() => {
    storage = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (/** @type {string} */ k) => storage.get(k) ?? null,
      setItem: (/** @type {string} */ k, /** @type {string} */ v) => void storage.set(k, v),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("dragging the History rail right grows it; capture is taken and the default is prevented", () => {
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 100 });
    shell.handlePanelResizePointerDown(down, "history");
    expect(down.preventDefault).toHaveBeenCalled();
    expect(down.currentTarget.setPointerCapture).toHaveBeenCalledWith(1);
    expect(shell.panelResizeState).toEqual({ which: "history", startX: 100, startWidth: 200 });
    shell.handlePanelResizePointerMove(pointer({ clientX: 150 }));
    expect(shell.panelWidths).toEqual({ history: 250, develop: 240 });
  });

  it("dragging the adjustments rail right shrinks it (it sits on the right edge)", () => {
    const shell = createShellStore(initial());
    shell.handlePanelResizePointerDown(pointer({ clientX: 500 }), "develop");
    shell.handlePanelResizePointerMove(pointer({ clientX: 520 }));
    expect(shell.panelWidths.develop).toBe(220);
    shell.handlePanelResizePointerMove(pointer({ clientX: 470 }));
    expect(shell.panelWidths.develop).toBe(270);
  });

  it("clamps each rail to its own min/max", () => {
    const shell = createShellStore(initial());
    shell.handlePanelResizePointerDown(pointer({ clientX: 0 }), "history");
    shell.handlePanelResizePointerMove(pointer({ clientX: 10_000 }));
    expect(shell.panelWidths.history).toBe(400);
    shell.handlePanelResizePointerMove(pointer({ clientX: -10_000 }));
    expect(shell.panelWidths.history).toBe(160);

    shell.handlePanelResizePointerDown(pointer({ clientX: 0 }), "develop");
    shell.handlePanelResizePointerMove(pointer({ clientX: -10_000 }));
    expect(shell.panelWidths.develop).toBe(480);
    shell.handlePanelResizePointerMove(pointer({ clientX: 10_000 }));
    expect(shell.panelWidths.develop).toBe(200);
  });

  it("ignores moves when no drag is in progress", () => {
    const shell = createShellStore(initial());
    shell.handlePanelResizePointerMove(pointer({ clientX: 999 }));
    expect(shell.panelWidths).toEqual({ history: 200, develop: 240 });
  });

  it("releasing persists the widths once and ends the drag", () => {
    const shell = createShellStore(initial());
    shell.handlePanelResizePointerDown(pointer({ clientX: 100 }), "history");
    shell.handlePanelResizePointerMove(pointer({ clientX: 130 }));
    const up = pointer({ clientX: 130 });
    shell.handlePanelResizePointerUp(up);
    expect(up.currentTarget.releasePointerCapture).toHaveBeenCalledWith(1);
    expect(shell.panelResizeState).toBeNull();
    expect(JSON.parse([...storage.values()][0])).toEqual({ history: 230, develop: 240 });

    storage.clear();
    shell.handlePanelResizePointerUp(pointer({ clientX: 0 })); // stray pointerup: nothing to save
    expect(storage.size).toBe(0);
  });

  it("a throwing setPointerCapture/releasePointerCapture does not abort the drag", () => {
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 100 });
    down.currentTarget.setPointerCapture.mockImplementation(() => {
      throw new Error("not capturable");
    });
    expect(() => shell.handlePanelResizePointerDown(down, "history")).not.toThrow();
    expect(shell.panelResizeState).not.toBeNull();

    const up = pointer({ clientX: 100 });
    up.currentTarget.releasePointerCapture.mockImplementation(() => {
      throw new Error("not captured");
    });
    expect(() => shell.handlePanelResizePointerUp(up)).not.toThrow();
    expect(shell.panelResizeState).toBeNull();
  });

  it("handlers work unbound, as the template passes them (onpointermove={shell.handler...})", () => {
    const shell = createShellStore(initial());
    const { handlePanelResizePointerDown, handlePanelResizePointerMove } = shell;
    handlePanelResizePointerDown(pointer({ clientX: 0 }), "history");
    handlePanelResizePointerMove(pointer({ clientX: 20 }));
    expect(shell.panelWidths.history).toBe(220);
  });
});

describe("photo drag onto the map (shell.photoDrag)", () => {
  // Plain pointer events, not native HTML5 drag-and-drop -- see photoDrag's own doc comment in
  // shell.svelte.js for why (Tauri's window-level drag-drop interception, needed for real OS file
  // imports, stops the browser's native drag events from firing for a drag that never leaves the
  // page). Same pointerdown/pointermove/pointerup + setPointerCapture shape the panel-resize tests
  // above already cover, plus the movement threshold that keeps a plain click a plain click.

  it("a plain pointerdown/up with no real movement never starts a drag", () => {
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 10, clientY: 10 });
    shell.handlePhotoDragPointerDown(down, [1]);
    expect(shell.photoDrag).toBeNull();
    shell.handlePhotoDragPointerMove(pointer({ clientX: 11, clientY: 10 })); // 1px, below threshold
    expect(shell.photoDrag).toBeNull();
    expect(down.currentTarget.setPointerCapture).not.toHaveBeenCalled();
    shell.handlePhotoDragPointerUp(pointer({ clientX: 11, clientY: 10 }));
    expect(shell.photoDrag).toBeNull();
  });

  it("moving past the threshold starts the drag, captures the pointer, and tracks position", () => {
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 100, clientY: 100 });
    shell.handlePhotoDragPointerDown(down, [5, 6]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 106, clientY: 100 })); // 6px > 4px threshold
    expect(shell.photoDrag).toEqual({ imageIds: [5, 6], pointer: { x: 106, y: 100 } });
    expect(down.currentTarget.setPointerCapture).toHaveBeenCalledWith(1);

    shell.handlePhotoDragPointerMove(pointer({ clientX: 200, clientY: 150 }));
    expect(shell.photoDrag).toEqual({ imageIds: [5, 6], pointer: { x: 200, y: 150 } });
  });

  it("ignores a move for an unrelated pointerId", () => {
    const shell = createShellStore(initial());
    shell.handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0 }), [1]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 50, clientY: 0, pointerId: 2 }));
    expect(shell.photoDrag).toBeNull();
  });

  it("ignores a non-primary button (a right/middle click never starts a drag)", () => {
    const shell = createShellStore(initial());
    shell.handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0, button: 2 }), [1]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 50, clientY: 0 }));
    expect(shell.photoDrag).toBeNull();
  });

  it("pointerup ends the drag and releases capture on the originating element", () => {
    // Capture means the up event's own currentTarget is that same element in a real browser, but
    // release is keyed off the pointerdown's element regardless -- so it's checked directly here.
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 0, clientY: 0 });
    shell.handlePhotoDragPointerDown(down, [1]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 10, clientY: 0 }));
    expect(shell.photoDrag).not.toBeNull();
    shell.handlePhotoDragPointerUp(pointer({ clientX: 10, clientY: 0 }));
    expect(down.currentTarget.releasePointerCapture).toHaveBeenCalledWith(1);
    expect(shell.photoDrag).toBeNull();
  });

  it("ignores a pointerup for an unrelated pointerId, leaving the real drag untouched", () => {
    const shell = createShellStore(initial());
    shell.handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0 }), [1]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 10, clientY: 0 }));
    expect(shell.photoDrag).not.toBeNull();
    shell.handlePhotoDragPointerUp(pointer({ clientX: 10, clientY: 0, pointerId: 2 }));
    expect(shell.photoDrag).not.toBeNull();
  });

  it("pointerup clears the internal candidate too, so a stray move afterward starts nothing", () => {
    const shell = createShellStore(initial());
    shell.handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0 }), [1]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 10, clientY: 0 }));
    shell.handlePhotoDragPointerUp(pointer({ clientX: 10, clientY: 0 }));
    shell.handlePhotoDragPointerMove(pointer({ clientX: 999, clientY: 999 }));
    expect(shell.photoDrag).toBeNull();
  });

  it("a stray pointerup with nothing in progress does nothing", () => {
    const shell = createShellStore(initial());
    expect(() => shell.handlePhotoDragPointerUp(pointer({ clientX: 0, clientY: 0 }))).not.toThrow();
    expect(shell.photoDrag).toBeNull();
  });

  it("a throwing setPointerCapture/releasePointerCapture does not abort the drag", () => {
    // Both mocks are on down.currentTarget: capture and release are always keyed to the pointerdown's
    // element (`c.el`), never whatever a later event's own currentTarget happens to be.
    const shell = createShellStore(initial());
    const down = pointer({ clientX: 0, clientY: 0 });
    down.currentTarget.setPointerCapture.mockImplementation(() => {
      throw new Error("not capturable");
    });
    shell.handlePhotoDragPointerDown(down, [1]);
    expect(() => shell.handlePhotoDragPointerMove(pointer({ clientX: 10, clientY: 0 }))).not.toThrow();
    expect(shell.photoDrag).not.toBeNull();

    down.currentTarget.releasePointerCapture.mockImplementation(() => {
      throw new Error("not captured");
    });
    expect(() => shell.handlePhotoDragPointerUp(pointer({ clientX: 10, clientY: 0 }))).not.toThrow();
    expect(shell.photoDrag).toBeNull();
  });

  it("a later pointerdown for the same pointerId replaces the candidate", () => {
    const shell = createShellStore(initial());
    shell.handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0 }), [1]);
    shell.handlePhotoDragPointerDown(pointer({ clientX: 500, clientY: 500 }), [2]);
    shell.handlePhotoDragPointerMove(pointer({ clientX: 506, clientY: 500 }));
    expect(shell.photoDrag?.imageIds).toEqual([2]);
  });

  it("handlers work unbound, as Filmstrip passes them (onpointermove={shell.handler...})", () => {
    const shell = createShellStore(initial());
    const { handlePhotoDragPointerDown, handlePhotoDragPointerMove } = shell;
    handlePhotoDragPointerDown(pointer({ clientX: 0, clientY: 0 }), [7]);
    handlePhotoDragPointerMove(pointer({ clientX: 20, clientY: 0 }));
    expect(shell.photoDrag).toEqual({ imageIds: [7], pointer: { x: 20, y: 0 } });
  });
});

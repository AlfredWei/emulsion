import { describe, it, expect } from "vitest";
import { flushSync } from "svelte";
import { createPrintStore } from "./print.svelte.js";

describe("PrintStore", () => {
  it("starts with the Print module's defaults", () => {
    const p = createPrintStore();
    expect(p.items).toEqual([]);
    expect(p.template).toBe("single");
    expect(p.fitMode).toBe("fit");
    expect([p.rows, p.cols, p.cellSpacing]).toEqual([2, 2, 0.1]);
    expect(p.paperSize).toBe("letter");
    expect(p.orientation).toBe("portrait");
    expect(p.margins).toEqual({ top: 0.5, right: 0.5, bottom: 0.5, left: 0.5 });
    expect(p.colorManaged).toBe(false);
    expect(p.profileTarget).toBe("srgb");
    expect(p.customProfilePath).toBeNull();
    expect(p.intent).toBe("relative");
    expect([p.printing, p.exportingPdf]).toEqual([false, false]);
    expect(p.readyUrls).toEqual({});
  });

  it("colour-management settings are null until colour management is switched on", () => {
    const p = createPrintStore();
    expect(p.colorManagementSettings).toBeNull();
    p.colorManaged = true;
    expect(p.colorManagementSettings).toEqual({ target: "srgb", customProfilePath: null, intent: "relative" });
    p.profileTarget = "custom";
    p.customProfilePath = "/icc/print.icc";
    p.intent = "perceptual";
    expect(p.colorManagementSettings).toEqual({ target: "custom", customProfilePath: "/icc/print.icc", intent: "perceptual" });
    p.colorManaged = false;
    expect(p.colorManagementSettings).toBeNull();
  });

  it("the derived settings are reactive for effects", () => {
    const p = createPrintStore();
    /** @type {unknown[]} */
    const seen = [];
    const stop = $effect.root(() => {
      $effect(() => {
        seen.push(p.colorManagementSettings?.intent ?? null);
      });
    });
    flushSync();
    p.colorManaged = true;
    flushSync();
    p.intent = "absolute";
    flushSync();
    stop();
    expect(seen).toEqual([null, "relative", "absolute"]);
  });

  it("instances are independent", () => {
    const a = createPrintStore();
    const b = createPrintStore();
    a.rows = 5;
    a.items = [{ path: "/x.jpg", version_id: 1 }];
    expect(b.rows).toBe(2);
    expect(b.items).toEqual([]);
  });
});

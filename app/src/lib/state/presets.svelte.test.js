import { describe, it, expect } from "vitest";
import { createPresetsStore } from "./presets.svelte.js";

describe("PresetsStore", () => {
  it("starts empty with every dialog closed and no operation in flight", () => {
    const p = createPresetsStore();
    expect(p.list).toEqual([]);
    expect([p.creatingPreset, p.applyingPreset, p.creatingSnapshot, p.confirmingReset, p.copySettingsDialogOpen, p.pastingSettingsToSelection]).toEqual([false, false, false, false, false, false]);
    expect(p.confirmingDeletePresetId).toBeNull();
  });

  it("each factory call gets its own state", () => {
    const a = createPresetsStore();
    const b = createPresetsStore();
    a.list = [/** @type {any} */ ({ id: 1, name: "x" })];
    a.creatingPreset = true;
    a.confirmingDeletePresetId = 1;
    expect(b.list).toEqual([]);
    expect(b.creatingPreset).toBe(false);
    expect(b.confirmingDeletePresetId).toBeNull();
  });

  it("list is replaced immutably (a spread-append is visible)", () => {
    const p = createPresetsStore();
    p.list = [...p.list, /** @type {any} */ ({ id: 1 })];
    p.list = [...p.list, /** @type {any} */ ({ id: 2 })];
    expect(p.list.map((x) => x.id)).toEqual([1, 2]);
  });
});

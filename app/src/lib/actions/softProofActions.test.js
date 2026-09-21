import { describe, it, expect, vi, beforeEach } from "vitest";

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

import { handleChooseCustomProfile } from "./softProofActions.js";
import { softProof } from "$lib/state/softProof.svelte.js";

beforeEach(() => {
  vi.clearAllMocks();
  softProof.target = "srgb";
  softProof.customProfilePath = null;
});

describe("handleChooseCustomProfile", () => {
  it("offers a single-file ICC/ICM picker", async () => {
    dialog.open.mockResolvedValue(null);
    await handleChooseCustomProfile();
    expect(dialog.open).toHaveBeenCalledWith({ multiple: false, filters: [{ name: "ICC Profile", extensions: ["icc", "icm"] }] });
  });

  it("stores the chosen path and switches the target to custom", async () => {
    dialog.open.mockResolvedValue("/icc/paper.icc");
    await handleChooseCustomProfile();
    expect(softProof.customProfilePath).toBe("/icc/paper.icc");
    expect(softProof.target).toBe("custom");
  });

  it.each([[null], [""], [["/a.icc", "/b.icc"]]])("a cancelled or non-single result (%j) changes nothing", async (result) => {
    dialog.open.mockResolvedValue(result);
    await handleChooseCustomProfile();
    expect(softProof.customProfilePath).toBeNull();
    expect(softProof.target).toBe("srgb");
  });
});

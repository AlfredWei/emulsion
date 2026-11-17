import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
const openPath = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath, revealItemInDir: vi.fn() }));

import { getModelEdition, openThirdPartyLibraries, openThirdPartyNotices } from "./system.js";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("the licence documents in Settings > About", () => {
  it("asks the backend to open the notices file, not the webview's opener scope", async () => {
    invoke.mockResolvedValue(undefined);
    await openThirdPartyNotices();
    expect(invoke).toHaveBeenCalledWith("open_third_party_notices");
    expect(openPath).not.toHaveBeenCalled();
  });

  it("asks the backend to open the library licences file, not the webview's opener scope", async () => {
    invoke.mockResolvedValue(undefined);
    await openThirdPartyLibraries();
    expect(invoke).toHaveBeenCalledWith("open_third_party_libraries");
    expect(openPath).not.toHaveBeenCalled();
  });

  it("passes the reason on when the file was not found", async () => {
    invoke.mockRejectedValue("THIRD_PARTY_LIBRARIES.md was not found next to the app");
    await expect(openThirdPartyLibraries()).rejects.toMatch(/not found/);
    expect(openPath).not.toHaveBeenCalled();
  });
});

describe("the model edition", () => {
  it("asks the backend for the running edition", async () => {
    invoke.mockResolvedValue({ name: "test-no-lama", excluded: ["inpaint"] });
    await expect(getModelEdition()).resolves.toEqual({ name: "test-no-lama", excluded: ["inpaint"] });
    expect(invoke).toHaveBeenCalledWith("model_edition");
  });
});

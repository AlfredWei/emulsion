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
  it("opens the notices file the backend located", async () => {
    invoke.mockResolvedValue("/app/THIRD_PARTY_NOTICES.md");
    await openThirdPartyNotices();
    expect(invoke).toHaveBeenCalledWith("third_party_notices_path");
    expect(openPath).toHaveBeenCalledWith("/app/THIRD_PARTY_NOTICES.md");
  });

  it("opens the library licences file the backend located", async () => {
    invoke.mockResolvedValue("/app/THIRD_PARTY_LIBRARIES.md");
    await openThirdPartyLibraries();
    expect(invoke).toHaveBeenCalledWith("third_party_libraries_path");
    expect(openPath).toHaveBeenCalledWith("/app/THIRD_PARTY_LIBRARIES.md");
  });

  it("opens nothing, and passes the reason on, when the file was not found", async () => {
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

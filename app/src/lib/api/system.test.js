import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
const openPath = vi.hoisted(() => vi.fn());
const revealItemInDir = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath, revealItemInDir }));

import { getModelEdition, openFolder, openThirdPartyLibraries, openThirdPartyNotices, revealInFileManager } from "./system.js";

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

describe("reveal and open", () => {
  it("reveals a file through the backend command, not the opener plugin", async () => {
    invoke.mockResolvedValue(undefined);
    await revealInFileManager("/photos/a.dng");
    expect(invoke).toHaveBeenCalledWith("reveal_in_file_manager", { path: "/photos/a.dng" });
    expect(revealItemInDir).not.toHaveBeenCalled();
  });

  it("opens a folder through the backend command, not the opener plugin", async () => {
    invoke.mockResolvedValue(undefined);
    await openFolder("/exports/day1");
    expect(invoke).toHaveBeenCalledWith("open_folder", { path: "/exports/day1" });
    expect(openPath).not.toHaveBeenCalled();
  });

  it("passes the backend's reason on when a path cannot be opened", async () => {
    invoke.mockRejectedValue("no such file");
    await expect(revealInFileManager("/gone.dng")).rejects.toBe("no such file");
    await expect(openFolder("/gone")).rejects.toBe("no such file");
  });
});

describe("the model edition", () => {
  it("asks the backend for the running edition", async () => {
    invoke.mockResolvedValue({ name: "test-no-lama", excluded: ["inpaint"] });
    await expect(getModelEdition()).resolves.toEqual({ name: "test-no-lama", excluded: ["inpaint"] });
    expect(invoke).toHaveBeenCalledWith("model_edition");
  });
});

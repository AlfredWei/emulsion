import { describe, it, expect } from "vitest";
import { classifyGpuFailure } from "./gpuFallback.js";

describe("classifyGpuFailure", () => {
  it("classifies a missing navigator.gpu", () => {
    const result = classifyGpuFailure(new Error("navigator.gpu is undefined in this webview"));
    expect(result.reason).toBe("no-webgpu");
    expect(result.message.length).toBeGreaterThan(0);
  });

  it("classifies a null adapter", () => {
    const result = classifyGpuFailure(new Error("requestAdapter() returned null"));
    expect(result.reason).toBe("adapter-unavailable");
    expect(result.message.length).toBeGreaterThan(0);
  });

  it("classifies an unrecognized device-request failure", () => {
    const result = classifyGpuFailure(new Error("some GPUDevice-specific rejection text"));
    expect(result.reason).toBe("device-request-failed");
    expect(result.message.length).toBeGreaterThan(0);
  });

  it("handles a non-Error thrown value without crashing", () => {
    const result = classifyGpuFailure("a plain string error");
    expect(result.reason).toBe("device-request-failed");
    expect(result.message.length).toBeGreaterThan(0);
  });
});

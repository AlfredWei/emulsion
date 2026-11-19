import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (/** @type {string} */ rel) => JSON.parse(readFileSync(new URL(`../src-tauri/${rel}`, import.meta.url), "utf8"));
const conf = read("tauri.conf.json");
const capability = read("capabilities/default.json");

/** @param {string} name */
const directive = (name) => {
  const found = String(conf.app.security.csp)
    .split(";")
    .map((d) => d.trim().split(/\s+/))
    .find(([n]) => n === name);
  return found ? found.slice(1) : null;
};

describe("the content security policy", () => {
  it("is set, with nothing that runs injected script", () => {
    expect(conf.app.security.csp).toEqual(expect.any(String));
    expect(directive("script-src")).toEqual(["'self'"]);
    expect(directive("object-src")).toEqual(["'none'"]);
  });

  it("lets the map draw tiles from any HTTPS server and from a local one", () => {
    expect(directive("img-src")).toEqual(expect.arrayContaining(["https:", "http://localhost:*", "http://127.0.0.1:*"]));
  });

  it("lets Develop and the Library read previews over the asset protocol and call the backend", () => {
    expect(directive("connect-src")).toEqual(expect.arrayContaining(["ipc:", "http://ipc.localhost", "asset:", "http://asset.localhost"]));
    expect(directive("img-src")).toEqual(expect.arrayContaining(["asset:", "http://asset.localhost", "data:", "blob:"]));
  });

  it("lets a spec read a data: URL with fetch, as develop-adaptive-presets does", () => {
    expect(directive("connect-src")).toContain("data:");
  });

  it("does not let the page connect to the open internet", () => {
    expect(directive("connect-src")).not.toContain("https:");
    expect(directive("connect-src")).not.toContain("*");
  });
});

describe("the main window's capability", () => {
  it("grants no opener permission that reaches the file system", () => {
    const text = JSON.stringify(capability.permissions);
    expect(text).not.toContain("allow-open-path");
    expect(text).not.toContain("reveal-item-in-dir");
    expect(text).not.toContain("opener:default");
    expect(text).not.toContain("**");
  });

  it("keeps opening web links", () => {
    expect(capability.permissions).toEqual(expect.arrayContaining(["opener:allow-open-url", "opener:allow-default-urls"]));
  });
});

describe("the store metadata", () => {
  it("files the app under Photography", () => {
    expect(conf.bundle.category).toBe("Photography");
  });
});

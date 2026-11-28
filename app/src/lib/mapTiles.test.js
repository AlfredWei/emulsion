import { describe, it, expect } from "vitest";
import { DEFAULT_TILE_URL, tileLayerConfig, tileServerHost } from "./mapTiles.js";

describe("tileLayerConfig", () => {
  it("uses OpenStreetMap, with its required credit link, when nothing is set", () => {
    for (const settings of [null, undefined, {}, { tile_url: null, attribution: null }, { tile_url: "  " }]) {
      const c = tileLayerConfig(settings);
      expect(c.url).toBe(DEFAULT_TILE_URL);
      expect(c.custom).toBe(false);
      expect(c.attributionHtml).toContain("openstreetmap.org/copyright");
    }
  });

  it("uses the saved tile address and shows the person's credit as plain text", () => {
    const c = tileLayerConfig({ tile_url: " https://t.example/{z}/{x}/{y}.png ", attribution: "© Example Maps" });
    expect(c).toEqual({ url: "https://t.example/{z}/{x}/{y}.png", attributionHtml: "© Example Maps", custom: true });
  });

  it("escapes markup in the credit instead of rendering it", () => {
    const c = tileLayerConfig({ tile_url: "https://t.example/{z}/{x}/{y}.png", attribution: `<img src=x onerror="alert(1)"> & 'co'` });
    expect(c.attributionHtml).not.toContain("<img");
    expect(c.attributionHtml).toContain("&lt;img");
    expect(c.attributionHtml).toContain("&amp;");
    expect(c.attributionHtml).toContain("&quot;alert(1)&quot;");
  });

  it("never shows a custom provider with no credit as OpenStreetMap", () => {
    const c = tileLayerConfig({ tile_url: "https://t.example/{z}/{x}/{y}.png", attribution: "  " });
    expect(c.custom).toBe(true);
    expect(c.attributionHtml).toBe("Custom map tiles");
  });
});

describe("tileServerHost", () => {
  it("names the server the tiles come from, by default OpenStreetMap's", () => {
    expect(tileServerHost(null)).toBe("tile.openstreetmap.org");
    expect(tileServerHost({ tile_url: "https://tiles.example.com:8443/{z}/{x}/{y}.png?key=abc", attribution: "x" })).toBe("tiles.example.com:8443");
    expect(tileServerHost({ tile_url: "http://localhost:48081/{z}/{x}/{y}.png", attribution: "x" })).toBe("localhost:48081");
    expect(tileServerHost({ tile_url: "https://{s}.tile.example.com/{z}/{x}/{y}.png", attribution: "x" })).toBe("{s}.tile.example.com");
  });
});

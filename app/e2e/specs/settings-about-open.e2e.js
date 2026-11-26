import { OPEN_SOURCE_URL } from "../../src/lib/openSource.js";

describe("Settings > About: the notice and library buttons", () => {
  before(async () => {
    await browser.setTimeout({ script: 30000 });
    await browser.execute(() => window.__TAURI__.event.emit("menu-action", "preferences"));
    await browser.pause(300);
    await browser.execute(() => {
      Array.from(document.querySelectorAll(".tab-btn"))
        .find((b) => b.textContent === "About")
        ?.click();
    });
  });

  it("states the product licence and links the public LibRaw source", async () => {
    const about = await browser.execute(() => ({
      licence: document.querySelector('[data-testid="about-licence"]')?.textContent ?? "",
      link: document.querySelector('[data-testid="about-source-link"]')?.getAttribute("href") ?? "",
    }));
    expect(about.licence).toContain("proprietary software");
    expect(about.link).toBe(OPEN_SOURCE_URL);
  });

  for (const label of ["Open third-party notices", "Open library licences"]) {
    it(`"${label}" opens its file without an error`, async () => {
      const shown = await browser.execute((text) => {
        const btn = Array.from(document.querySelectorAll("button")).find((b) => b.textContent === text);
        btn?.click();
        return btn !== undefined;
      }, label);
      expect(shown).toBe(true);
      await browser.pause(1500);
      const error = await browser.execute(() => document.querySelector(".status")?.textContent ?? null);
      expect(error).toBeNull();
    });
  }
});

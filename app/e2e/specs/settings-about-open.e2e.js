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

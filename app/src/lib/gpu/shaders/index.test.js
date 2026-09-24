import { describe, it, expect } from "vitest";
import { WGSL } from "./index.js";

describe("assembled WGSL source", () => {
  it("defines every entry point the Develop pipelines are built from", () => {
    const entryPoints = [
      "vs_main", "fs_original", "fs_grade", "fs_lens_correct", "fs_perspective",
      "fs_sharpen_h", "fs_sharpen_v", "fs_lumaNR_h", "fs_lumaNR_v", "fs_colorNR_h", "fs_colorNR_v",
      "fs_texture_h", "fs_texture_v",
      "fs_clarity_meanp_h", "fs_clarity_meanp_v", "fs_clarity_corrp_h", "fs_clarity_corrp_v",
      "fs_clarity_a", "fs_clarity_b", "fs_clarity_meana_h", "fs_clarity_meana_v",
      "fs_clarity_meanb_h", "fs_clarity_meanb_v", "fs_clarity_v",
      "fs_atm_reduce",
      "fs_min_channel", "fs_min_h", "fs_min_v",
      "fs_dehaze_meanguide_h", "fs_dehaze_meanguide_v", "fs_dehaze_meanp_h", "fs_dehaze_meanp_v",
      "fs_dehaze_corrguide_h", "fs_dehaze_corrguide_v", "fs_dehaze_corrguidep_h", "fs_dehaze_corrguidep_v",
      "fs_dehaze_a", "fs_dehaze_b", "fs_dehaze_meana_h", "fs_dehaze_meana_v",
      "fs_dehaze_meanb_h", "fs_dehaze_meanb_v", "fs_dehaze_refine",
      "fs_premask", "fs_mask",
    ];
    for (const name of entryPoints) {
      expect(WGSL, name).toContain(`fn ${name}(`);
    }
  });

  it("declares each uniform/texture binding exactly once", () => {
    const bindings = [...WGSL.matchAll(/@group\(0\) @binding\((\d+)\)/g)].map((m) => Number(m[1]));
    expect(new Set(bindings).size).toBe(bindings.length);
  });
});

import { describe, it, expect } from "vitest";
import { removalProblem } from "./removalFit.js";

describe("removalProblem", () => {
  it("says nothing until the catalog was asked", () => {
    expect(removalProblem(undefined, 1.5)).toBeNull();
  });
  it("a removal with no row is missing, whatever the photograph", () => {
    expect(removalProblem(null, 1.5)).toBe("missing");
    expect(removalProblem(null, 0)).toBe("missing");
  });
  it("a patch of the same aspect ratio fits, also on a smaller decode", () => {
    expect(removalProblem({ w: 4000, h: 3000, hole: true }, 4 / 3)).toBeNull();
    expect(removalProblem({ w: 1280, h: 960, hole: false }, 4 / 3)).toBeNull();
  });
  it("tolerates a pixel of rounding, not another shape", () => {
    expect(removalProblem({ w: 1281, h: 960, hole: true }, 4 / 3)).toBeNull(); // 0.08 percent
    expect(removalProblem({ w: 4000, h: 3000, hole: true }, 3 / 2)).toBe("no longer fits");
    expect(removalProblem({ w: 3000, h: 4000, hole: true }, 4 / 3)).toBe("no longer fits"); // rotated
  });
  it("does not judge the fit before the photograph's size is known", () => {
    expect(removalProblem({ w: 4000, h: 3000, hole: true }, 0)).toBeNull();
    expect(removalProblem({ w: 4000, h: 3000, hole: true }, NaN)).toBeNull();
  });
});

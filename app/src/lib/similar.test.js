import { describe, it, expect } from "vitest";
import { buildGroupViews, rejectOthersTargets, similarityLabel, rejectOthersMessage } from "./similar.js";

const img = (/** @type {number} */ id, /** @type {object} */ over = {}) =>
  /** @type {any} */ ({ image_id: id, version_id: id * 10, path: `/p/${id}.jpg`, flag: "none", rating: 0, color_label: "none", ...over });
const member = (/** @type {number} */ id, /** @type {object} */ over = {}) => ({ image_id: id, version_id: id * 10, relative_sharpness: 1, soft: false, ...over });

describe("buildGroupViews", () => {
  it("joins members with their photos in order and marks the suggested frame", () => {
    const groups = [{ members: [member(1, { relative_sharpness: 0.4, soft: true }), member(2)], best_image_id: 2 }];
    const [view] = buildGroupViews(groups, [img(1), img(2), img(3)]);
    expect(view.key).toBe(1);
    expect(view.frames.map((f) => f.image.image_id)).toEqual([1, 2]);
    expect(view.frames.map((f) => f.soft)).toEqual([true, false]);
    expect(view.frames.map((f) => f.relative)).toEqual([0.4, 1]);
    expect(view.best?.image.image_id).toBe(2);
  });

  it("has no best when the backend suggested none", () => {
    const [view] = buildGroupViews([{ members: [member(1), member(2)], best_image_id: null }], [img(1), img(2)]);
    expect(view.best).toBeNull();
    expect(view.frames.every((f) => !f.best)).toBe(true);
  });

  it("drops a removed member, and a group left with fewer than two frames", () => {
    const groups = [
      { members: [member(1), member(2), member(3)], best_image_id: null },
      { members: [member(4), member(5)], best_image_id: null },
    ];
    const views = buildGroupViews(groups, [img(1), img(3), img(4)]);
    expect(views).toHaveLength(1);
    expect(views[0].frames.map((f) => f.image.image_id)).toEqual([1, 3]);
  });

  it("keeps the backend's order of groups", () => {
    const groups = [
      { members: [member(7), member(8)], best_image_id: null },
      { members: [member(1), member(2)], best_image_id: null },
    ];
    expect(buildGroupViews(groups, [1, 2, 7, 8].map((i) => img(i))).map((v) => v.key)).toEqual([7, 1]);
  });

  it("follows a photo's flag as the library updates (the photo is read from the images given)", () => {
    const groups = [{ members: [member(1), member(2)], best_image_id: null }];
    const [view] = buildGroupViews(groups, [img(1, { flag: "reject" }), img(2)]);
    expect(view.frames[0].image.flag).toBe("reject");
  });
});

describe("rejectOthersTargets", () => {
  const group = () => buildGroupViews([{ members: [member(1), member(2), member(3), member(4)], best_image_id: null }], [img(1), img(2, { flag: "reject" }), img(3), img(4, { flag: "pick" })])[0];

  it("is every frame but the kept one that is not rejected already (a pick is not spared)", () => {
    expect(rejectOthersTargets(group(), 10)).toEqual([30, 40]);
  });

  it("is empty when everything else is already rejected", () => {
    const g = buildGroupViews([{ members: [member(1), member(2)], best_image_id: null }], [img(1), img(2, { flag: "reject" })])[0];
    expect(rejectOthersTargets(g, 10)).toEqual([]);
  });

  it("never includes the kept frame even if it is rejected", () => {
    expect(rejectOthersTargets(group(), 20)).toEqual([10, 30, 40]);
  });
});

describe("labels", () => {
  it("states the time window the slider means, matching the backend's 2 to 18 s", () => {
    expect(similarityLabel(0)).toBe("taken within 2 s of each other");
    expect(similarityLabel(0.5)).toBe("taken within 10 s of each other");
    expect(similarityLabel(1)).toBe("taken within 18 s of each other");
    expect(similarityLabel(9)).toBe("taken within 18 s of each other");
  });

  it("says plainly what rejecting does and how to undo it", () => {
    expect(rejectOthersMessage(3)).toContain("3 others get the Reject flag");
    expect(rejectOthersMessage(1)).toContain("The other photo gets the Reject flag");
    expect(rejectOthersMessage(3)).toContain("no file is changed");
  });
});

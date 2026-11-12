import { describe, it, expect } from "vitest";
import { eyesBadge, suggestedFrame, EYES_TIEBREAK_MIN_RELATIVE, buildGroupViews, rejectOthersTargets, similarityLabel, rejectOthersMessage } from "./similar.js";

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

describe("eyesBadge", () => {
  const b = (/** @type {object} */ o) => ({ faces: 5, judged: 3, closed: 0, one_closed: 0, ...o });

  it("says nothing without an answer, or when no face could be judged", () => {
    expect(eyesBadge(undefined)).toBeNull();
    expect(eyesBadge(null)).toBeNull();
    expect(eyesBadge(b({ judged: 0, closed: 0 }))).toBeNull();
  });

  it("says nothing when every judged face has its eyes open", () => {
    expect(eyesBadge(b({}))).toBeNull();
  });

  it("names a closed-eyes face, with a count only for several", () => {
    expect(eyesBadge(b({ closed: 1 }))?.text).toBe("Eyes closed");
    expect(eyesBadge(b({ closed: 2 }))?.text).toBe("Eyes closed ×2");
    expect(eyesBadge(b({ closed: 1 }))?.title).toBe("Both eyes closed in 1 of 3 faces checked (5 found)");
  });

  it("puts closed before one-closed, and still names a wink when that is all there is", () => {
    expect(eyesBadge(b({ closed: 1, one_closed: 2 }))?.text).toBe("Eyes closed");
    expect(eyesBadge(b({ one_closed: 1 }))?.text).toBe("One eye closed");
    expect(eyesBadge(b({ judged: 1, one_closed: 1 }))?.title).toContain("1 of 1 face checked");
  });
});

describe("suggestedFrame (the eyes as a tiebreak)", () => {
  const frame = (/** @type {number} */ id, /** @type {object} */ over = {}) => ({ image: img(id), relative: 1, soft: false, best: false, ...over });
  const eyes = (/** @type {Record<number, number>} */ closedByImage) => Object.fromEntries(Object.entries(closedByImage).map(([id, closed]) => [id, { closed }]));

  it("changes nothing without an eye check, or when nobody has closed eyes", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: 0.9 })];
    expect(suggestedFrame(frames, null)).toEqual({ frame: frames[0], change: "none" });
    expect(suggestedFrame(frames, eyes({ 1: 0, 2: 0 }))).toEqual({ frame: frames[0], change: "none" });
  });

  it("keeps the sharpest frame when its own eyes are fine, whatever the others show", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: 0.95 })];
    expect(suggestedFrame(frames, eyes({ 2: 1 })).frame).toBe(frames[0]);
  });

  it("moves the suggestion off a closed-eyes frame to the sharpest frame inside the margin with open eyes", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: 0.85 }), frame(3, { relative: 0.95 })];
    const r = suggestedFrame(frames, eyes({ 1: 1 }));
    expect(r).toEqual({ frame: frames[2], change: "moved" });
  });

  it("does not move it to a frame outside the margin or a soft one, and then keeps the suggestion", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: EYES_TIEBREAK_MIN_RELATIVE - 0.01 }), frame(3, { relative: 0.9, soft: true })];
    expect(suggestedFrame(frames, eyes({ 1: 1 }))).toEqual({ frame: frames[0], change: "none" });
  });

  it("does not move it to another closed-eyes frame", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: 0.9 })];
    expect(suggestedFrame(frames, eyes({ 1: 1, 2: 2 }))).toEqual({ frame: frames[0], change: "none" });
  });

  it("a frame the check could not judge is not held against it", () => {
    const frames = [frame(1, { best: true }), frame(2, { relative: 0.9 })];
    expect(suggestedFrame(frames, { 1: { closed: 1 } }).frame).toBe(frames[1]);
  });

  it("suggests a frame when there was none, only because another frame has closed eyes", () => {
    const frames = [frame(1), frame(2, { relative: 0.9 }), frame(3, { relative: 0.5 })];
    expect(suggestedFrame(frames, eyes({ 1: 1 }))).toEqual({ frame: frames[1], change: "added" });
    expect(suggestedFrame(frames, eyes({ 1: 0, 2: 0 })).frame).toBeNull();
  });
});

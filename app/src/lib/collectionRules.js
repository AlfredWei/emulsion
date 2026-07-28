// Smart Collection rule evaluation (M2 Slice 5) -- entirely client-side.
// Rules round-trip through the backend as opaque JSON (catalog.js's
// CollectionRule typedef); this is the one place that actually interprets
// them, reused both for filtering the Library grid and for computing a
// smart collection's live count in the rail. Rules are ANDed together, no
// OR/nesting for V1.

/** @typedef {Map<number, Set<number>>} KeywordIdsByImage */

/** Builds the image_id -> Set<keyword_id> map `matchesRules` needs for
 * "has keyword"/"untagged" rules, from listAllImageKeywords()'s flat list.
 * @param {import('./api/catalog.js').ImageKeywordAssignment[]} assignments
 * @returns {KeywordIdsByImage} */
export function buildKeywordIdsByImage(assignments) {
  /** @type {KeywordIdsByImage} */
  const map = new Map();
  for (const { image_id, keyword_id } of assignments) {
    if (!map.has(image_id)) map.set(image_id, new Set());
    map.get(image_id)?.add(keyword_id);
  }
  return map;
}

/**
 * @param {import('./api/catalog.js').ImageSummary} image
 * @param {import('./api/catalog.js').CollectionRule[]} rules
 * @param {KeywordIdsByImage} keywordIdsByImage
 * @returns {boolean}
 */
export function matchesRules(image, rules, keywordIdsByImage) {
  return rules.every((rule) => matchesRule(image, rule, keywordIdsByImage));
}

/**
 * @param {import('./api/catalog.js').ImageSummary} image
 * @param {import('./api/catalog.js').CollectionRule} rule
 * @param {KeywordIdsByImage} keywordIdsByImage
 * @returns {boolean}
 */
function matchesRule(image, rule, keywordIdsByImage) {
  switch (rule.field) {
    case "rating": {
      const value = /** @type {number} */ (rule.value);
      if (rule.op === ">=") return image.rating >= value;
      if (rule.op === "<=") return image.rating <= value;
      return image.rating === value;
    }
    case "flag":
      return image.flag === rule.value;
    case "color_label":
      return image.color_label === rule.value;
    case "keyword": {
      const ids = keywordIdsByImage.get(image.image_id) ?? new Set();
      if (rule.op === "empty") return ids.size === 0;
      return ids.has(/** @type {number} */ (rule.value));
    }
    default:
      return false;
  }
}

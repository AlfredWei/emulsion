// The complete WGSL shader source for the Develop canvas, assembled from the section
// modules below in the order they appear in the shader. Order matters: WGSL requires
// declarations before use, and the text is byte-for-byte what used to live inline in
// DevelopCanvas.svelte.

import { common } from "./common.js";
import { gradeUniforms } from "./gradeUniforms.js";
import { detailFilters } from "./detailFilters.js";
import { gradeMath } from "./gradeMath.js";
import { lens } from "./lens.js";
import { perspective } from "./perspective.js";
import { grade } from "./grade.js";
import { dehazeLocalContrast } from "./dehazeLocalContrast.js";
import { premask } from "./premask.js";
import { mask } from "./mask.js";

export const WGSL = "\n" + [common, gradeUniforms, detailFilters, gradeMath, lens, perspective, grade, dehazeLocalContrast, premask, mask].join("") + "  ";

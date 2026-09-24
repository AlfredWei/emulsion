use super::*;

/// Same formula as `DevelopCanvas.svelte`'s WGSL fragment shader, in the
/// same order, kept in `f32` to track the shader's precision. Not required
/// to be byte-identical to the shader's GPU output (their source
/// resolutions differ anyway -- full-res export vs. the downsampled
/// Develop preview) -- sourced from the same formula, tested against the
/// same hand-derived expected values, same tolerance the shader's own
/// numeric smoke test already uses.
///
/// Local adjustments (mask ops) are applied AFTER the global exposure/
/// contrast/saturation pass, in true stack order (via the unified `Mask`
/// enum -- see its own doc comment for why parsing kinds into separate
/// `Vec`s and applying "all of one kind, then all of the other" would be a
/// real parity gap once a user interleaves linear and radial masks) --
/// matches real Lightroom's own layering (local adjustments grade on top
/// of the globally-graded image) and the WGSL shader's own order.
pub(crate) fn apply_edit_stack(image: &mut RgbImage, stack: &EditStack) {
    let exposure_ev = op_value(&stack.ops, "exposure");
    let contrast = op_value(&stack.ops, "contrast");
    let saturation = op_value(&stack.ops, "saturation");
    let temperature = op_value(&stack.ops, "temperature");
    let tint = op_value(&stack.ops, "tint");
    let highlights = op_value(&stack.ops, "highlights");
    let shadows = op_value(&stack.ops, "shadows");
    let whites = op_value(&stack.ops, "whites");
    let blacks = op_value(&stack.ops, "blacks");
    let mut masks = parse_masks(&stack.ops);
    // Built once per call, not per pixel -- see build_curve_lut's own doc
    // comment for why this must be the same discretized LUT the WGSL
    // shader consumes, not an exact per-pixel spline evaluation.
    let curve_lut = build_curve_lut(&tone_curve_points(&stack.ops));
    let bands = hsl_bands(&stack.ops);
    let split = split_toning(&stack.ops);
    // Reuses the generic scalar-op reader `op_value` already establishes
    // for exposure/contrast/saturation -- `{op:"dehaze", value: amount}` is
    // the exact same single-scalar shape, no dedicated parser needed.
    let dehaze_amount = op_value(&stack.ops, "dehaze");
    let texture_amount = op_value(&stack.ops, "texture");
    let clarity_amount = op_value(&stack.ops, "clarity");
    let vignette = vignette_op(&stack.ops);
    let grain = grain_op(&stack.ops);
    let sharpen = sharpen_op(&stack.ops);
    let luma_nr = luma_nr_op(&stack.ops);
    let color_nr = color_nr_op(&stack.ops);

    let (width, height) = (image.width(), image.height());
    // Only consumed by brush masks (see Mask::weight) -- converts a dab's
    // width-only `radius` into a true circle in pixel space regardless of
    // the image's own aspect ratio.
    let aspect = height as f32 / width as f32;
    let (w, h) = (width as usize, height as usize);

    // Pass 1: the existing global chain (exposure -> ... -> split toning),
    // written into an intermediate buffer rather than the image directly.
    // Dehaze (below) is the first op in this pipeline that needs
    // NEIGHBORING pixels' graded value -- an in-place single loop can't
    // provide that without a race (a pixel's neighbor would already be
    // overwritten with final output before this pixel could read its
    // still-graded, not-yet-finalized value). Every op above this one was a
    // pure per-pixel remap and fit directly in one loop; this one can't.
    let mut graded = vec![[0.0f32; 3]; w * h];
    for (x, y, pixel) in image.enumerate_pixels() {
        let mut rgb = apply_global_adjustments(
            [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ],
            exposure_ev,
            contrast,
            saturation,
            temperature,
            tint,
            highlights,
            shadows,
            whites,
            blacks,
        );
        rgb = [
            sample_lut(&curve_lut, rgb[0]),
            sample_lut(&curve_lut, rgb[1]),
            sample_lut(&curve_lut, rgb[2]),
        ];
        rgb = apply_hsl_bands(rgb, &bands);
        rgb = apply_split_toning(rgb, &split);
        graded[y as usize * w + x as usize] = rgb;
    }

    // Texture -> Clarity: each independently skipped at amount=0 (exact
    // passthrough, see apply_local_contrast's own doc comment), applied
    // sequentially so Clarity's blur sees Texture's already-adjusted
    // luminance -- matches Lightroom's own slider order and the GPU
    // side's pass order. Clarity uses its own guided-filter-based
    // `apply_clarity` (RFC-0010), not `apply_local_contrast` -- see that
    // function's own doc comment.
    if texture_amount != 0.0 {
        apply_local_contrast(&mut graded, w, h, TEXTURE_RADIUS, texture_amount);
    }
    if clarity_amount != 0.0 {
        apply_clarity(&mut graded, w, h, clarity_amount);
    }

    // Sharpening / Noise Reduction blur sources: computed from `graded` at
    // this SAME point -- the post-Texture/Clarity, pre-Dehaze-recovery
    // snapshot Dehaze's own atmospheric-light/dark-channel maps below
    // already read from. NAMED, ACCEPTED LIMITATION (a design review
    // flagged this explicitly): Dehaze's recovery is a spatially-varying
    // affine transform applied later, in the final loop, so these blurs
    // and the edge/detail signals derived from them are measured on the
    // pre-Dehaze image while their resulting deltas get added to the
    // post-Dehaze pixel. Materializing Dehaze's own recovery into a real
    // intermediate buffer first (so these could read the TRUE final
    // pre-Sharpen/NR image) would require also adding `dehaze_amount` to
    // every downstream cache-invalidation concern it's currently exempt
    // from -- accepted as out of scope for this slice, the same class of
    // "named, deferred" approximation Dehaze's own transmission
    // refinement (a box-mean standing in for a true guided filter) and
    // Vignette's own roundness (unimplemented) already are.
    let graded_luma: Vec<f32> = graded.iter().map(|c| luma3(*c)).collect();

    let sharpen_blur = if sharpen.amount != 0.0 {
        Some(separable_mean_filter(&graded_luma, w, h, sharpen_radius_px(sharpen.radius)))
    } else {
        None
    };
    let luma_nr_blur = if luma_nr.amount != 0.0 {
        // RFC-0012: guided_filter_self, not a plain box mean -- see
        // LUMA_NR_RADIUS's own doc comment in detail.rs.
        Some(guided_filter_self(&graded_luma, w, h, LUMA_NR_RADIUS, LUMA_NR_GUIDED_EPS))
    } else {
        None
    };
    let color_nr_blur = if color_nr.amount != 0.0 {
        let r: Vec<f32> = graded.iter().map(|c| c[0]).collect();
        let g: Vec<f32> = graded.iter().map(|c| c[1]).collect();
        let b: Vec<f32> = graded.iter().map(|c| c[2]).collect();
        Some((
            separable_mean_filter(&r, w, h, COLOR_NR_RADIUS),
            separable_mean_filter(&g, w, h, COLOR_NR_RADIUS),
            separable_mean_filter(&b, w, h, COLOR_NR_RADIUS),
        ))
    } else {
        None
    };

    // Passes 2-6: Dehaze's dark-channel-prior maps (see the doc comment
    // above `dehaze_atmospheric_light` for the corrected algorithm and its
    // two named deviations from He et al.). Skipped entirely at amount=0 --
    // not just cheap-but-computed-anyway, since these are several full-image
    // passes and most photos won't use Dehaze; this also keeps "amount=0 is
    // an exact passthrough" a structural guarantee (the final loop below
    // simply has nothing to blend toward), not a numerically-near-zero one.
    let dehaze_maps = if dehaze_amount != 0.0 {
        let a = dehaze_atmospheric_light(&graded);
        let min_channel: Vec<f32> = graded
            .iter()
            .map(|c| (c[0] / a[0]).min(c[1] / a[1]).min(c[2] / a[2]))
            .collect();
        let dark_channel = separable_min_filter(&min_channel, w, h, DEHAZE_PATCH_RADIUS);
        let t_raw: Vec<f32> = dark_channel.iter().map(|d| 1.0 - DEHAZE_OMEGA * d).collect();
        // RFC-0011: guided by the graded image's own luma, not a plain box
        // mean of t_raw -- see guided_filter's own doc comment for why this
        // is the general two-signal case, not guided_filter_self.
        let graded_luma: Vec<f32> = graded.iter().map(|c| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722).collect();
        let t_refined = guided_filter(&graded_luma, &t_raw, w, h, DEHAZE_REFINE_RADIUS, DEHAZE_GUIDED_EPS);
        Some((a, t_refined))
    } else {
        None
    };

    // Pass 7: Dehaze recovery + amount blend, then Noise Reduction,
    // Sharpening, Vignette, Grain -- all per-pixel, no CROSS-pixel reads
    // besides the blur sources already fully materialized above -- written
    // into `pre_mask`, NOT `image` directly. This is the same
    // "neighbor read needs its own buffer pass" reasoning `graded`'s own
    // doc comment gives for Dehaze, extended one step further: a `Spot`
    // mask (M4 Slice 1, Healing/Clone brush) needs to read some OTHER
    // pixel's fully-graded value (its `source` point) while computing
    // `dest`'s -- fusing that into this same single loop would make the
    // result depend on raster scan order (whichever of `source`/`dest`
    // happens to be visited first would see a stale, not-yet-graded
    // neighbor), exactly the race `graded` was already introduced to
    // avoid. Every mask kind before Spot has no such cross-pixel
    // dependency and would have been fine fused into this loop -- the
    // split exists entirely for Spot's sake.
    let mut pre_mask = vec![[0.0f32; 3]; w * h];
    for (x, y, _) in image.enumerate_pixels() {
        let idx = y as usize * w + x as usize;
        let graded_rgb = graded[idx];
        let mut rgb = graded_rgb;
        if let Some((a, t_refined)) = &dehaze_maps {
            let t = t_refined[idx].max(DEHAZE_T0);
            for c in 0..3 {
                let recovered = (graded_rgb[c] - a[c]) / t + a[c];
                rgb[c] = graded_rgb[c] + (recovered - graded_rgb[c]) * (dehaze_amount / 100.0);
            }
        }

        // Noise Reduction (luminance, then color), then Sharpening -- NR
        // before Sharpen is deliberate: sharpening amplifies high-frequency
        // content, so running it after denoising avoids re-amplifying
        // noise NR would otherwise have removed. All three read the
        // pre-Dehaze-recovery blur sources computed above (see that block's
        // own doc comment for the named limitation this implies).
        if let Some(blur) = &luma_nr_blur {
            let delta = luma_nr_delta(graded_luma[idx], blur[idx], &luma_nr);
            for c in rgb.iter_mut() {
                *c += delta;
            }
        }
        if let Some((br, bg, bb)) = &color_nr_blur {
            let delta = color_nr_delta(graded_rgb, [br[idx], bg[idx], bb[idx]], &color_nr);
            for c in 0..3 {
                rgb[c] += delta[c];
            }
        }
        if let Some(blur) = &sharpen_blur {
            let grad_mag = local_gradient_magnitude(&graded_luma, w, h, x as usize, y as usize);
            let delta = sharpen_delta(graded_luma[idx], blur[idx], grad_mag, &sharpen);
            for c in rgb.iter_mut() {
                *c += delta;
            }
        }

        // Pixel-center sampling, matching how a texture lookup samples at
        // the middle of a texel -- not required to be exact, same "not
        // byte-identical, tested to tolerance" bar as the rest of this
        // module.
        let uv = (
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / height as f32,
        );

        let vf = vignette_factor(uv, aspect, &vignette);
        for c in rgb.iter_mut() {
            *c *= vf;
        }

        let gd = grain_delta((x as f32, y as f32), &grain);
        for c in rgb.iter_mut() {
            *c += gd;
        }

        pre_mask[idx] = rgb;
    }

    // `heal_shift` depends only on each Spot mask's own (dest, radius,
    // source) geometry, not on any per-pixel state, so it's computed once
    // per mask here rather than once per pixel inside Pass 8's loop --
    // same "precompute what doesn't vary per pixel" discipline as
    // `curve_lut`/`bands`/`vignette`/`grain` above. Needs `pre_mask` to
    // exist first (see this mask kind's own doc comment on `heal_shift`).
    for mask in &mut masks {
        if let Mask::Spot(m) = mask {
            if m.heal {
                m.heal_shift = compute_heal_shift(m, &pre_mask, w, h, aspect);
            }
        }
    }

    // Pass 8 (final): the mask loop, reading `pre_mask` as its accumulator
    // base instead of continuing to mutate an in-loop `rgb` from Pass 7 --
    // required so `Spot`'s `local_color` can look up a DIFFERENT pixel's
    // `pre_mask` entry (see `Mask::local_color`'s own doc comment) and
    // have it already be the true final pre-mask value, not a
    // scan-order-dependent partial one.
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let idx = y as usize * w + x as usize;
        let mut rgb = pre_mask[idx];
        let uv = (
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / height as f32,
        );

        if !masks.is_empty() {
            for mask in &masks {
                let weight = mask.weight(uv, aspect, rgb);
                let local = mask.local_color(uv, rgb, &pre_mask, w, h);
                for c in 0..3 {
                    rgb[c] += (local[c] - rgb[c]) * weight;
                }
            }
        }

        for (channel, value) in pixel.0.iter_mut().zip(rgb.iter()) {
            *channel = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// RFC-0013: the 12 op-bearing Develop panels' own key -> op-name mapping,
/// the single source of truth `effective_stack_for_render` and every
/// per-panel reset action reads. Soft Proof, Crop, and masks are
/// deliberately absent -- see the RFC's own §2 for why each is out of
/// scope (Soft Proof isn't an edit-stack op at all; Crop and masks weren't
/// part of what was asked).
pub(crate) const PANEL_OP_NAMES: &[(&str, &[&str])] = &[
    (
        "basic",
        &[
            "exposure", "contrast", "saturation", "temperature", "tint", "highlights", "shadows", "whites", "blacks",
        ],
    ),
    ("tone_curve", &["tone_curve"]),
    ("hsl", &["hsl"]),
    ("split_toning", &["split_toning"]),
    ("texture_clarity", &["texture", "clarity"]),
    ("dehaze", &["dehaze"]),
    ("sharpening", &["sharpen"]),
    ("noise_reduction", &["luma_nr", "color_nr"]),
    ("vignette", &["vignette"]),
    ("grain", &["grain"]),
    ("lens_corrections", &["lens_correction"]),
    ("perspective", &["perspective"]),
];

/// Strips every op belonging to a hidden panel (per `{"op": "panel_hidden",
/// "panel": "<key>"}` markers), and the markers themselves, before any
/// apply_* function ever sees the stack -- so `apply_edit_stack`/
/// `apply_lens_correction`/`apply_perspective`/`apply_crop` stay completely
/// unaware panel visibility exists; a hidden panel's ops are, to them,
/// simply absent, the same as if the user had never touched that panel at
/// all (every op's own getter already falls back to its own identity
/// default when its op is absent -- see RFC-0013 §3/§4). Called once at
/// each of the three real render entry points (`preview_cache.rs`,
/// `export.rs`, `import.rs`'s thumbnail regeneration), ahead of their
/// existing four `apply_*` calls -- those four functions, and every
/// existing Rust unit test that builds an `EditStack` directly and calls
/// them, are untouched by this change.
pub(crate) fn effective_stack_for_render(stack: &EditStack) -> EditStack {
    let hidden: std::collections::HashSet<&str> = stack
        .ops
        .iter()
        .filter(|op| op.get("op").and_then(|v| v.as_str()) == Some("panel_hidden"))
        .filter_map(|op| op.get("panel").and_then(|v| v.as_str()))
        .collect();
    if hidden.is_empty() {
        return stack.clone();
    }
    let hidden_op_names: std::collections::HashSet<&str> = PANEL_OP_NAMES
        .iter()
        .filter(|(panel, _)| hidden.contains(panel))
        .flat_map(|(_, names)| names.iter().copied())
        .collect();
    let ops = stack
        .ops
        .iter()
        .filter(|op| {
            let name = op.get("op").and_then(|v| v.as_str());
            match name {
                Some("panel_hidden") => false,
                Some(n) => !hidden_op_names.contains(n),
                None => true,
            }
        })
        .cloned()
        .collect();
    EditStack { schema_version: stack.schema_version, ops }
}

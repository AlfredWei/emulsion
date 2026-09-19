// The fs_mask pass: local adjustments composited over the graded image.
// Part of the WGSL source, split out of DevelopCanvas.svelte; see index.js for the
// concatenation order, which must not change.
export const mask = `    // Final pass: reads preMaskTex (this pass's own predecessor's output,
    // NOT gradedTex -- Dehaze/NR/Sharpen/Vignette/Grain are all already
    // baked in, see fs_premask's own doc comment above for why those
    // needed to be a separate pass this slice), runs the mask loop, then
    // the clipping-overlay toggle, writing the real swapchain output.
    // Split out of what used to be fs_final's own single pass specifically
    // so a spot mask (kind=5, M4 Slice 1) can read some OTHER pixel's
    // fully-graded value without a scan-order hazard -- see preMaskTex's
    // own doc comment for the full reasoning.
    @fragment
    fn fs_mask(in: VertexOut) -> @location(0) vec4<f32> {
      // Use in.uv (0..1, independent of render-target size) rather than
      // in.position.xy (relative to THIS pass's own render target) -- this
      // pipeline is reused to draw into both the main canvas (matching
      // preMaskTex's size) and the small fixed-size histogramTex, so a
      // position-based coord silently only sampled preMaskTex's top-left
      // corner when drawing into the smaller target.
      let dims = vec2<i32>(textureDimensions(preMaskTex));
      let coord = clamp(vec2<i32>(in.uv * vec2<f32>(dims)), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
      var rgb = textureLoad(preMaskTex, coord, 0).rgb;

      // Local adjustments layer on top of the globally-graded image,
      // matching real Lightroom's own ordering and develop_engine.rs's.
      let mask_count = i32(adj.mask_count);
      for (var i = 0; i < mask_count; i = i + 1) {
        let m = masks[i];
        let kind = m.params.z;
        var weight: f32;
        // Ascending bands, not the two-comparison overlapping-threshold
        // chain this used to be (a real bug a design review caught: that
        // older chain only correctly bucketed kinds 0/1/2 by coincidence,
        // and adding a 4th kind would have silently aliased it onto the
        // brush branch). Each else-if here bounds exactly one kind, so a
        // future 5th kind just needs one more band inserted before the
        // final else, not a re-audit of the whole chain's ordering.
        if (kind < 0.5) {
          // Linear: projection-onto-segment parametrization. 0 at start, 1
          // at end, extrapolated linearly beyond both, then clamped.
          let dir = m.start_end.zw - m.start_end.xy;
          let len2 = max(dot(dir, dir), 0.000001);
          let t = dot(in.uv - m.start_end.xy, dir) / len2;
          // Feather widens the transition band symmetrically around the
          // midpoint -- at feather=50 the pins themselves move to weight
          // 0.25/0.75 rather than staying at 0/1, matching real Lightroom's
          // own gradient-feather model (separate outer feather lines beyond
          // the pins), not a corner-only softening.
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          weight = clamp((t + softness) / (1.0 + 2.0 * softness), 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 1.5) {
          // Radial: start_end.xy = center, start_end.zw = (radiusX,
          // radiusY). d is 0 at center, 1 at the ellipse boundary. At
          // feather=0 the transition band is d in [0.999, 1.0] (width
          // 0.001, sitting just inside the boundary, not symmetric around
          // it); widens to roughly d in [0.001,1.999] as feather
          // approaches 100. insideWeight is ~1 at/near the center
          // regardless of feather.
          let dx = (in.uv.x - m.start_end.x) / m.start_end.z;
          let dy = (in.uv.y - m.start_end.y) / m.start_end.w;
          let d = sqrt(dx * dx + dy * dy);
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let denom = max(2.0 * softness, 0.001);
          let insideWeight = clamp((1.0 + softness - d) / denom, 0.0, 1.0);
          // Default (invert=false) applies the effect OUTSIDE the ellipse
          // -- real Lightroom's own Radial Filter convention (its classic
          // vignette use case); invert=true applies it inside (spotlight/
          // subject use case).
          weight = select(1.0 - insideWeight, insideWeight, m.params.y > 0.5);
        } else if (kind < 2.5) {
          // Brush: rasterized CPU-side into this mask's own texture-array
          // layer, luminance-as-weight (see DevelopCanvas.svelte's
          // rasterizeDab/syncBrushRasterization and develop_engine.rs's
          // dab_falloff/brush_mask_weight for the exact accumulation
          // formula both renderers agree on).
          let layer = i32(m.params.w);
          weight = textureSampleLevel(brushMasks, srcSampler, in.uv, layer, 0.0).r;
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 3.5) {
          // Luminance range: the first kind whose weight depends on pixel
          // VALUE, not position -- reads rgb as already graded by every
          // PRECEDING mask in the stack (this is a mutating accumulator,
          // see develop_engine.rs's Mask::weight doc comment for why this
          // order-dependence is the correct WYSIWYG behavior, not a bug).
          // Trapezoidal falloff, raw-then-clamp-once (same style as the
          // linear/radial formulas above) -- start_end.x/y hold
          // rangeMin/rangeMax (0-100, same scale as feather).
          let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
          let range_min = m.start_end.x / 100.0;
          let range_max = m.start_end.y / 100.0;
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let feather_width = softness * 0.5;
          let denom = max(feather_width, 0.001);
          let rising = (luma - (range_min - feather_width)) / denom;
          let falling = (range_max + feather_width - luma) / denom;
          weight = clamp(min(rising, falling), 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 4.5) {
          // Color range (4): also reads the mutating rgb accumulator,
          // same order-dependence as luminance range. One reference color
          // and one tolerance rather than two edges, so this is a
          // SINGLE-sided falloff (closer to radial's inside/outside
          // shape, but in RGB-distance space) rather than luminance's
          // two-sided min(rising,falling) band. start_end.xyz holds
          // refColor (0-1, matching this shader's own texture-sample
          // convention), start_end.w holds range (0-100).
          //
          // The exact-match pixel must get weight=1 regardless of how
          // small feather is -- an earlier draft blended feather_width
          // into the numerator alongside threshold, which meant the same
          // denom floor meant to prevent divide-by-zero at feather=0 also
          // diluted that term, so dist=0 could evaluate to LESS than full
          // weight at low feather (see develop_engine.rs's
          // color_mask_weight doc comment for the full derivation). Fixed
          // here the same way: denom is the transition width added AFTER
          // threshold via the "+ 1.0", never blended into the numerator.
          let dist = distance(rgb, m.start_end.xyz);
          let max_dist = sqrt(3.0);
          let threshold = clamp(m.start_end.w / 100.0, 0.0, 1.0) * max_dist;
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let feather_width = softness * max_dist * 0.5;
          let denom = max(feather_width, 0.001);
          weight = clamp((threshold - dist) / denom + 1.0, 0.0, 1.0);
          if (m.params.y > 0.5) { weight = 1.0 - weight; }
        } else if (kind < 5.5) {
          // Spot (5, M4 Slice 1/2, Healing/Clone brush): structurally
          // unlike every kind above -- it doesn't gate a parametric
          // adjustment, it copies pixel CONTENT from a source offset into
          // its dabs (see SpotMask's own doc comment in develop_engine.rs).
          // M4 Slice 2 (brush-like spot removal, per explicit user request)
          // made this a dab-stroke mask just like Brush (2): weight comes
          // from its OWN texture-array layer (rasterized CPU-side by
          // syncMaskRasterization/rasterizeSpotDab, matching
          // spot_mask_weight's feather formula per dab, then
          // max-accumulated across dabs the same way Brush's own texture
          // already accumulates its Add dabs), not an inline analytic
          // circle the way the original single-dest-circle design used.
          let layer = i32(m.params.w);
          weight = textureSampleLevel(brushMasks, srcSampler, in.uv, layer, 0.0).r;

          let offset = m.start_end.xy;
          let sampleUv = in.uv + offset;
          let dims = vec2<i32>(textureDimensions(preMaskTex));
          let dimsF = vec2<f32>(dims);
          let sampleCoord = clamp(vec2<i32>(sampleUv * dimsF), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
          var sampled = textureLoad(preMaskTex, sampleCoord, 0).rgb;

          // Heal (mode = adjustments.x > 0.5): shift the sampled patch by
          // the destination surround's mean color minus the source
          // surround's -- see healRingMean's own doc comment. A whole
          // stroke shares ONE shift, anchored to the dabs' own centroid
          // (start_end.zw) and average radius (params.y) -- mirroring
          // develop_engine.rs's compute_heal_shift/spot_centroid_and_radius
          // exactly, same "simplified, not true Poisson blending" scope as
          // before, just now anchored to a representative point for the
          // whole stroke instead of a single dest circle. Gated on weight
          // > 0 so the ring-sampling cost only lands on pixels actually
          // near this spot (a small circle in practice), not every pixel
          // in the image -- GPUs handle a spatially-clustered branch like
          // this reasonably (most warps near a compact circle are either
          // fully in or fully out), unlike a data-dependent branch
          // scattered across the whole frame.
          if (m.adjustments.x > 0.5 && weight > 0.001) {
            let centroid = m.start_end.zw;
            let avgRadius = m.params.y;
            let destPx = centroid * dimsF;
            let sourcePx = (centroid + offset) * dimsF;
            let radiusPx = avgRadius * dimsF.x;
            let destMean = healRingMean(destPx, radiusPx, dims);
            let sourceMean = healRingMean(sourcePx, radiusPx, dims);
            sampled = clamp(sampled + (destMean - sourceMean), vec3<f32>(0.0), vec3<f32>(1.0));
          }

          rgb = mix(rgb, sampled, weight);
        } else {
          // Red Eye (6, M4): same elliptical geometry as Radial (1) --
          // start_end.xy = center, start_end.zw = (radiusX, radiusY),
          // params.x = feather -- but always applied INSIDE (no invert;
          // the whole point is "correct what's in the oval") and gated by
          // a SECOND factor beyond ellipse membership: how strongly this
          // pixel's own color reads as red-eye red. Exact mirror of
          // develop_engine.rs's red_eye_ellipse_weight/
          // red_eye_redness_factor/red_eye_local_color -- see those
          // functions' own doc comments for the reasoning behind each
          // constant here.
          let dx = (in.uv.x - m.start_end.x) / m.start_end.z;
          let dy = (in.uv.y - m.start_end.y) / m.start_end.w;
          let d = sqrt(dx * dx + dy * dy);
          let softness = clamp(m.params.x / 100.0, 0.0, 0.999);
          let denom = max(2.0 * softness, 0.001);
          let ellipseWeight = clamp((1.0 + softness - d) / denom, 0.0, 1.0);

          // params.y repurposed as pupilSize (0-100) for this kind -- red
          // eye has no invert concept, so this slot is free (see the Mask
          // struct's own doc comment above).
          let pupilSize = m.params.y;
          let redness = max(rgb.r - max(rgb.g, rgb.b), 0.0);
          let threshold = clamp(1.0 - pupilSize / 100.0, 0.05, 1.0);
          let rednessFactor = clamp(redness / threshold, 0.0, 1.0);
          weight = ellipseWeight * rednessFactor;

          // adjustments.w repurposed as darken (0-100) for this kind --
          // unused padding for every other kind (see the Mask struct's own
          // doc comment above). Always fully desaturates to luma at full
          // weight (that's this tool's whole "de-redify" point, not
          // optional); darken additionally pulls that luma down by up to
          // 60% at darken=100, matching a real pupil's near-black look
          // without ever crushing to pure black regardless of slider
          // position.
          let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
          let darkenAmt = clamp(m.adjustments.w / 100.0, 0.0, 1.0);
          let corrected = luma * (1.0 - darkenAmt * 0.6);
          rgb = mix(rgb, vec3<f32>(corrected, corrected, corrected), weight);
        }
        if (kind < 4.5) {
          rgb = mix(rgb, apply_adjustments(rgb, m.adjustments.x, m.adjustments.y, m.adjustments.z), weight);
        }

        // Selected-mask overlay (soft colored fill, toggleable): every
        // no-geometry kind (brush, luminance range, color range -- kind
        // 2..4) -- linear/radial deliberately excluded, preserving the
        // prior explicit scope decision that they keep their existing
        // dashed-outline-only feedback (PROGRESS.md,
        // mask-overlay-feather-indicators slice); spot (5) is ALSO
        // excluded -- it always shows a real source-circle/move/
        // source-offset handle set (drawn in the DOM overlay, not here),
        // matching linear/radial's own "has geometry, no colored-fill
        // overlay" precedent, not brush's. Reuses the weight just computed
        // above for THIS
        // mask -- already invert-adjusted, already evaluated against the
        // correct (pre-this-mask) rgb state -- so no separate
        // re-sample-and-re-invert step is needed regardless of kind,
        // unlike the brush-only texture-based mechanism this replaces.
        if (kind > 1.5 && kind < 4.5 && i == i32(adj.selected_mask_index)) {
          rgb = mix(rgb, vec3<f32>(1.0, 0.24, 0.24), weight * 0.55);
        }
      }

      rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));

      // Clipping-overlay toggle (Develop histogram): blue over pixels
      // clipped to pure/near-black, red over pixels clipped to
      // pure/near-white -- the standard shadow/highlight clip-warning
      // convention. Deliberately a simplified ALL-channels-near-extreme
      // check (true black/white), not real Lightroom's own more nuanced
      // per-channel-color-coded overlay (which channel(s) clipped) --
      // named scope cut, avoids a noisier overlay that would light up on
      // any single saturated channel (e.g. a normal warm shadow with a
      // near-zero blue channel) rather than genuine highlight/shadow
      // detail loss.
      if (clipping.show_clipping > 0.5) {
        let CLIP_EPS = 0.004; // ~1/255
        let mx = max(rgb.r, max(rgb.g, rgb.b));
        let mn = min(rgb.r, min(rgb.g, rgb.b));
        if (mx <= CLIP_EPS) {
          rgb = vec3<f32>(0.15, 0.45, 1.0);
        } else if (mn >= 1.0 - CLIP_EPS) {
          rgb = vec3<f32>(1.0, 0.15, 0.15);
        }
      }

      return vec4<f32>(rgb, 1.0);
    }
`;

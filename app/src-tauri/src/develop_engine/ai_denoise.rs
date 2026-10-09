//! `ai_denoise` (M6 slice 3b, RFC-0027 §3.1-3.2): the edit-stack op that mixes a **cached** AI-denoised copy of
//! the photo into the decoded source, ahead of every other step (lens correction, grading, crop).
//!
//! The op holds only `amount` (0..100); the model never runs here. `out = source + (denoised - source) *
//! amount / 100`, per 8-bit channel, rounded once. 0 leaves the source untouched and 100 is the denoised copy.

use crate::catalog::EditStack;
use image::RgbImage;

/// The op's amount, 0 when it is absent, not a number, or not above 0; capped at 100.
pub(crate) fn ai_denoise_amount(stack: &EditStack) -> f32 {
    stack
        .ops
        .iter()
        .find(|op| op.get("op").and_then(|v| v.as_str()) == Some("ai_denoise"))
        .and_then(|op| op.get("amount"))
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32).clamp(0.0, 100.0))
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

/// Mixes `denoised` into `base` in place. Both must be the same size (the caller checks; a mismatch is a no-op
/// rather than a panic, since a wrong-sized cache entry must never take a render down).
pub(crate) fn blend_denoised(base: &mut RgbImage, denoised: &RgbImage, amount: f32) {
    if amount <= 0.0 || base.dimensions() != denoised.dimensions() {
        return;
    }
    let t = (amount / 100.0).clamp(0.0, 1.0);
    for (b, d) in base.as_mut().iter_mut().zip(denoised.as_raw()) {
        *b = (*b as f32 + (*d as f32 - *b as f32) * t + 0.5).clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stack(ops: Vec<serde_json::Value>) -> EditStack {
        EditStack { schema_version: 1, ops }
    }

    #[test]
    fn the_amount_is_read_clamped_and_defaults_to_zero() {
        assert_eq!(ai_denoise_amount(&stack(vec![])), 0.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "ai_denoise", "amount": 35})])), 35.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "ai_denoise", "amount": 250})])), 100.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "ai_denoise", "amount": -5})])), 0.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "ai_denoise"})])), 0.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "ai_denoise", "amount": "lots"})])), 0.0);
        assert_eq!(ai_denoise_amount(&stack(vec![json!({"op": "exposure", "amount": 80})])), 0.0);
    }

    #[test]
    fn the_blend_hits_both_ends_and_the_midpoint_with_one_rounding() {
        let denoised = RgbImage::from_pixel(2, 1, image::Rgb([100, 0, 255]));
        let make = || RgbImage::from_pixel(2, 1, image::Rgb([200, 100, 0]));
        let mut none = make();
        blend_denoised(&mut none, &denoised, 0.0);
        assert_eq!(none, make(), "amount 0 leaves the source untouched");
        let mut full = make();
        blend_denoised(&mut full, &denoised, 100.0);
        assert_eq!(full, denoised);
        let mut half = make();
        blend_denoised(&mut half, &denoised, 50.0);
        assert_eq!(half.get_pixel(0, 0).0, [150, 50, 128]); // 127.5 rounds up
        let mut quarter = make();
        blend_denoised(&mut quarter, &denoised, 25.0);
        assert_eq!(quarter.get_pixel(1, 0).0, [175, 75, 64]); // 63.75 -> 64
    }

    #[test]
    fn a_wrong_sized_denoised_image_changes_nothing() {
        let mut base = RgbImage::from_pixel(3, 3, image::Rgb([9, 9, 9]));
        blend_denoised(&mut base, &RgbImage::new(2, 2), 100.0);
        assert_eq!(base, RgbImage::from_pixel(3, 3, image::Rgb([9, 9, 9])));
    }
}

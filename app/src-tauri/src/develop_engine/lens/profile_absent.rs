//! A build without the `lens-profiles` feature (the Mac App Store build, ADR-0011) has no profile path. A profile
//! cannot exist here: `LensProfileData` has no values, so a stored profile op parses to `None` and renders without its
//! profile part, while the op itself stays in the catalog.

use super::{LensChannel, LensNorm, RgbImage};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::develop_engine) enum LensProfileData {}

pub(super) fn parse(_p: &serde_json::Value) -> Option<LensProfileData> {
    None
}

impl LensNorm {
    pub(in crate::develop_engine) fn for_profile(p: &LensProfileData, _width: u32, _height: u32) -> Self {
        match *p {}
    }
}

pub(super) fn correct_distortion(p: &LensProfileData, _norm: LensNorm, _x: f32, _y: f32, _amount: f32) -> (f32, f32) {
    match *p {}
}

pub(super) fn correct_tca(p: &LensProfileData, _norm: LensNorm, _channel: LensChannel, _x: f32, _y: f32, _amount: f32) -> (f32, f32) {
    match *p {}
}

pub(super) fn devignette(_image: &mut RgbImage, p: &LensProfileData, _norm: LensNorm, _amount: f32) {
    match *p {}
}

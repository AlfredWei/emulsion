//! From mesh readings to a verdict on one face's eyes, with a "cannot tell" gate (RFC-0031 §8.14, §8.15). Pure: numbers in, a verdict out.
//!
//! The rules are the ones frozen in `docs/rfc/RFC-0031-appendix/blink_gate.py` before the second labelled batch was marked, and are
//! **not tuned further here**: if batch 2 moves them, they move in both places. Anything the gate does not let through is
//! `CannotTell`, never a guess: a wrong "closed" costs the user trust, a missing badge costs nothing.

use crate::blink_mesh::EyeMeshReading;

/// An eye aspect ratio below this is a closed eye (open eyes are about 0.26 on the user's marks, closed about 0.10-0.12).
pub const EAR_CLOSED_BELOW: f32 = 0.18;
/// The face must be at least this wide, in pixels of the decode the mesh ran on (the user's marks: closed-eye recall falls below it).
pub const MIN_FACE_PX: f32 = 96.0;
/// The mesh model must be at least this sure it is looking at a face.
pub const MIN_PRESENCE: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EyeState {
    Open,
    Closed,
}

/// What to say about a face. `OneClosed` is a wink, a squint or a half blink: worth a look, not a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceEyes {
    Open,
    Closed,
    OneClosed,
    CannotTell,
}

/// Per-eye states (picture left, picture right) when the gate lets the face through; None when it does not.
pub fn eye_states(reading: &EyeMeshReading, face_width_px: f32) -> Option<(EyeState, EyeState)> {
    let sane = reading.ear_left.is_finite() && reading.ear_right.is_finite() && reading.presence.is_finite();
    if !sane || !(face_width_px >= MIN_FACE_PX) || reading.presence < MIN_PRESENCE {
        return None;
    }
    let state = |ear: f32| if ear < EAR_CLOSED_BELOW { EyeState::Closed } else { EyeState::Open };
    Some((state(reading.ear_left), state(reading.ear_right)))
}

pub fn face_eyes(reading: &EyeMeshReading, face_width_px: f32) -> FaceEyes {
    match eye_states(reading, face_width_px) {
        None => FaceEyes::CannotTell,
        Some((EyeState::Open, EyeState::Open)) => FaceEyes::Open,
        Some((EyeState::Closed, EyeState::Closed)) => FaceEyes::Closed,
        Some(_) => FaceEyes::OneClosed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(ear_left: f32, ear_right: f32, presence: f32) -> EyeMeshReading {
        EyeMeshReading { presence, ear_left, ear_right }
    }

    #[test]
    fn open_closed_and_one_closed_faces_are_told_apart() {
        assert_eq!(face_eyes(&reading(0.27, 0.25, 0.99), 200.0), FaceEyes::Open);
        assert_eq!(face_eyes(&reading(0.10, 0.12, 0.99), 200.0), FaceEyes::Closed);
        assert_eq!(face_eyes(&reading(0.10, 0.26, 0.99), 200.0), FaceEyes::OneClosed);
    }

    #[test]
    fn a_small_face_or_an_unsure_mesh_is_cannot_tell_whatever_the_ratios_say() {
        assert_eq!(face_eyes(&reading(0.10, 0.10, 0.99), 95.9), FaceEyes::CannotTell);
        assert_eq!(face_eyes(&reading(0.10, 0.10, 0.49), 300.0), FaceEyes::CannotTell);
        assert_eq!(face_eyes(&reading(0.26, 0.26, 0.2), 300.0), FaceEyes::CannotTell);
    }

    #[test]
    fn the_cuts_are_inclusive_where_the_frozen_rules_are() {
        assert_eq!(face_eyes(&reading(0.18, 0.18, 0.5), 96.0), FaceEyes::Open, "ratio 0.18 is not below 0.18; width 96 and presence 0.5 pass");
        assert_eq!(face_eyes(&reading(0.179, 0.179, 0.5), 96.0), FaceEyes::Closed);
    }

    #[test]
    fn nonsense_numbers_are_cannot_tell() {
        assert_eq!(face_eyes(&reading(f32::NAN, 0.2, 0.9), 200.0), FaceEyes::CannotTell);
        assert_eq!(face_eyes(&reading(0.2, 0.2, f32::NAN), 200.0), FaceEyes::CannotTell);
        assert_eq!(face_eyes(&reading(0.2, 0.2, 0.9), f32::NAN), FaceEyes::CannotTell);
    }
}

//! Independent literal controls for the paste placement rules (F15, L02-C):
//! the 12 px nudge applies only over the same place, and a held destination
//! frame is reported as a typed directive for the transaction leaf (the
//! kernel never promotes a keyframe itself). Hand-written from `pasteSelection`.

use super::{
    paste_frame_directive, paste_offset, ClipSpot, FrameState as F, PasteFrameDirective as D,
    DUPLICATE_OFFSET_PX,
};

#[test]
fn paste_offset_is_twelve_only_over_the_same_place() {
    let here = ClipSpot { layer: 2, frame: 5 };
    assert_eq!(paste_offset(here, here), 12.0);
    assert_eq!(DUPLICATE_OFFSET_PX, 12.0);
    assert_eq!(paste_offset(here, ClipSpot { layer: 2, frame: 6 }), 0.0);
    assert_eq!(paste_offset(here, ClipSpot { layer: 3, frame: 5 }), 0.0);
}

#[test]
fn held_frame_destinations_are_reported_as_directives_for_the_transaction() {
    let frames = [F::Keyframe, F::Interpolated, F::Held, F::Missing, F::Held];
    // Animation 2D promotes a held frame; every other frame stores in place.
    assert_eq!(
        paste_frame_directive(&frames, 2, false),
        D::NeedsKeyframePromotion
    );
    assert_eq!(paste_frame_directive(&frames, 0, false), D::WriteCurrent);
    assert_eq!(paste_frame_directive(&frames, 1, false), D::WriteCurrent);
    assert_eq!(paste_frame_directive(&frames, 3, false), D::WriteCurrent);
    assert_eq!(paste_frame_directive(&frames, 9, false), D::WriteCurrent);
    // Motion writes into the keyframe the hold inherits from (skipping interpolated/missing).
    assert_eq!(
        paste_frame_directive(&frames, 2, true),
        D::WriteIntoHeldKeyframe(0)
    );
    assert_eq!(
        paste_frame_directive(&frames, 4, true),
        D::WriteIntoHeldKeyframe(0)
    );
    assert_eq!(paste_frame_directive(&frames, 1, true), D::WriteCurrent);
    // No keyframe at or before the hold: the baseline stores nothing.
    assert_eq!(
        paste_frame_directive(&[F::Held, F::Held], 1, true),
        D::HeldWithoutKeyframe
    );
}

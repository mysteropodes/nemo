//! T12 edit kernel: a video placement's stored timing is owned by
//! `SetTimeRemap` / `SetInterpMode`, kept across offset edits, and refused on
//! any other asset kind.
use super::tests::{catalog, document, track};
use super::*;
use crate::object_document::placement_offset::VideoTiming;

fn remap(json: &str) -> OrderTrack {
    serde_json::from_str(json).unwrap()
}
const TRACK: &str = r#"{"keys":[{"frame":0,"v":[0]},{"frame":10,"v":[19]}]}"#;

fn changed(inventory: &AssetInventory, edit: PlacementEdit) -> AssetInventory {
    match apply_edit(inventory, &document(), edit).unwrap() {
        PlacementOutcome::Changed(next) => next,
        PlacementOutcome::Unchanged => panic!("expected a change"),
    }
}
fn timing(inventory: &AssetInventory) -> VideoTiming {
    inventory.placements()[0].timing().clone()
}
fn set_remap(track: Option<&str>) -> PlacementEdit {
    PlacementEdit::SetTimeRemap {
        placement_id: "p1".into(),
        time_remap: track.map(remap),
    }
}
fn set_mode(mode: &str) -> PlacementEdit {
    PlacementEdit::SetInterpMode {
        placement_id: "p1".into(),
        interp_mode: mode.into(),
    }
}

#[test]
fn time_remap_is_set_replaced_and_cleared_and_the_same_track_is_a_no_op() {
    let enabled = changed(&catalog(), set_remap(Some(TRACK)));
    assert_eq!(timing(&enabled).remap_value(5.0), Some(9.5));
    assert_eq!(
        apply_edit(&enabled, &document(), set_remap(Some(TRACK))).unwrap(),
        PlacementOutcome::Unchanged
    );
    let edited = changed(
        &enabled,
        set_remap(Some(
            r#"{"keys":[{"frame":0,"v":[4],"hold":true},{"frame":10,"v":[19]}]}"#,
        )),
    );
    assert_eq!(timing(&edited).remap_value(5.0), Some(4.0));
    let cleared = changed(&edited, set_remap(None));
    assert!(timing(&cleared).is_empty());
    assert_eq!(cleared, catalog());
    assert_eq!(
        apply_edit(&catalog(), &document(), set_remap(None)).unwrap(),
        PlacementOutcome::Unchanged
    );
}

#[test]
fn the_mode_selector_writes_interp_mode_and_frame_blend_together() {
    for (mode, blend) in [("none", false), ("blend", true), ("motion", true)] {
        let next = timing(&changed(&catalog(), set_mode(mode)));
        assert_eq!(next.interp_mode.as_deref(), Some(mode));
        assert_eq!(next.frame_blend, Some(blend));
        assert!(next.time_remap.is_none(), "the remap track is untouched");
    }
    let once = changed(&catalog(), set_mode("blend"));
    assert_eq!(
        apply_edit(&once, &document(), set_mode("blend")).unwrap(),
        PlacementOutcome::Unchanged
    );
    for bad in ["", "a\0b", &"m".repeat(33)] {
        assert_eq!(
            apply_edit(&catalog(), &document(), set_mode(bad)),
            Err(PlacementEditError::Invalid),
            "{bad:?}"
        );
    }
}

#[test]
fn offset_edits_keep_the_stored_timing() {
    let timed = changed(
        &changed(&catalog(), set_remap(Some(TRACK))),
        set_mode("blend"),
    );
    let before = timing(&timed);
    for edit in [
        PlacementEdit::SetOffset {
            placement_id: "p1".into(),
            offset_frames: 9.into(),
        },
        PlacementEdit::ShiftOffset {
            placement_id: "p1".into(),
            delta_frames: (-3).into(),
        },
    ] {
        assert_eq!(timing(&changed(&timed, edit)), before);
    }
}

#[test]
fn timing_on_a_non_video_placement_or_an_unknown_one_is_refused() {
    let audio = AssetInventory::new(
        catalog().records().to_vec(),
        vec![AssetPlacement::new(
            "p1".into(),
            AssetId::new("a1".into()).unwrap(),
            track("t"),
            0,
        )
        .unwrap()],
        8,
        8,
    )
    .unwrap();
    for edit in [set_remap(Some(TRACK)), set_mode("blend"), set_remap(None)] {
        assert_eq!(
            apply_edit(&audio, &document(), edit),
            Err(PlacementEditError::KindMismatch)
        );
    }
    let unknown = PlacementEdit::SetInterpMode {
        placement_id: "nope".into(),
        interp_mode: "blend".into(),
    };
    assert_eq!(
        apply_edit(&catalog(), &document(), unknown),
        Err(PlacementEditError::UnknownPlacement)
    );
    let disordered = r#"{"keys":[{"frame":4,"v":[0]},{"frame":2,"v":[1]}]}"#;
    assert_eq!(
        apply_edit(&catalog(), &document(), set_remap(Some(disordered))),
        Err(PlacementEditError::Invalid)
    );
}

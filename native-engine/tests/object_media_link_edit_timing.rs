//! T12 edit kernel: a video placement's stored timing is owned by the bounded
//! remap intents and `SetInterpMode`, kept across offset edits, and refused on
//! any other asset kind.
use super::tests::{catalog, document, track};
use super::*;
use crate::object_document::placement_offset::{RemapIntent, VideoTiming};

fn changed(inventory: &AssetInventory, edit: PlacementEdit) -> AssetInventory {
    match apply_edit(inventory, &document(), edit).unwrap() {
        PlacementOutcome::Changed(next) => next,
        PlacementOutcome::Unchanged => panic!("expected a change"),
    }
}
fn timing(inventory: &AssetInventory) -> VideoTiming {
    inventory.placements()[0].timing().clone()
}
fn remap(intent: RemapIntent) -> PlacementEdit {
    PlacementEdit::TimeRemap {
        placement_id: "p1".into(),
        intent,
    }
}
fn enable(in_frame: i64, out_frame: i64, last: i64) -> PlacementEdit {
    remap(RemapIntent::Enable {
        in_frame: in_frame.into(),
        out_frame: out_frame.into(),
        last_source_frame: last.into(),
    })
}
fn disable() -> PlacementEdit {
    remap(RemapIntent::Disable)
}
fn remove_key(frame: i64) -> PlacementEdit {
    remap(RemapIntent::RemoveKey {
        frame: frame.into(),
    })
}
fn set_key(frame: f64, value: f64, curve: Option<serde_json::Value>) -> PlacementEdit {
    remap(RemapIntent::SetKey {
        frame: serde_json::Number::from_f64(frame).unwrap(),
        value: serde_json::Number::from_f64(value).unwrap(),
        curve: curve.map(|c| serde_json::from_value(c).unwrap()),
    })
}
fn set_mode(mode: &str) -> PlacementEdit {
    PlacementEdit::SetInterpMode {
        placement_id: "p1".into(),
        interp_mode: mode.into(),
    }
}

#[test]
fn enable_writes_the_baseline_two_linear_keys_and_refuses_a_second_enable() {
    let enabled = changed(&catalog(), enable(4, 23, 47));
    let track = timing(&enabled).time_remap.unwrap();
    let saved = serde_json::to_value(&track).unwrap();
    let linear = serde_json::json!([{"x":0,"y":0},{"x":1,"y":1}]);
    assert_eq!(
        saved,
        serde_json::json!({"keys":[
            {"frame":4,"v":[0],"curvePoints":linear,"hIn":[0,0],"hOut":[0,0]},
            {"frame":23,"v":[47],"curvePoints":linear,"hIn":[0,0],"hOut":[0,0]}]})
    );
    // Linear from source 0 at the in point to 47 at the out point.
    assert_eq!(timing(&enabled).remap_value(4.0 + 19.0 / 2.0), Some(23.5));
    assert_eq!(
        apply_edit(&enabled, &document(), enable(0, 10, 5)),
        Err(PlacementEditError::Conflict)
    );
    // `Math.max(inF + 1, outF)`: a one-frame layer still gets two keys.
    let short = timing(&changed(&catalog(), enable(7, 7, 0)));
    let frames: Vec<f64> = short
        .time_remap
        .unwrap()
        .keys
        .iter()
        .map(|k| k.frame.as_f64().unwrap())
        .collect();
    assert_eq!(frames, [7.0, 8.0]);
}

#[test]
fn disable_and_remove_key_follow_the_baseline_and_no_ops_publish_nothing() {
    let enabled = changed(&catalog(), enable(0, 10, 20));
    let one_key = changed(&enabled, remove_key(10));
    assert_eq!(timing(&one_key).time_remap.unwrap().keys.len(), 1);
    assert_eq!(
        apply_edit(&one_key, &document(), remove_key(5)).unwrap(),
        PlacementOutcome::Unchanged
    );
    // Removing every key keeps an empty track, as `t.keys.retain` does.
    let empty = changed(&one_key, remove_key(0));
    assert!(timing(&empty).has_time_remap());
    assert_eq!(timing(&empty).remap_value(3.0), None);
    let cleared = changed(&empty, disable());
    assert_eq!(cleared, catalog());
    assert_eq!(
        apply_edit(&catalog(), &document(), disable()).unwrap(),
        PlacementOutcome::Unchanged
    );
    assert_eq!(
        apply_edit(&catalog(), &document(), remove_key(0)),
        Err(PlacementEditError::Invalid),
        "no remap to edit"
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
    let timed = changed(&changed(&catalog(), enable(0, 10, 19)), set_mode("blend"));
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
    for edit in [
        enable(0, 10, 19),
        set_mode("blend"),
        disable(),
        remove_key(0),
        set_key(0.0, 1.0, None),
    ] {
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
    let nan = remap(RemapIntent::Enable {
        in_frame: 0.into(),
        out_frame: serde_json::Number::from_f64(1e300).unwrap(),
        last_source_frame: (-3).into(),
    });
    // Finite numbers are admitted as the caller computed them; a remap value
    // below zero is the baseline's `Math.max(0, ...)` floor.
    let floored = timing(&changed(&catalog(), nan)).time_remap.unwrap();
    assert_eq!(floored.keys[1].v[0].as_f64(), Some(0.0));
}

#[test]
fn set_key_uses_the_shared_property_key_kernel() {
    let on = changed(&catalog(), enable(0, 10, 19));
    // Without a remap there is no row to key: refused, nothing written.
    assert_eq!(
        apply_edit(&catalog(), &document(), set_key(5.0, 3.0, None)),
        Err(PlacementEditError::Invalid)
    );
    // A new key gets the kernel's default curve and zero handles, sorted in.
    let added = changed(&on, set_key(5.0, 3.0, None));
    let keys = &timing(&added).time_remap.unwrap().keys;
    assert_eq!(
        keys.iter()
            .map(|k| k.frame.as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![0.0, 5.0, 10.0]
    );
    let saved = serde_json::to_value(&keys[1]).unwrap();
    assert_eq!(saved["v"], serde_json::json!([3.0]));
    assert_eq!(saved["hIn"], serde_json::json!([0, 0]));
    assert_eq!(saved["hOut"], serde_json::json!([0, 0]));
    assert!(saved["curvePoints"].is_array());
    assert_eq!(timing(&added).remap_value(5.0), Some(3.0));
    // Re-keying an existing frame replaces its value and keeps its handles;
    // a curve, when given, replaces the stored one.
    let hold = serde_json::json!([{"x":0,"y":0},{"x":1,"y":0}]);
    let moved = changed(&added, set_key(10.0, 7.0, Some(hold.clone())));
    let last = serde_json::to_value(&timing(&moved).time_remap.unwrap().keys[2]).unwrap();
    assert_eq!(last["v"], serde_json::json!([7.0]));
    assert_eq!(last["curvePoints"], hold);
    assert_eq!(last["hIn"], serde_json::json!([0, 0]));
    // The same value again is a no-op with the stored spelling kept.
    assert_eq!(
        apply_edit(&added, &document(), set_key(5.0, 3.0, None)),
        Ok(PlacementOutcome::Unchanged)
    );
}

//! Hand-computed cases for the sparse Repeat Selection timing plan: every expected timing is written out as the
//! sparse keys and samples the dense editor's cycle must leave (baseline `repeatSelection`, src/js/timeline.js at
//! fbe4b8a3), not derived from the code under test. The oracle rows that run the whole transaction against the verbatim
//! baseline are in object_frame_clipboard_repeat_history.rs.
use super::*;
use serde_json::{json, Value};

fn timing(keys: &[u32], samples: &[(u32, Option<bool>)]) -> LayerTiming {
    serde_json::from_value(shape("drawing", keys, samples)).unwrap()
}
fn shape(family: &str, keys: &[u32], samples: &[(u32, Option<bool>)]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|frame| json!({"frame":frame})).collect();
    let samples: Vec<Value> = samples
        .iter()
        .map(|(frame, manual)| match manual {
            Some(flag) => json!({"frame":frame,"manualEdit":flag}),
            None => json!({"frame":frame}),
        })
        .collect();
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":family,
        "contextId":"scene-root","keys":keys,"samples":samples})
}
fn changed(plan: &RepeatPlan) -> Value {
    match &plan.edit {
        TimingEdit::Changed(timing) => serde_json::to_value(timing).unwrap(),
        TimingEdit::NoChange => panic!("expected a changed timing"),
    }
}

#[test]
fn keys_and_in_betweens_cycle_and_the_manual_flag_does_not_travel() {
    // Frames 0..=1 cycled twice on a 3-frame timeline: the cells land on 2,3 and 4,5 and the timeline grows to 6.
    let source = timing(&[0], &[(1, Some(true))]);
    let plan = source.repeat_range(0, 1, 2, 3, 6).unwrap();
    assert_eq!(plan.sources, vec![0, 1]);
    assert_eq!(
        changed(&plan),
        shape(
            "drawing",
            &[0, 2, 4],
            &[(1, Some(true)), (3, None), (5, None)]
        ),
        "the source keeps manualEdit true, the copies have no manualEdit member"
    );
}

#[test]
fn a_manual_edit_of_false_copies_as_absent_not_false() {
    let source = timing(&[0], &[(1, Some(false))]);
    let plan = source.repeat_range(0, 1, 1, 2, 4).unwrap();
    assert_eq!(
        changed(&plan),
        shape("drawing", &[0, 2], &[(1, Some(false)), (3, None)])
    );
}

#[test]
fn destinations_are_replaced_whole_and_later_entries_stay() {
    // Cycling 0..=1 once writes frames 2 and 3. The sample at 2 and the key at 3 vanish; frame 0 is a key (copied to 2),
    // frame 1 is a hold (frame 3 becomes a hold); the key at 5, past the window, stays.
    let source = timing(&[0, 3, 5], &[(2, None)]);
    let plan = source.repeat_range(0, 1, 1, 7, 7).unwrap();
    assert_eq!(plan.sources, vec![0]);
    assert_eq!(changed(&plan), shape("drawing", &[0, 2, 5], &[]));
}

#[test]
fn a_key_inside_the_window_goes_even_when_it_leaves_the_layer_unexposed_before_it() {
    // The only key sits at 2; cycling the two leading holds over it clears it: an explicit empty key list, not absence.
    let source = timing(&[2], &[]);
    let plan = source.repeat_range(0, 1, 1, 6, 6).unwrap();
    assert!(plan.sources.is_empty());
    assert_eq!(changed(&plan), shape("drawing", &[], &[]));
}

#[test]
fn a_cycle_that_writes_what_is_already_there_is_no_change() {
    let source = timing(&[0, 2], &[]);
    let plan = source.repeat_range(0, 1, 1, 4, 4).unwrap();
    assert_eq!(plan.edit, TimingEdit::NoChange);
    assert_eq!(plan.sources, vec![0]);
}

#[test]
fn a_range_of_holds_has_no_sources_and_changes_no_timing_however_far_it_grows() {
    let source = timing(&[0], &[]);
    let plan = source.repeat_range(1, 2, 2, 3, 7).unwrap();
    assert_eq!(plan.edit, TimingEdit::NoChange);
    assert!(plan.sources.is_empty());
}

#[test]
fn the_last_destination_frame_is_first_plus_span_times_and_sources_are_ordered() {
    // Range 1..=2 (span 2) three times: destinations 3..=8. Sources are the key at 1 and the sample at 2.
    let source = timing(&[0, 1], &[(2, None)]);
    let plan = source.repeat_range(1, 2, 3, 3, 9).unwrap();
    assert_eq!(plan.sources, vec![1, 2]);
    assert_eq!(
        changed(&plan),
        shape(
            "drawing",
            &[0, 1, 3, 5, 7],
            &[(2, None), (4, None), (6, None), (8, None)]
        )
    );
}

#[test]
fn malformed_ranges_and_timings_refuse_typed() {
    let source = timing(&[0], &[]);
    let invalid = |first, last, times, total, grown| {
        source
            .repeat_range(first, last, times, total, grown)
            .unwrap_err()
    };
    assert_eq!(
        invalid(2, 1, 1, 5, 9),
        LayerTimingError::Invalid,
        "empty range"
    );
    assert_eq!(
        invalid(0, 5, 1, 5, 20),
        LayerTimingError::Invalid,
        "past the end"
    );
    assert_eq!(
        invalid(0, 1, 0, 5, 5),
        LayerTimingError::Invalid,
        "no repetition"
    );
    assert_eq!(
        invalid(0, 1, 1, 5, 4),
        LayerTimingError::Invalid,
        "shrinking"
    );
    assert_eq!(
        invalid(0, 1, 2, 5, 5),
        LayerTimingError::Invalid,
        "last copy past the count"
    );
    assert!(
        source.repeat_range(0, 1, 2, 5, 6).is_ok(),
        "the last copy on the final frame fits"
    );
    let wide = timing(&[0, 9], &[]);
    assert_eq!(
        wide.repeat_range(0, 1, 1, 5, 5).unwrap_err(),
        LayerTimingError::Invalid
    );
    let other: LayerTiming = serde_json::from_value(shape("component", &[0], &[])).unwrap();
    assert_eq!(
        other.repeat_range(0, 0, 1, 3, 3).unwrap_err(),
        LayerTimingError::Unsupported
    );
}

#[test]
fn the_entry_cap_refuses_a_cycle_that_would_exceed_it_without_truncating() {
    let keys: Vec<u32> = (0..100).collect();
    let source = timing(&keys, &[]);
    // 100 + 39 * 100 = 4000 entries fit; 100 + 40 * 100 = 4100 do not.
    let fits = source.repeat_range(0, 99, 39, 100, 4000).unwrap();
    assert_eq!(
        changed(&fits)["keys"].as_array().unwrap().len(),
        4000,
        "every key of every round is kept"
    );
    assert_eq!(
        source.repeat_range(0, 99, 40, 100, 4100).unwrap_err(),
        LayerTimingError::Invalid
    );
}

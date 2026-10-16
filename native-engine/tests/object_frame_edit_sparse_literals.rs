//! Hand-computed expectations for the sparse frame-edit kernel: exact entries, exposure tuples,
//! entry/frame bounds, refusals and the serialized bytes of a published plan. These do not read
//! the oracle; they pin the acceptance cases one by one.
use super::super::{LayerTiming, LayerTimingError, TimingKey, TimingSample, MAX_ENTRIES};
use super::TimingEdit;
use crate::object_document::Presence;

fn literal(keys: &[u32], samples: &[(u32, Presence<bool>)]) -> LayerTiming {
    let mut timing = LayerTiming::blank_drawing();
    timing.keys = keys.iter().map(|&frame| TimingKey { frame }).collect();
    timing.samples = samples
        .iter()
        .map(|(frame, manual_edit)| TimingSample {
            frame: *frame,
            manual_edit: manual_edit.clone(),
        })
        .collect();
    timing
}
fn changed(edit: Result<TimingEdit, LayerTimingError>) -> LayerTiming {
    match edit {
        Ok(TimingEdit::Changed(timing)) => timing,
        other => panic!("expected a published plan, got {other:?}"),
    }
}
const UNEXPOSED: (&str, Option<u32>, bool) = ("unexposed", None, false);
#[allow(non_snake_case)]
fn HOLD(key: u32) -> (&'static str, Option<u32>, bool) {
    ("held", Some(key), false)
}

/// K . I M . K  (keys 0 and 5, samples 2 and 3 with manualEdit absent / true)
fn tween_layer() -> LayerTiming {
    literal(
        &[0, 5],
        &[(2, Presence::Missing), (3, Presence::Present(true))],
    )
}

#[test]
fn gap_shifts_only_entries_after_cf_and_leaves_the_straddled_cell_held() {
    let after = changed(tween_layer().insert_gap(2, 6));
    assert_eq!(
        after,
        literal(
            &[0, 6],
            &[(2, Presence::Missing), (4, Presence::Present(true))]
        )
    );
    assert_eq!(after.validate(7), Ok(()));
    // cf 2 and cf+1 are both tween samples; the new cell 3 is neither key nor sample.
    assert_eq!(after.exposure(2), ("interpolated", Some(2), false));
    assert_eq!(after.exposure(3), HOLD(0));
    assert_eq!(after.exposure(4), ("interpolated", Some(4), true));
    assert_eq!(after.exposure(5), HOLD(0));
    assert_eq!(after.exposure(6), ("key", Some(6), false));
}

#[test]
fn gap_at_the_key_frame_itself_moves_nothing_at_cf_and_appends_at_the_end() {
    let timing = literal(&[0, 3], &[]);
    let at_key = changed(timing.insert_gap(3, 4));
    assert_eq!(
        at_key, timing,
        "nothing lies after the last frame, so only the count grows"
    );
    assert_eq!(at_key.exposure(4), HOLD(3));
    let before_key = changed(timing.insert_gap(2, 4));
    assert_eq!(before_key, literal(&[0, 4], &[]));
    assert_eq!(before_key.exposure(3), HOLD(0));
}

#[test]
fn pad_keeps_every_authored_frame_number_and_holds_at_the_new_end() {
    let timing = tween_layer();
    let padded = changed(timing.insert_pad(6));
    assert_eq!(padded, timing);
    assert_eq!(padded.validate(7), Ok(()));
    assert_eq!(padded.exposure(6), HOLD(5));
    for frame in 0..6 {
        assert_eq!(
            padded.exposure(frame),
            timing.exposure(frame),
            "frame {frame} unchanged"
        );
    }
}

#[test]
fn remove_span_drops_inside_and_shifts_later_entries_down_without_collision() {
    let after = changed(tween_layer().remove_span(2, 3, 6));
    assert_eq!(after, literal(&[0, 3], &[]));
    assert_eq!(after.exposure(2), HOLD(0));
    assert_eq!(after.exposure(3), ("key", Some(3), false));
    // A sample moves onto a vacated frame: K I I K -> K I K.
    let dense = literal(
        &[0, 3],
        &[(1, Presence::Missing), (2, Presence::Present(false))],
    );
    assert_eq!(
        changed(dense.remove_span(1, 1, 4)),
        literal(&[0, 2], &[(1, Presence::Present(false))])
    );
}

#[test]
fn removing_the_only_key_at_zero_is_an_explicit_empty_plan() {
    let after = changed(literal(&[0], &[]).remove_span(0, 0, 3));
    assert_eq!(after, literal(&[], &[]));
    assert_eq!(after.validate(2), Ok(()));
    for frame in 0..2 {
        assert_eq!(after.exposure(frame), UNEXPOSED);
    }
    assert_eq!(
        serde_json::to_string(&after).unwrap(),
        r#"{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[],"samples":[]}"#
    );
    // A later key becomes the first one, and the hole before it is unexposed.
    let shifted = changed(literal(&[0, 5], &[(2, Presence::Missing)]).remove_span(0, 0, 6));
    assert_eq!(shifted, literal(&[4], &[(1, Presence::Missing)]));
    assert_eq!(shifted.exposure(0), UNEXPOSED);
    assert_eq!(shifted.exposure(1), ("interpolated", Some(1), false));
}

#[test]
fn remove_span_refusals_are_typed() {
    let timing = tween_layer();
    assert_eq!(
        timing.remove_span(0, 5, 6),
        Ok(TimingEdit::NoChange),
        "the last frame is never removed"
    );
    assert_eq!(
        literal(&[0], &[]).remove_span(0, 0, 1),
        Ok(TimingEdit::NoChange)
    );
    assert_eq!(timing.remove_span(3, 2, 6), Err(LayerTimingError::Invalid));
    assert_eq!(timing.remove_span(4, 6, 6), Err(LayerTimingError::Invalid));
    assert_eq!(
        timing.remove_span(0, 0, 5),
        Err(LayerTimingError::Invalid),
        "timing outside its frame count"
    );
}

#[test]
fn promote_adds_replaces_or_reports_no_change() {
    let held = changed(literal(&[0, 3], &[]).promote_cell(1, 4));
    assert_eq!(held, literal(&[0, 1, 3], &[]));
    assert_eq!(held.exposure(2), HOLD(1));
    let sample = changed(
        literal(
            &[0, 3],
            &[(1, Presence::Missing), (2, Presence::Present(true))],
        )
        .promote_cell(2, 4),
    );
    assert_eq!(
        sample,
        literal(&[0, 2, 3], &[(1, Presence::Missing)]),
        "the sample is replaced by the key"
    );
    assert_eq!(sample.exposure(2), ("key", Some(2), false));
    assert_eq!(
        literal(&[0, 3], &[]).promote_cell(3, 4),
        Ok(TimingEdit::NoChange)
    );
    let opening = changed(literal(&[2], &[]).promote_cell(0, 3));
    assert_eq!(opening.exposure(0), ("key", Some(0), false));
    assert_eq!(opening.exposure(1), HOLD(0));
}

#[test]
fn promote_cells_is_atomic_order_free_and_duplicate_safe() {
    let timing = literal(&[0], &[]);
    assert_eq!(
        changed(timing.promote_cells(&[3, 1, 3], 4)),
        literal(&[0, 1, 3], &[])
    );
    assert_eq!(
        timing.promote_cells(&[1, 9], 4),
        Err(LayerTimingError::Invalid)
    );
    assert_eq!(timing.promote_cells(&[0, 0], 4), Ok(TimingEdit::NoChange));
    assert_eq!(timing.promote_cells(&[], 4), Ok(TimingEdit::NoChange));
}

#[test]
fn clear_removes_the_entry_so_the_cell_holds_the_previous_key_or_is_unexposed() {
    let keys = literal(&[0, 1, 3], &[]);
    let cleared = changed(keys.clear_key(1, 4));
    assert_eq!(cleared, literal(&[0, 3], &[]));
    assert_eq!(cleared.exposure(1), HOLD(0));
    let first = changed(literal(&[0, 2], &[]).clear_key(0, 3));
    assert_eq!(first.exposure(0), UNEXPOSED);
    assert_eq!(first.exposure(2), ("key", Some(2), false));
    // Clear Keyframe touches keys only; Delete Frames clears samples too.
    let sampled = literal(
        &[0],
        &[(1, Presence::Present(true)), (2, Presence::Missing)],
    );
    assert_eq!(sampled.clear_key(1, 3), Ok(TimingEdit::NoChange));
    assert_eq!(sampled.clear_key(2, 3), Ok(TimingEdit::NoChange));
    assert_eq!(changed(sampled.clear_cells(&[1, 2], 3)), literal(&[0], &[]));
    assert_eq!(
        changed(sampled.clear_cells(&[0, 1], 3)),
        literal(&[], &[(2, Presence::Missing)])
    );
    assert_eq!(sampled.clear_cells(&[], 3), Ok(TimingEdit::NoChange));
    assert_eq!(
        literal(&[0], &[]).clear_cells(&[1, 2], 3),
        Ok(TimingEdit::NoChange)
    );
    assert_eq!(keys.clear_key(4, 4), Err(LayerTimingError::Invalid));
    assert_eq!(keys.clear_cells(&[0, 4], 4), Err(LayerTimingError::Invalid));
}

#[test]
fn demote_clears_strictly_between_and_keeps_both_endpoints() {
    let tween = literal(
        &[0, 4],
        &[
            (1, Presence::Missing),
            (2, Presence::Present(true)),
            (3, Presence::Missing),
        ],
    );
    let flat = changed(tween.demote_span(0, 4, 5));
    assert_eq!(flat, literal(&[0, 4], &[]));
    assert_eq!(flat.exposure(2), HOLD(0));
    // The dense editor clears a key sitting inside the span as well; the sparse kernel does the same.
    assert_eq!(
        changed(literal(&[0, 2, 4], &[(1, Presence::Missing)]).demote_span(0, 4, 5)),
        literal(&[0, 4], &[])
    );
    assert_eq!(
        tween.demote_span(0, 1, 5),
        Ok(TimingEdit::NoChange),
        "adjacent frames: nothing between"
    );
    assert_eq!(tween.demote_span(2, 2, 5), Ok(TimingEdit::NoChange));
    assert_eq!(tween.demote_span(3, 1, 5), Err(LayerTimingError::Invalid));
    assert_eq!(tween.demote_span(0, 5, 5), Err(LayerTimingError::Invalid));
}

fn at_cap(samples: usize) -> LayerTiming {
    let mut timing = LayerTiming::blank_drawing();
    timing.keys = (0..(MAX_ENTRIES - samples) as u32)
        .map(|frame| TimingKey { frame })
        .collect();
    timing.samples = (0..samples as u32)
        .map(|index| TimingSample {
            frame: 4500 + index,
            manual_edit: Presence::Missing,
        })
        .collect();
    timing
}

#[test]
fn entry_cap_and_frame_bounds_are_enforced_with_the_existing_errors() {
    let full = at_cap(0);
    assert_eq!(full.validate(5000), Ok(()));
    assert_eq!(
        full.promote_cell(4500, 5000),
        Err(LayerTimingError::Invalid),
        "a 4097th entry is refused"
    );
    assert_eq!(full.promote_cell(10, 5000), Ok(TimingEdit::NoChange));
    // Promoting a sample keeps the entry count, so it still fits at the cap.
    let sampled = at_cap(1);
    assert_eq!(sampled.keys.len() + sampled.samples.len(), MAX_ENTRIES);
    assert_eq!(changed(sampled.promote_cell(4500, 5000)).samples.len(), 0);
    assert_eq!(
        sampled.promote_cell(4600, 5000),
        Err(LayerTimingError::Invalid)
    );
    // Gap and pad only move frames, so they stay legal at the cap; the last key lands on frame 4096.
    assert_eq!(
        changed(full.insert_gap(2000, 5000))
            .keys
            .last()
            .map(|key| key.frame),
        Some(4096)
    );
    assert_eq!(changed(full.insert_pad(5000)), full);
    // Frame count bounds: u32 overflow, cf outside the timeline, timing wider than its frame count.
    let one = literal(&[0], &[]);
    assert_eq!(one.insert_gap(0, u32::MAX), Err(LayerTimingError::Invalid));
    assert_eq!(one.insert_pad(u32::MAX), Err(LayerTimingError::Invalid));
    assert_eq!(one.insert_gap(3, 3), Err(LayerTimingError::Invalid));
    assert_eq!(one.promote_cell(3, 3), Err(LayerTimingError::Invalid));
    assert_eq!(
        literal(&[0, 4], &[]).insert_gap(0, 4),
        Err(LayerTimingError::Invalid)
    );
}

#[test]
fn other_timing_families_are_unsupported_for_every_edit() {
    let mut timing = literal(&[0], &[]);
    timing.family = "component".into();
    let refusal = Err(LayerTimingError::Unsupported);
    assert_eq!(timing.insert_gap(0, 2), refusal);
    assert_eq!(timing.insert_pad(2), refusal);
    assert_eq!(timing.remove_span(0, 0, 2), refusal);
    assert_eq!(timing.promote_cell(1, 2), refusal);
    assert_eq!(timing.clear_key(0, 2), refusal);
    assert_eq!(timing.clear_cells(&[0], 2), refusal);
    assert_eq!(timing.demote_span(0, 1, 2), refusal);
}

#[test]
fn a_published_plan_serializes_and_reloads_with_authored_presence_intact() {
    let after = changed(tween_layer().insert_gap(2, 6));
    let json = serde_json::to_string(&after).unwrap();
    assert_eq!(
        json,
        r#"{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0},{"frame":6}],"samples":[{"frame":2},{"frame":4,"manualEdit":true}]}"#
    );
    assert_eq!(serde_json::from_str::<LayerTiming>(&json).unwrap(), after);
    let explicit_false = changed(literal(&[0], &[(1, Presence::Present(false))]).insert_gap(0, 3));
    assert!(serde_json::to_string(&explicit_false)
        .unwrap()
        .contains(r#"{"frame":2,"manualEdit":false}"#));
}

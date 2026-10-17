//! Hand-computed cases for the range join: the structure-edit to kernel-edit mapping, typed refusals,
//! what a layer without a range keeps, the operations that move no range, the typed unavailable work and
//! the composition-marker port. The oracle rows run in object_frame_edit_range_records_oracle.rs.
use super::object_frame_edit_range_records_oracle::rows::{range_doc, range_json, LayerRow};
use super::{
    join_document_ranges, join_structure_ranges, pending_for, range_edit_for, shift_marker_records,
    RangeJoinError, RangeUnavailable, StructureEdit,
};
use crate::object_codec::{encode_storage_project, ObjectCodecErrorKind};
use crate::object_document::{FrameEditKind, MarkerRecord, RangeEdit, RangeEditError};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn layer(uid: &str, range: Value) -> LayerRow {
    LayerRow {
        uid: uid.to_string(),
        locked: false,
        range,
    }
}

fn marker_records(value: Value) -> Vec<MarkerRecord> {
    serde_json::from_value(value).unwrap()
}

#[test]
fn each_structure_edit_maps_to_exactly_one_kernel_edit() {
    let none: &[&str] = &[];
    let cases = [
        (
            StructureEdit::Insert {
                cell: 2,
                targeted: none,
            },
            FrameEditKind::InsertFrame,
            2,
        ),
        (
            StructureEdit::Extend { cell: 1, count: 3 },
            FrameEditKind::ExtendExposure { count: 3 },
            1,
        ),
        (
            StructureEdit::Duplicate { cell: 4 },
            FrameEditKind::DuplicateKeyframe,
            4,
        ),
        (
            StructureEdit::Remove { from: 3, to: 3 },
            FrameEditKind::RemoveFrame,
            3,
        ),
        (
            StructureEdit::Remove { from: 1, to: 3 },
            FrameEditKind::RemoveFrameSpan { count: 3 },
            1,
        ),
    ];
    for (edit, kind, cursor) in cases {
        let want = RangeEdit {
            kind,
            cursor,
            total: 6,
        };
        assert_eq!(range_edit_for(&edit, 6), Ok(want), "{edit:?}");
    }
}

#[test]
fn kernel_refusals_come_back_typed() {
    let none: &[&str] = &[];
    let insert = |cell| StructureEdit::Insert {
        cell,
        targeted: none,
    };
    let edit_error = |error| Err(RangeJoinError::Edit(error));
    assert_eq!(
        range_edit_for(&StructureEdit::Remove { from: 3, to: 2 }, 5),
        edit_error(RangeEditError::SpanOutsideTimeline)
    );
    assert_eq!(
        range_edit_for(&StructureEdit::Remove { from: 2, to: 5 }, 5),
        edit_error(RangeEditError::SpanOutsideTimeline)
    );
    assert_eq!(
        range_edit_for(&StructureEdit::Remove { from: 0, to: 0 }, 1),
        edit_error(RangeEditError::WouldEmptyTimeline)
    );
    assert_eq!(
        range_edit_for(&StructureEdit::Remove { from: 0, to: 3 }, 4),
        edit_error(RangeEditError::WouldEmptyTimeline)
    );
    assert_eq!(
        range_edit_for(&insert(5), 5),
        edit_error(RangeEditError::CursorOutsideTimeline)
    );
    assert_eq!(
        range_edit_for(&StructureEdit::Extend { cell: 0, count: 0 }, 5),
        edit_error(RangeEditError::EmptyEdit)
    );
    assert_eq!(
        range_edit_for(&insert(0), u32::MAX),
        edit_error(RangeEditError::Overflow)
    );
    assert_eq!(
        range_edit_for(
            &StructureEdit::Remove {
                from: 0,
                to: u32::MAX
            },
            u32::MAX
        ),
        edit_error(RangeEditError::Overflow),
        "a span length past u32 is an overflow, not a wrapped count"
    );
}

#[test]
fn a_targeted_layer_shifts_a_padded_one_and_a_layer_without_a_range_do_not() {
    let full = json!({"inPoint":2,"outPoint":5,"timeLink":{"uid":"L1","mode":"in"},
        "markers":[{"frame":3,"name":"a","color":"#e8b64c"},{"frame":1}]});
    let layers = vec![
        layer("L0", full.clone()),
        layer("L1", Value::Null),
        layer("L2", json!({})),
        layer("L3", json!({"inPoint":2,"outPoint":null})),
    ];
    let mut after = range_doc(&layers, 6);
    after.total_frames = 7;
    let targeted = ["L0", "L1", "L2"];
    let edit = StructureEdit::Insert {
        cell: 1,
        targeted: &targeted,
    };
    let joined = join_structure_ranges(after, 6, &edit).unwrap();
    assert_eq!(
        range_json(&joined.document, 0),
        json!({"inPoint":3,"outPoint":6,"timeLink":{"uid":"L1","mode":"in"},
            "markers":[{"frame":4,"name":"a","color":"#e8b64c"},{"frame":1}]}),
        "values strictly after the cursor move, the marker at the cursor and the link stay"
    );
    assert_eq!(
        range_json(&joined.document, 1),
        Value::Null,
        "no range stays none"
    );
    assert_eq!(
        range_json(&joined.document, 2),
        json!({}),
        "an empty record stays present"
    );
    assert_eq!(
        range_json(&joined.document, 3),
        json!({"inPoint":2,"outPoint":null}),
        "a padded layer keeps every number and its explicit null"
    );
    assert_eq!(joined.layers_changed, 1);
    assert_eq!(
        joined.pending,
        &[
            RangeUnavailable::CompositionMarkers,
            RangeUnavailable::WorkAreaEnd
        ]
    );
}

#[test]
fn extend_duplicate_and_span_removal_move_no_layer_range() {
    let range = json!({"inPoint":2,"outPoint":5,"markers":[{"frame":4,"name":"x"}],
        "timeLink":{"uid":"L1"}});
    let layers = vec![layer("L0", range.clone())];
    let cases = [
        (StructureEdit::Extend { cell: 0, count: 2 }, 8),
        (StructureEdit::Duplicate { cell: 0 }, 7),
        (StructureEdit::Remove { from: 0, to: 1 }, 4),
    ];
    for (edit, total) in cases {
        let mut after = range_doc(&layers, 6);
        after.total_frames = total;
        let joined = join_structure_ranges(after, 6, &edit).unwrap();
        assert_eq!(range_json(&joined.document, 0), range, "{edit:?}");
        assert_eq!(joined.layers_changed, 0, "{edit:?}");
        assert_eq!(joined.pending, &[RangeUnavailable::WorkAreaEnd], "{edit:?}");
    }
}

#[test]
fn a_one_frame_removal_collapses_the_next_marker_onto_the_cursor() {
    let layers = vec![layer(
        "L0",
        json!({"markers":[{"frame":2,"name":"at"},{"frame":3,"name":"next"},{"frame":3}]}),
    )];
    let mut after = range_doc(&layers, 5);
    after.total_frames = 4;
    let edit = StructureEdit::Remove { from: 2, to: 2 };
    let joined = join_structure_ranges(after, 5, &edit).unwrap();
    assert_eq!(
        range_json(&joined.document, 0),
        json!({"markers":[{"frame":2,"name":"at"},{"frame":2,"name":"next"},{"frame":2}]})
    );
    assert_eq!(joined.pending.len(), 2);
}

#[test]
fn pending_work_names_what_has_no_native_owner() {
    let edit = |kind| RangeEdit {
        kind,
        cursor: 0,
        total: 4,
    };
    let both: &[RangeUnavailable] = &[
        RangeUnavailable::CompositionMarkers,
        RangeUnavailable::WorkAreaEnd,
    ];
    let work_area: &[RangeUnavailable] = &[RangeUnavailable::WorkAreaEnd];
    assert_eq!(pending_for(&edit(FrameEditKind::InsertFrame)), both);
    assert_eq!(pending_for(&edit(FrameEditKind::RemoveFrame)), both);
    for kind in [
        FrameEditKind::RemoveFrameSpan { count: 2 },
        FrameEditKind::ExtendExposure { count: 2 },
        FrameEditKind::DuplicateKeyframe,
    ] {
        assert_eq!(pending_for(&edit(kind)), work_area, "{kind:?}");
    }
}

#[test]
fn the_composition_marker_port_shifts_frames_and_keeps_the_authored_text() {
    let stored = marker_records(json!([
        {"frame":0,"name":"a"},{"frame":2,"name":"b","color":"#fff"},{"frame":3}
    ]));
    let insert = RangeEdit {
        kind: FrameEditKind::InsertFrame,
        cursor: 2,
        total: 5,
    };
    assert_eq!(
        serde_json::to_value(shift_marker_records(&insert, &stored).unwrap()).unwrap(),
        json!([{"frame":0,"name":"a"},{"frame":2,"name":"b","color":"#fff"},{"frame":4}])
    );
    let remove = RangeEdit {
        kind: FrameEditKind::RemoveFrame,
        cursor: 0,
        total: 5,
    };
    assert_eq!(
        serde_json::to_value(shift_marker_records(&remove, &stored).unwrap()).unwrap(),
        json!([{"frame":0,"name":"a"},{"frame":1,"name":"b","color":"#fff"},{"frame":2}])
    );
    let span = RangeEdit {
        kind: FrameEditKind::RemoveFrameSpan { count: 2 },
        cursor: 0,
        total: 5,
    };
    assert_eq!(
        shift_marker_records(&span, &stored).unwrap(),
        stored,
        "a span moves none"
    );
    let none: Vec<MarkerRecord> = Vec::new();
    assert_eq!(shift_marker_records(&insert, &none).unwrap(), none);
}

#[test]
fn a_shift_past_the_admission_bound_is_refused_by_admission_and_never_wrapped() {
    let layers = vec![layer(
        "L0",
        json!({"markers":[{"frame":9007199254740991i64}]}),
    )];
    let mut after = range_doc(&layers, 2);
    after.total_frames = 3;
    let targeted = ["L0"];
    let edit = StructureEdit::Insert {
        cell: 0,
        targeted: &targeted,
    };
    let joined = join_structure_ranges(after, 2, &edit).unwrap();
    assert_eq!(
        range_json(&joined.document, 0),
        json!({"markers":[{"frame":9007199254740992i64}]}),
        "the kernel adds in 64-bit integers"
    );
    let refused = encode_storage_project(&joined.document).unwrap_err();
    assert_eq!(refused.kind(), ObjectCodecErrorKind::Invalid);
    assert_eq!(refused.message(), "layer marker frame is out of bounds");
}

#[test]
fn the_transaction_exit_maps_every_refusal_to_a_dispatcher_code() {
    let layers = vec![layer(
        "L0",
        json!({"markers":[{"frame":9007199254740991i64}]}),
    )];
    let targeted = ["L0"];
    let insert = StructureEdit::Insert {
        cell: 0,
        targeted: &targeted,
    };
    // a shifted value outside the admitted bounds is refused by the admission re-run, not published
    let mut after = range_doc(&layers, 2);
    after.total_frames = 3;
    assert_eq!(
        join_document_ranges(after, 2, &insert),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // a kernel refusal is an invalid request
    let after = range_doc(&layers, 1);
    let remove = StructureEdit::Remove { from: 0, to: 0 };
    assert_eq!(
        join_document_ranges(after, 1, &remove),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // an edited document with the wrong frame count is an internal fault
    let after = range_doc(&layers, 2);
    assert_eq!(
        join_document_ranges(after, 2, &insert),
        Err(DispatchErrorCode::Internal)
    );
    // an ordinary shift is published whole
    let ordinary = vec![layer("L0", json!({"inPoint":4}))];
    let mut after = range_doc(&ordinary, 5);
    after.total_frames = 6;
    let published = join_document_ranges(after, 5, &insert).unwrap();
    assert_eq!(range_json(&published, 0), json!({"inPoint":5}));
}

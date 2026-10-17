//! L05-G record tests: the `ObjectLayer.range` member through the single native codec (v1 and v2), its
//! strict wire shape, its admission limits and the mapping to and from the range kernel. Absent, null and
//! explicit values stay distinct; nothing is repaired on load. The baseline oracle rows run through the
//! join (object_frame_edit_range_records_oracle.rs) and through the history owner
//! (object_frame_edit_range_records_history.rs).
use super::*;
use crate::object_codec::{
    decode_ordered_project, decode_storage_project, encode_project, encode_storage_project,
    ObjectCodecError, ObjectCodecErrorKind,
};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
use build::{document_json, Build, Cell};

fn cells() -> Vec<Cell> {
    (0..3)
        .map(|frame| Cell {
            flag: if frame == 0 { 'K' } else { '.' },
            strokes: Vec::new(),
        })
        .collect()
}

/// A two-layer, three-frame document whose layers carry `ranges` (`Null` = no member).
fn doc(build: Build, ranges: [Value; 2]) -> Value {
    let mut value = document_json(&[cells(), cells()], build);
    for (index, range) in ranges.into_iter().enumerate() {
        if !range.is_null() {
            value["layers"][index]["range"] = range;
        }
    }
    value
}

/// v1 documents read through the storage entry; a v2 document is read by the ordered decoder directly,
/// because the storage entry reports the v1 error for a v2 document that fails admission.
fn decode(value: &Value) -> Result<ObjectDocument, ObjectCodecError> {
    let text = value.to_string();
    if value["formatVersion"] == 2 {
        decode_ordered_project(text.as_bytes()).map(|ordered| ordered.into_storage())
    } else {
        decode_storage_project(text.as_bytes())
    }
}

fn refusal(value: &Value) -> (ObjectCodecErrorKind, String) {
    let error = decode(value).unwrap_err();
    (error.kind(), error.message().to_string())
}

fn range_of(value: Value) -> LayerRange {
    serde_json::from_value(value).unwrap()
}

fn wire(range: &LayerRange) -> Value {
    serde_json::to_value(range).unwrap()
}

#[test]
fn a_range_round_trips_through_v1_and_v2_with_absent_null_and_explicit_values_distinct() {
    let full = json!({"inPoint":0,"outPoint":null,
        "timeLink":{"uid":"L1","mode":"out","srcAnchor":"in"},
        "markers":[{"frame":1,"name":"","color":"#e8b64c"},{"frame":1}]});
    for build in [Build::V1, Build::V2] {
        let decoded = decode(&doc(build, [full.clone(), json!({})])).unwrap();
        let bytes = encode_storage_project(&decoded).unwrap();
        let again: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(again["layers"][0]["range"], full, "{build:?}");
        assert_eq!(again["layers"][1]["range"], json!({}), "{build:?}");
        assert_eq!(
            decode_storage_project(&bytes).unwrap(),
            decoded,
            "{build:?}"
        );
        if build == Build::V1 {
            assert_eq!(
                encode_project(&decoded).unwrap(),
                bytes,
                "the v1 encoder agrees"
            );
        }
        let range = decoded.layers[0].range.value().unwrap();
        assert_eq!(range.in_point, Presence::Present(Some(0)), "explicit zero");
        assert_eq!(range.out_point, Presence::Present(None), "explicit null");
        let markers = range.markers.value().unwrap();
        assert_eq!(markers[0].name, Presence::Present(String::new()));
        assert_eq!(
            markers[1].name,
            Presence::Missing,
            "a marker without text stays so"
        );
        assert_eq!(
            decoded.layers[1].range.value(),
            Some(&LayerRange::default())
        );
        let plain = decode(&doc(build, [Value::Null, Value::Null])).unwrap();
        let plain_wire: Value =
            serde_json::from_slice(&encode_storage_project(&plain).unwrap()).unwrap();
        assert!(
            plain_wire["layers"][0].get("range").is_none(),
            "absent stays absent"
        );
        assert_ne!(
            plain,
            decode(&doc(build, [json!({}), Value::Null])).unwrap()
        );
    }
}

#[test]
fn the_wire_shape_is_strict_and_object_only() {
    let base = doc(Build::V1, [Value::Null, Value::Null]).to_string();
    let with = |raw: &str| {
        let text = base.replacen(
            r#""layerUid":"L0""#,
            &format!(r#""layerUid":"L0","range":{raw}"#),
            1,
        );
        decode_storage_project(text.as_bytes()).unwrap_err().kind()
    };
    let invalid = ObjectCodecErrorKind::Invalid;
    let unsupported = ObjectCodecErrorKind::Unsupported;
    for (raw, kind) in [
        ("null", invalid),
        ("[]", invalid),
        (r#"{"extra":1}"#, unsupported),
        (r#"{"inPoint":1.5}"#, invalid),
        (r#"{"inPoint":"3"}"#, invalid),
        (r#"{"inPoint":1,"inPoint":2}"#, invalid),
        (r#"{"markers":null}"#, invalid),
        (r#"{"markers":[{"name":"x"}]}"#, invalid),
        (r#"{"markers":[{"frame":null}]}"#, invalid),
        (r#"{"markers":[[1]]}"#, invalid),
        (r#"{"markers":[{"frame":1,"extra":true}]}"#, unsupported),
        (r#"{"timeLink":{}}"#, invalid),
        (r#"{"timeLink":[]}"#, invalid),
        (r#"{"timeLink":{"uid":5}}"#, invalid),
        (r#"{"timeLink":{"uid":"L1","inOffset":3}}"#, unsupported),
    ] {
        assert_eq!(with(raw), kind, "{raw}");
    }
}

#[test]
fn admission_refusals_are_typed_with_exact_messages() {
    let marker = |frame: i64| json!({"markers":[{"frame":frame}]});
    let long = "x".repeat(1025);
    let many: Vec<Value> = (0..4097).map(|frame| json!({"frame":frame})).collect();
    let cases = [
        (
            json!({"inPoint":9007199254740992i64}),
            "layer range frame is out of bounds",
        ),
        (
            json!({"outPoint":-9007199254740992i64}),
            "layer range frame is out of bounds",
        ),
        (marker(-1), "layer marker frame is out of bounds"),
        (
            marker(9007199254740992),
            "layer marker frame is out of bounds",
        ),
        (
            json!({"markers":[{"frame":1,"name":long}]}),
            "layer marker text is too long",
        ),
        (
            json!({"markers":[{"frame":1,"color":long}]}),
            "layer marker text is too long",
        ),
        (json!({"markers":many}), "layer range has too many markers"),
        (json!({"timeLink":{"uid":""}}), "time link requires a uid"),
        (
            json!({"timeLink":{"uid":long}}),
            "time link uid is too long",
        ),
        (
            json!({"timeLink":{"uid":"L1","mode":"weekly"}}),
            "unknown time link mode",
        ),
        (
            json!({"timeLink":{"uid":"L1","srcAnchor":"mid"}}),
            "unknown time link srcAnchor",
        ),
    ];
    for build in [Build::V1, Build::V2] {
        for (range, message) in &cases {
            let value = doc(build, [Value::Null, range.clone()]);
            assert_eq!(
                refusal(&value),
                (ObjectCodecErrorKind::Invalid, message.to_string()),
                "{build:?} {range}"
            );
        }
    }
}

#[test]
fn limits_are_inclusive_and_baseline_quirks_are_admitted() {
    let name = "n".repeat(1024);
    let markers: Vec<Value> = (0..4096).map(|frame| json!({"frame":frame})).collect();
    let edge = json!({"inPoint":-9007199254740991i64,"outPoint":9007199254740991i64,
        "markers":[{"frame":0,"name":name,"color":name},{"frame":9007199254740991i64}],
        "timeLink":{"uid":name,"mode":"both"}});
    // in after out, out beyond the timeline, unsorted and repeated markers, a link to a missing layer
    // and a link to itself: the baseline keeps all of them and resolves them on read.
    let quirks = json!({"inPoint":5,"outPoint":2,"markers":[{"frame":2},{"frame":0},{"frame":2}],
        "timeLink":{"uid":"L0"}});
    let outside = json!({"inPoint":-3,"outPoint":50,"timeLink":{"uid":"gone","srcAnchor":"out"}});
    for build in [Build::V1, Build::V2] {
        let ranges = [edge.clone(), json!({"markers":markers})];
        for range in [&ranges[0], &ranges[1], &quirks, &outside] {
            let value = doc(build, [range.clone(), Value::Null]);
            let decoded = decode(&value).unwrap_or_else(|e| panic!("{build:?} {range}: {e}"));
            let bytes = encode_storage_project(&decoded).unwrap();
            let again: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(again["layers"][0]["range"], *range, "{build:?}");
        }
    }
}

#[test]
fn the_kernel_view_keeps_absent_null_and_explicit_values_apart() {
    let bare = range_of(json!({}));
    assert_eq!(
        bare.to_ranges(),
        LayerRanges {
            in_point: FrameBound::Absent,
            out_point: FrameBound::Absent,
            markers: None,
            time_link: TimeLinkSlot::Absent,
        }
    );
    let explicit = range_of(json!({"inPoint":0,"outPoint":null,"markers":[],"timeLink":null}));
    assert_eq!(
        explicit.to_ranges(),
        LayerRanges {
            in_point: FrameBound::At(0),
            out_point: FrameBound::Null,
            markers: Some(Vec::new()),
            time_link: TimeLinkSlot::Null,
        }
    );
    let linked = range_of(json!({"inPoint":3,"timeLink":{"uid":"L1"},
        "markers":[{"frame":4,"name":"n"}]}));
    assert_eq!(
        linked.to_ranges(),
        LayerRanges {
            in_point: FrameBound::At(3),
            out_point: FrameBound::Absent,
            markers: Some(vec![FrameBound::At(4)]),
            time_link: TimeLinkSlot::Linked,
        }
    );
    for record in [&bare, &explicit, &linked] {
        assert_eq!(record.with_ranges(&record.to_ranges()).as_ref(), Ok(record));
    }
}

#[test]
fn a_kernel_result_the_record_cannot_hold_is_a_typed_error() {
    let stored = range_of(json!({"markers":[{"frame":1}]}));
    let none = range_of(json!({}));
    let kernel = |markers: Option<Vec<FrameBound>>, time_link| LayerRanges {
        in_point: FrameBound::Absent,
        out_point: FrameBound::Absent,
        markers,
        time_link,
    };
    let absent = TimeLinkSlot::Absent;
    let shape = Err(RangeRecordError::MarkerShape);
    assert_eq!(stored.with_ranges(&kernel(None, absent)), shape);
    assert_eq!(none.with_ranges(&kernel(Some(Vec::new()), absent)), shape);
    let longer = Some(vec![FrameBound::At(1), FrameBound::At(2)]);
    assert_eq!(stored.with_ranges(&kernel(longer, absent)), shape);
    for bound in [FrameBound::Null, FrameBound::Absent] {
        assert_eq!(
            stored.with_ranges(&kernel(Some(vec![bound]), absent)),
            Err(RangeRecordError::MarkerFrame)
        );
    }
    let no_link = Err(RangeRecordError::TimeLinkShape);
    assert_eq!(
        none.with_ranges(&kernel(None, TimeLinkSlot::Linked)),
        no_link
    );
    let null_link = range_of(json!({"timeLink":null}));
    assert_eq!(
        null_link.with_ranges(&kernel(None, TimeLinkSlot::Linked)),
        no_link
    );
}

#[test]
fn a_kernel_shift_keeps_marker_text_the_link_object_and_the_absent_members() {
    let link = json!({"uid":"L9","mode":"both","srcAnchor":"out"});
    let stored = range_of(
        json!({"inPoint":2,"markers":[{"frame":3,"name":"n","color":"c"}],
        "timeLink":link}),
    );
    let edit = RangeEdit {
        kind: FrameEditKind::InsertFrame,
        cursor: 1,
        total: 6,
    };
    let moved = edit
        .layer_ranges(LayerRole::Targeted, &stored.to_ranges())
        .unwrap();
    assert_eq!(
        wire(&stored.with_ranges(&moved).unwrap()),
        json!({"inPoint":3,"markers":[{"frame":4,"name":"n","color":"c"}],"timeLink":link}),
        "outPoint was never there and is not invented"
    );
    let padded = edit
        .layer_ranges(LayerRole::Padded, &stored.to_ranges())
        .unwrap();
    assert_eq!(stored.with_ranges(&padded).as_ref(), Ok(&stored));
}

#[test]
fn a_cut_gives_the_original_the_new_out_and_the_second_half_the_rest() {
    let linked = range_of(json!({"inPoint":2,"outPoint":null,
        "markers":[{"frame":4,"name":"m"}],"timeLink":{"uid":"L1","mode":"in"}}));
    let Ok(RangeSplit::Cut {
        original,
        second,
        link_dropped,
    }) = split_layer_range(Some(&linked), 5, 2, 8)
    else {
        panic!("a cut inside the edges is a cut");
    };
    assert_eq!(
        wire(&original),
        json!({"inPoint":2,"outPoint":4,"markers":[{"frame":4,"name":"m"}]}),
        "the live link is deleted from the original"
    );
    assert_eq!(
        wire(&second),
        json!({"inPoint":5,"outPoint":8,"markers":[{"frame":4,"name":"m"}]}),
        "the second half never carries a link and copies the markers"
    );
    assert!(link_dropped);
    let null_link = range_of(json!({"timeLink":null}));
    let Ok(RangeSplit::Cut {
        original,
        second,
        link_dropped,
    }) = split_layer_range(Some(&null_link), 3, 0, 4)
    else {
        panic!("cut");
    };
    assert_eq!(wire(&original), json!({"outPoint":2,"timeLink":null}));
    assert_eq!(wire(&second), json!({"inPoint":3,"outPoint":4}));
    assert!(!link_dropped, "a null link is kept as null");
    let Ok(RangeSplit::Cut {
        original, second, ..
    }) = split_layer_range(None, 3, 0, 9)
    else {
        panic!("cut");
    };
    assert_eq!(
        wire(&original),
        json!({"outPoint":2}),
        "a layer with no range gains one"
    );
    assert_eq!(wire(&second), json!({"inPoint":3,"outPoint":9}));
}

#[test]
fn a_cut_outside_the_resolved_edges_is_refused() {
    for (cut, edge_in, edge_out, refused) in [
        (2, 2, 8, true),
        (1, 2, 8, true),
        (9, 2, 8, true),
        (8, 2, 8, false),
        (3, 2, 8, false),
    ] {
        let got = split_layer_range(None, cut, edge_in, edge_out).unwrap();
        assert_eq!(
            got == RangeSplit::OutsideLayer,
            refused,
            "cut {cut} in {edge_in}..{edge_out}"
        );
    }
}

#[test]
fn only_a_live_in_out_or_link_gates_the_frames_a_layer_shows() {
    let layer = |range: Value| {
        let mut value = doc(Build::V1, [Value::Null, Value::Null]);
        if !range.is_null() {
            value["layers"][0]["range"] = range;
        }
        decode(&value).unwrap().layers[0].clone()
    };
    for (range, gates) in [
        (Value::Null, false),
        (json!({}), false),
        (
            json!({"inPoint":null,"outPoint":null,"timeLink":null}),
            false,
        ),
        (json!({"markers":[{"frame":1}]}), false),
        (json!({"inPoint":0}), true),
        (json!({"outPoint":2}), true),
        (json!({"timeLink":{"uid":"L1"}}), true),
        (
            json!({"inPoint":null,"timeLink":{"uid":"L1","mode":"out"}}),
            true,
        ),
    ] {
        assert_eq!(layer(range.clone()).range_gates_frames(), gates, "{range}");
    }
}

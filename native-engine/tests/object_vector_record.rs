//! L01-D (G14) oracle: the `editable-vector-path` record round-trips through the real native codec and
//! carries exactly the values the baseline `desP`/`serP` pair (fbe4b8a3) produces for 14 literal strokes
//! (data, pins and the baseline cross-check live in object_vector_record_data.rs).
use crate::object_codec::{
    decode_project, decode_storage_project, encode_project, encode_storage_project,
    ObjectCodecError, ObjectCodecErrorKind, ObjectDocument,
};
use crate::object_document::{Presence, SolidFill};
use serde_json::{json, Number, Value};

#[path = "object_vector_record_controls.rs"]
mod controls;
#[path = "object_vector_record_data.rs"]
mod data;
use data::{
    check_pair, expand, paint_bytes, pair, pins, rows, BASELINE, DESP_SLICE, LEGACY_BYTES,
    SERP_SLICE, VECTOR_BYTES,
};
#[cfg(feature = "commands")]
#[path = "object_vector_record_fill.rs"]
mod fill;

pub(super) fn doc(records: Vec<Value>) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L1"}],"objects":records})
}
pub(super) fn decode(value: &Value) -> Result<ObjectDocument, ObjectCodecError> {
    decode_project(&serde_json::to_vec(value).unwrap())
}
fn seg(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> Value {
    json!({"point":{"x":point[0],"y":point[1]},"handleIn":{"x":handle_in[0],"y":handle_in[1]},
        "handleOut":{"x":handle_out[0],"y":handle_out[1]}})
}
/// An open two-anchor vector record: fill null, a real alpha-1 stroke, every required style member.
pub(super) fn record() -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[seg([0.0, 0.0], [0.0, 0.0], [10.5, 5.0]), seg([100.0, 50.0], [-10.5, -5.0], [0.0, 0.0])]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1.0},"hasRealStroke":true,
            "strokeWidth":4.0,"strokeCap":"round","strokeJoin":"round","opacity":1.0}})
}
pub(super) fn with(mut value: Value, pointer: &str, to: Value) -> Value {
    *value
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("no member {pointer}")) = to;
    value
}
/// Insert or replace one member (optional members and unknown-member controls).
pub(super) fn put(mut value: Value, pointer: &str, to: Value) -> Value {
    if let Some(slot) = value.pointer_mut(pointer) {
        *slot = to;
        return value;
    }
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    let parent = value.pointer_mut(parent).and_then(Value::as_object_mut);
    parent
        .unwrap_or_else(|| panic!("no parent object for {pointer}"))
        .insert(key.to_string(), to);
    value
}
pub(super) fn without(mut value: Value, pointer: &str) -> Value {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    let parent = value.pointer_mut(parent).unwrap().as_object_mut().unwrap();
    parent
        .remove(key)
        .unwrap_or_else(|| panic!("no member {pointer}"));
    value
}
pub(super) fn decoded(record: Value) -> ObjectDocument {
    decode(&doc(vec![record])).unwrap_or_else(|e| panic!("record refused: {e}"))
}
/// Re-encode through the real codec and return the first record as JSON.
fn through_codec(record: Value) -> Value {
    let bytes = encode_project(&decoded(record)).unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["objects"][0].clone()
}
fn rgba(paint: &SolidFill) -> [u8; 4] {
    let byte = |n: &Number| (n.as_f64().unwrap() * 255.0).round() as u8;
    [
        byte(&paint.r),
        byte(&paint.g),
        byte(&paint.b),
        byte(&paint.a),
    ]
}

#[test]
fn literals_are_complete_and_pinned_to_the_baseline() {
    let (meta, pairs) = rows();
    let get = |k: &str| {
        meta.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("baseline"), Some(BASELINE));
    assert_eq!(get("pairs"), Some("14"));
    assert_eq!(
        get("anchors"),
        Some("colorHex8:551,applyBrushKeyline:591,_r3:608,serP:609,desP:846")
    );
    assert_eq!(get("slice-sha256-serP"), Some(SERP_SLICE));
    assert_eq!(get("slice-sha256-desP"), Some(DESP_SLICE));
    let ids: Vec<&str> = pairs.iter().map(|p| p.id.as_str()).collect();
    let pins = pins();
    let pinned: Vec<&str> = pins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, pinned);
}

#[test]
fn baseline_pairs_decode_and_round_trip_without_loss() {
    let (_, pairs) = rows();
    for p in &pairs {
        let source = doc(vec![p.native.clone()]);
        let document = decode(&source).unwrap_or_else(|e| panic!("{}: {e}", p.id));
        let bytes = encode_project(&document).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            source,
            "{}",
            p.id
        );
        assert_eq!(decode_project(&bytes).unwrap(), document, "{}", p.id);
        let stored = decode_storage_project(&bytes).unwrap();
        assert_eq!(encode_storage_project(&stored).unwrap(), bytes, "{}", p.id);
    }
}

#[test]
fn native_records_carry_the_baseline_values_exactly() {
    let (_, pairs) = rows();
    for p in &pairs {
        check_pair(&p.baseline, &p.native).unwrap_or_else(|e| panic!("{}: {e}", p.id));
    }
}

#[test]
fn decoded_records_match_the_pinned_baseline_facts() {
    for pin in pins() {
        let id = pin.id.as_str();
        let document = decoded(pair(id).native);
        let record = &document.objects()[0];
        let body = record.vector().expect("vector body");
        assert_eq!(record.geometry.closed, pin.closed, "{id}");
        assert_eq!(record.geometry.segments.len(), pin.anchors, "{id}");
        assert_eq!(body.has_real_stroke(), pin.real, "{id}");
        assert_eq!(body.stroke_width.as_f64(), Some(pin.width), "{id}");
        assert_eq!(record.solid_fill().map(rgba), pin.fill, "{id}");
        assert_eq!(body.solid_stroke().map(rgba), pin.stroke, "{id}");
        assert_eq!(body.opacity.as_f64(), Some(pin.opacity), "{id}");
    }
}

#[test]
fn open_and_closed_are_independent_of_the_anchors() {
    let (open, closed) = (
        pair("open-curve-5").native,
        pair("closed-same-segments").native,
    );
    assert_eq!(open["geometry"]["segments"], closed["geometry"]["segments"]);
    assert_eq!(
        (&open["geometry"]["closed"], &closed["geometry"]["closed"]),
        (&json!(false), &json!(true))
    );
    for (rec, flag) in [(open, true), (closed, false)] {
        let flipped = with(rec.clone(), "/geometry/closed", json!(flag));
        assert_eq!(decoded(flipped).objects()[0].geometry.closed, flag);
        assert_eq!(
            through_codec(rec.clone())["geometry"]["segments"],
            rec["geometry"]["segments"]
        );
    }
}

#[test]
fn legacy_family_bytes_are_unchanged() {
    let bytes = expand(LEGACY_BYTES);
    let document = decode_project(bytes.as_bytes()).unwrap();
    assert_eq!(encode_project(&document).unwrap(), bytes.as_bytes());
    let record = &document.objects()[0];
    assert_eq!(record.solid_fill().map(rgba), Some([128, 64, 191, 255]));
    assert!(record.vector().is_none());
    assert_eq!(
        encode_storage_project(&decode_storage_project(bytes.as_bytes()).unwrap()).unwrap(),
        bytes.as_bytes()
    );
}

#[test]
fn vector_family_serialization_order_is_pinned() {
    let bytes = expand(VECTOR_BYTES);
    let document = decode_project(bytes.as_bytes()).unwrap();
    assert_eq!(encode_project(&document).unwrap(), bytes.as_bytes());
    let record = &document.objects()[0];
    assert_eq!(record.fill, Presence::Present(None));
    let body = record.vector().unwrap();
    assert_eq!(
        body.stroke_width.as_f64(),
        Some(0.0),
        "strokeWidth 0 is stored as 0"
    );
    assert_eq!(body.miter_limit.value().and_then(Number::as_f64), Some(0.0));
    assert_eq!(body.dash_offset.value().and_then(Number::as_f64), Some(0.0));
    assert_eq!(
        body.paint_order.value().map(String::as_str),
        Some("strokeFirst")
    );
    assert_eq!(body.dash_array.value().map(Vec::len), Some(2));
}

#[test]
fn fill_absent_null_and_explicit_stay_distinct() {
    let base = record();
    assert_eq!(
        decoded(base.clone()).objects()[0].fill,
        Presence::Present(None)
    );
    assert!(decoded(base.clone()).objects()[0].solid_fill().is_none());
    assert_eq!(through_codec(base.clone()).get("fill"), Some(&Value::Null));
    let half = json!({"kind":"solid","r":1.0,"g":0.0,"b":0.5,"a":0.5});
    let explicit = with(base.clone(), "/fill", half.clone());
    let document = decoded(explicit.clone());
    assert_eq!(
        document.objects()[0].solid_fill().unwrap().a.as_f64(),
        Some(0.5)
    );
    assert_eq!(through_codec(explicit)["fill"], half);
    let missing = without(base, "/fill");
    assert_eq!(
        decode(&doc(vec![missing])).unwrap_err().kind(),
        ObjectCodecErrorKind::Invalid
    );
}

#[test]
fn stroke_slot_and_flag_are_stored_not_inferred() {
    let real = through_codec(record());
    assert_eq!(real["vector"]["hasRealStroke"], json!(true));
    assert_eq!(real["vector"]["stroke"]["a"], json!(1.0));
    let none = with(
        with(record(), "/vector/stroke", Value::Null),
        "/vector/hasRealStroke",
        json!(false),
    );
    let document = decoded(none.clone());
    let body = document.objects()[0].vector().unwrap();
    assert!(body.solid_stroke().is_none() && !body.has_real_stroke());
    assert_eq!(body.stroke, Presence::Present(None));
    assert_eq!(
        through_codec(none)["vector"].get("stroke"),
        Some(&Value::Null)
    );
    // a white stroke is a real stroke when authored (the baseline fallback is white with the flag false)
    let white = pair("white-stroke-is-real");
    assert_eq!(
        paint_bytes(&white.native["vector"]["stroke"]),
        Some([255, 255, 255, 255])
    );
    assert_eq!(white.baseline["hasRealStroke"], json!(true));
    let fallback = pair("fill-only-fallback-white");
    assert_eq!(fallback.baseline["strokeColor"], json!("#ffffff"));
    assert!(fallback.native["vector"]["stroke"].is_null());
}

#[test]
fn style_members_keep_absent_and_explicit_values_apart() {
    let plain = through_codec(record());
    for key in ["miterLimit", "dashArray", "dashOffset", "paintOrder"] {
        assert!(
            plain["vector"].get(key).is_none(),
            "{key} absent stays absent"
        );
    }
    let mut styled = record();
    for (pointer, to) in [
        ("/vector/miterLimit", json!(0)),
        ("/vector/dashOffset", json!(0)),
        ("/vector/strokeWidth", json!(0)),
        ("/vector/dashArray", json!([4, 2])),
    ] {
        styled = put(styled, pointer, to);
    }
    let out = through_codec(styled);
    assert_eq!(out["vector"]["miterLimit"], json!(0));
    assert_eq!(out["vector"]["dashOffset"], json!(0));
    assert_eq!(
        out["vector"]["strokeWidth"],
        json!(0),
        "the `|| 3` importer rule is not applied in storage"
    );
    assert_eq!(out["vector"]["dashArray"], json!([4, 2]));
    let fill_first = through_codec(put(record(), "/vector/paintOrder", json!("fillFirst")));
    assert_eq!(fill_first["vector"]["paintOrder"], json!("fillFirst"));
    for opacity in [0.0, 1.0] {
        assert_eq!(
            through_codec(with(record(), "/vector/opacity", json!(opacity)))["vector"]["opacity"],
            json!(opacity)
        );
    }
    for (cap, join) in [("butt", "miter"), ("round", "round"), ("square", "bevel")] {
        let rec = with(
            with(record(), "/vector/strokeCap", json!(cap)),
            "/vector/strokeJoin",
            json!(join),
        );
        let out = through_codec(rec);
        assert_eq!(
            (
                out["vector"]["strokeCap"].clone(),
                out["vector"]["strokeJoin"].clone()
            ),
            (json!(cap), json!(join))
        );
    }
}

#[test]
fn anchor_count_is_bounded_for_open_and_closed_alike() {
    for closed in [false, true] {
        for count in [0usize, 1, 2, 256, 257] {
            let one = record()["geometry"]["segments"][0].clone();
            let rec = with(
                with(record(), "/geometry/closed", json!(closed)),
                "/geometry/segments",
                json!(vec![one; count]),
            );
            assert_eq!(
                decode(&doc(vec![rec])).is_ok(),
                (2..=256).contains(&count),
                "closed {closed} count {count}"
            );
        }
    }
}

#[test]
fn legacy_and_vector_records_share_one_document() {
    let legacy: Value = serde_json::from_str(&expand(LEGACY_BYTES)).unwrap();
    let mut mixed = legacy.clone();
    let mut second = record();
    second["target"]["strokeId"] = json!("v2");
    mixed["objects"].as_array_mut().unwrap().push(second);
    let document = decode(&mixed).unwrap();
    assert_eq!(document.objects().len(), 2);
    assert_eq!(document.objects()[0].target().stroke_id(), "v1");
    assert_eq!(document.objects()[1].target().stroke_id(), "v2");
    let out: Value = serde_json::from_slice(&encode_project(&document).unwrap()).unwrap();
    assert_eq!(
        out["objects"][0]["family"],
        json!("closed-solid-fill-cubic-path")
    );
    assert_eq!(out["objects"][1]["family"], json!("editable-vector-path"));
}

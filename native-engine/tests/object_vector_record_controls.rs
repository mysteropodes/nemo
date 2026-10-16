//! L01-D negative and mutation controls. Every control first proves that the unmutated record is
//! admitted, so a refusal can only come from the mutation; the pair controls prove `check_pair` fails
//! when a single baseline-derived value is altered (otherwise the cross-check would be vacuous).
use super::{check_pair, decode, doc, pair, put, record, with, without};
use crate::object_codec::{decode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

fn legacy() -> Value {
    let mut rec = with(record(), "/family", json!("closed-solid-fill-cubic-path"));
    rec = with(rec, "/geometry/closed", json!(true));
    rec = with(
        rec,
        "/fill",
        json!({"kind":"solid","r":0.5,"g":0.25,"b":0.75,"a":1.0}),
    );
    without(rec, "/vector")
}
fn kind_of(rec: Value) -> ObjectCodecErrorKind {
    decode(&doc(vec![rec]))
        .expect_err("mutated record must be refused")
        .kind()
}
/// The base is admitted, the mutation is refused with `kind`.
fn refused(base: Value, mutated: Value, kind: ObjectCodecErrorKind) {
    assert!(
        decode(&doc(vec![base])).is_ok(),
        "control base must be admitted"
    );
    assert_eq!(kind_of(mutated), kind);
}

#[test]
fn control_unknown_members_are_unsupported_not_dropped() {
    for pointer in [
        "/stroke",
        "/vector/strokeDash",
        "/geometry/winding",
        "/target/extra",
    ] {
        refused(
            record(),
            put(record(), pointer, json!({})),
            ObjectCodecErrorKind::Unsupported,
        );
    }
}

#[test]
fn control_duplicate_members_are_rejected_in_the_vector_body_and_record() {
    let source = serde_json::to_string(&doc(vec![record()])).unwrap();
    assert!(decode_project(source.as_bytes()).is_ok());
    for needle in [
        "\"hasRealStroke\":true",
        "\"strokeWidth\":4.0",
        "\"strokeCap\":\"round\"",
        "\"opacity\":1.0",
        "\"family\":\"editable-vector-path\"",
        "\"fill\":null",
        "\"closed\":false",
    ] {
        assert!(
            source.contains(needle),
            "control must alter the record: {needle}"
        );
        let twice = source.replacen(needle, &format!("{needle},{needle}"), 1);
        assert!(decode_project(twice.as_bytes()).is_err(), "{needle}");
    }
}

#[test]
fn control_positional_sequences_cannot_replace_the_vector_objects() {
    for pointer in ["/vector", "/vector/stroke", "/geometry", "/target"] {
        let object = record().pointer(pointer).unwrap().clone();
        let positional = Value::Array(object.as_object().unwrap().values().cloned().collect());
        assert!(
            decode(&doc(vec![with(record(), pointer, positional)])).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn control_family_typos_and_legacy_crossings_are_refused() {
    for family in [
        "editable-vector-paths",
        "Editable-Vector-Path",
        "vector",
        "",
    ] {
        refused(
            record(),
            with(record(), "/family", json!(family)),
            ObjectCodecErrorKind::Unsupported,
        );
    }
    assert!(
        decode(&doc(vec![legacy()])).is_ok(),
        "control base: the legacy record is admitted"
    );
    // an open path under the closed fill-only family
    refused(
        legacy(),
        with(legacy(), "/geometry/closed", json!(false)),
        ObjectCodecErrorKind::Unsupported,
    );
    // the vector body under the legacy family
    let crossed = put(legacy(), "/vector", record()["vector"].clone());
    refused(legacy(), crossed, ObjectCodecErrorKind::Unsupported);
    // legacy keeps its required solid fill: null and absent are both refused
    refused(
        legacy(),
        with(legacy(), "/fill", Value::Null),
        ObjectCodecErrorKind::Invalid,
    );
    refused(
        legacy(),
        without(legacy(), "/fill"),
        ObjectCodecErrorKind::Invalid,
    );
    // the vector family without its body (a legacy-shaped record under the new tag)
    let shaped = with(legacy(), "/family", json!("editable-vector-path"));
    refused(record(), shaped, ObjectCodecErrorKind::Invalid);
}

#[test]
fn control_absent_required_slots_are_refused() {
    for pointer in [
        "/fill",
        "/vector",
        "/vector/stroke",
        "/vector/hasRealStroke",
        "/vector/strokeWidth",
        "/vector/strokeCap",
        "/vector/strokeJoin",
        "/vector/opacity",
    ] {
        refused(
            record(),
            without(record(), pointer),
            ObjectCodecErrorKind::Invalid,
        );
    }
}

#[test]
fn control_stroke_flag_never_disagrees_with_the_stroke_slot() {
    // hasRealStroke true with a null stroke
    refused(
        record(),
        with(record(), "/vector/stroke", Value::Null),
        ObjectCodecErrorKind::Invalid,
    );
    // a stroke colour with hasRealStroke false is a stored fallback, not an authored stroke
    refused(
        record(),
        with(record(), "/vector/hasRealStroke", json!(false)),
        ObjectCodecErrorKind::Invalid,
    );
}

#[test]
fn control_out_of_range_style_and_paint_values_are_typed_errors() {
    use ObjectCodecErrorKind::{Invalid, Unsupported};
    let filled = with(
        record(),
        "/fill",
        json!({"kind":"solid","r":1.0,"g":0.0,"b":0.0,"a":0.5}),
    );
    assert!(decode(&doc(vec![filled.clone()])).is_ok(), "control base");
    for (pointer, bad, kind) in [
        ("/fill/a", json!(1.5), Invalid),
        ("/fill/r", json!(-0.1), Invalid),
        ("/fill/kind", json!("gradient"), Unsupported),
        ("/vector/stroke/g", json!(2.0), Invalid),
        ("/vector/stroke/kind", json!("pattern"), Unsupported),
        ("/vector/strokeWidth", json!(-1.0), Invalid),
        ("/vector/strokeWidth", json!("4"), Invalid),
        ("/vector/strokeCap", json!("flat"), Invalid),
        ("/vector/strokeJoin", json!("arc"), Invalid),
        ("/vector/miterLimit", json!(-1.0), Invalid),
        ("/vector/dashArray", json!([]), Invalid),
        ("/vector/dashArray", json!([0, 0]), Invalid),
        ("/vector/dashArray", json!([-1, 2]), Invalid),
        ("/vector/dashArray", json!("4 2"), Invalid),
        ("/vector/dashOffset", json!("1"), Invalid),
        ("/vector/paintOrder", json!("behind"), Invalid),
        ("/vector/paintOrder", json!(""), Invalid),
        ("/vector/opacity", json!(1.5), Invalid),
        ("/vector/opacity", json!(-0.1), Invalid),
    ] {
        let mutated = put(filled.clone(), pointer, bad.clone());
        assert_eq!(kind_of(mutated), kind, "{pointer} = {bad}");
    }
}

#[test]
fn control_non_finite_numbers_and_geometry_violations_are_refused() {
    let source = serde_json::to_string(&doc(vec![record()])).unwrap();
    for (needle, replacement) in [
        ("\"strokeWidth\":4.0", "\"strokeWidth\":1e999"),
        ("\"x\":10.5", "\"x\":-1e999"),
    ] {
        assert!(source.contains(needle), "{needle}");
        assert!(decode_project(source.replacen(needle, replacement, 1).as_bytes()).is_err());
    }
    use ObjectCodecErrorKind::{Invalid, Unsupported};
    for (pointer, bad, kind) in [
        ("/geometry/kind", json!("polyline"), Unsupported),
        ("/geometry/coordinateSpace", json!("screen"), Unsupported),
        ("/geometry/handleSpace", json!("absolute"), Unsupported),
        ("/geometry/segments/0/point/x", json!("0"), Invalid),
        ("/geometry/segments", json!([]), Invalid),
        ("/target/frameScope/frame", json!(3), Invalid),
        ("/target/layerUid", json!("missing"), Invalid),
    ] {
        refused(record(), with(record(), pointer, bad.clone()), kind);
    }
    let twice = doc(vec![record(), record()]);
    assert_eq!(
        decode(&twice).unwrap_err().kind(),
        ObjectCodecErrorKind::DuplicateId
    );
}

#[test]
fn control_pair_check_fails_for_each_altered_baseline_value() {
    let real = pair("closed-rect");
    check_pair(&real.baseline, &real.native).expect("control base: the real pair passes");
    let dashed = pair("dashed");
    check_pair(&dashed.baseline, &dashed.native).expect("control base: the dashed pair passes");
    let mutations: [(&str, &str, Value); 12] = [
        ("closed-rect", "/geometry/closed", json!(false)),
        ("closed-rect", "/geometry/segments/1/point/x", json!(101)),
        ("closed-rect", "/geometry/segments/2/handleIn/y", json!(0.5)),
        ("closed-rect", "/fill/a", json!(1)),
        ("closed-rect", "/fill", Value::Null),
        ("closed-rect", "/vector/stroke/r", json!(0.5)),
        ("closed-rect", "/vector/hasRealStroke", json!(false)),
        ("closed-rect", "/vector/strokeWidth", json!(3)),
        ("closed-rect", "/vector/strokeCap", json!("butt")),
        ("closed-rect", "/vector/miterLimit", json!(11)),
        ("dashed", "/vector/dashArray", json!([2, 4])),
        ("authored-stroke-id", "/target/strokeId", json!("other")),
    ];
    for (id, pointer, to) in mutations {
        let p = pair(id);
        check_pair(&p.baseline, &p.native).expect("control base");
        assert!(
            check_pair(&p.baseline, &with(p.native.clone(), pointer, to.clone())).is_err(),
            "{id} {pointer} = {to}"
        );
    }
    // dropping or adding optional members is also a difference
    assert!(check_pair(
        &real.baseline,
        &without(real.native.clone(), "/vector/miterLimit")
    )
    .is_err());
    assert!(check_pair(
        &real.baseline,
        &without(real.native.clone(), "/vector/dashOffset")
    )
    .is_err());
    let paint = put(
        real.native.clone(),
        "/vector/paintOrder",
        json!("fillFirst"),
    );
    assert!(check_pair(&real.baseline, &paint).is_err());
    assert!(check_pair(
        &real.baseline,
        &with(real.native.clone(), "/geometry/segments", json!([]))
    )
    .is_err());
}

#[test]
fn control_fallback_colour_is_never_read_as_a_real_stroke() {
    let fallback = pair("fill-only-fallback-white");
    check_pair(&fallback.baseline, &fallback.native).expect("control base");
    assert_eq!(fallback.baseline["strokeColor"], json!("#ffffff"));
    // the baseline column still carries the white fallback; a native record that stores it as a stroke fails
    let inferred = with(
        with(
            fallback.native.clone(),
            "/vector/stroke",
            json!({"kind":"solid","r":1,"g":1,"b":1,"a":1}),
        ),
        "/vector/hasRealStroke",
        json!(true),
    );
    assert!(check_pair(&fallback.baseline, &inferred).is_err());
    let flag_only = with(
        fallback.native.clone(),
        "/vector/hasRealStroke",
        json!(true),
    );
    assert!(check_pair(&fallback.baseline, &flag_only).is_err());
    // and the codec refuses a stored fallback (stroke present, flag false)
    let stored = with(
        fallback.native,
        "/vector/stroke",
        json!({"kind":"solid","r":1,"g":1,"b":1,"a":1}),
    );
    assert_eq!(kind_of(stored), ObjectCodecErrorKind::Invalid);
}

//! L01-E (G14/G22/G13) oracle: pressure fields, the `paramShape` record and the per-layer `pathFx` stack
//! round-trip through the real native codec and carry exactly the values the baseline (fbe4b8a3) produced
//! (data, pins and the baseline cross-check live in object_vector_extensions_data.rs; refusals in
//! object_vector_extensions_controls.rs; the join to the L01-A kernel in object_vector_extensions_kernel.rs).
use crate::object_codec::{
    decode_project, decode_storage_project, encode_project, encode_storage_project,
    ObjectCodecError, ObjectCodecErrorKind, ObjectDocument,
};
use crate::object_document::ParamShape;
use serde_json::{json, Number, Value};

#[path = "object_vector_brush_texture.rs"]
mod brush_texture;
#[path = "object_vector_extensions_controls.rs"]
mod controls;
#[path = "object_vector_extensions_data.rs"]
mod data;
#[cfg(feature = "evaluation")]
#[path = "object_vector_extensions_kernel.rs"]
mod kernel;
#[path = "object_vector_extensions_layers.rs"]
mod layers;
#[path = "object_vector_extensions_pair_controls.rs"]
mod pair_controls;
#[path = "object_vector_extensions_pins.rs"]
mod pins;
use data::{check_pair, pair, rows, BASELINE, DESP_SLICE, FX_SLICE, SERP_SLICE};
use pins::{FX_PINS, PRESSURE_PINS, SHAPE_PINS};

pub(super) fn doc(layers: Value, records: Vec<Value>) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":layers,"objects":records})
}
pub(super) fn record_doc(records: Vec<Value>) -> Value {
    doc(json!([{"layerUid":"L1"}]), records)
}
pub(super) fn decode(value: &Value) -> Result<ObjectDocument, ObjectCodecError> {
    decode_project(&serde_json::to_vec(value).unwrap())
}
pub(super) fn decoded(record: Value) -> ObjectDocument {
    decode(&record_doc(vec![record])).unwrap_or_else(|e| panic!("record refused: {e}"))
}
fn seg(point: [f64; 2]) -> Value {
    json!({"point":{"x":point[0],"y":point[1]},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}})
}
/// A closed four-anchor vector record: solid fill, no stroke, no extension.
pub(super) fn brush() -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"b1"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[seg([0.0, 0.0]), seg([100.0, 0.0]), seg([100.0, 50.0]), seg([0.0, 50.0])]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1.0},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":6.0,"strokeCap":"round",
            "strokeJoin":"round","opacity":1.0}})
}
/// Insert or replace one member by JSON pointer (the parent object must exist).
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
pub(super) fn kind_of(source: &Value) -> ObjectCodecErrorKind {
    decode(source)
        .expect_err("mutated document must be refused")
        .kind()
}

#[test]
fn oracle_is_complete_and_pinned_to_the_baseline() {
    let (meta, pairs, fxs) = rows();
    let get = |k: &str| {
        meta.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("baseline"), Some(BASELINE));
    assert_eq!(get("pairs"), Some("18"));
    assert_eq!(get("fx"), Some("9"));
    assert_eq!(
        get("anchors"),
        Some("colorHex8:551,applyBrushKeyline:591,_r3:608,serP:609,desP:846,syncTranslate:6883,syncScale:6887")
    );
    assert_eq!(get("slice-sha256-serP"), Some(SERP_SLICE));
    assert_eq!(get("slice-sha256-desP"), Some(DESP_SLICE));
    assert_eq!(get("slice-sha256-pathFx"), Some(FX_SLICE));
    let pair_ids: Vec<&str> = pairs.iter().map(|p| p.id.as_str()).collect();
    let pinned: Vec<&str> = PRESSURE_PINS
        .iter()
        .map(|p| p.id)
        .chain(SHAPE_PINS.iter().map(|p| p.id))
        .collect();
    assert_eq!(pair_ids, pinned);
    let fx_ids: Vec<&str> = fxs.iter().map(|f| f.id.as_str()).collect();
    let mut fx_pinned: Vec<&str> = FX_PINS.iter().map(|p| p.0).collect();
    fx_pinned.push("unknown-type-ignored-by-baseline");
    assert_eq!(fx_ids, fx_pinned);
}

#[test]
fn baseline_pairs_round_trip_through_the_native_codec() {
    let (_, pairs, _) = rows();
    for p in &pairs {
        let source = record_doc(vec![p.native.clone()]);
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
    let (_, pairs, _) = rows();
    for p in &pairs {
        check_pair(&p.baseline, &p.native).unwrap_or_else(|e| panic!("{}: {e}", p.id));
    }
}

#[test]
fn pressure_records_match_the_pinned_baseline_facts() {
    for pin in &PRESSURE_PINS {
        let document = decoded(pair(pin.id).native);
        let record = &document.objects()[0];
        let body = record.vector.value().expect("vector body");
        assert!(
            !body.has_real_stroke() && body.solid_stroke().is_none(),
            "{}",
            pin.id
        );
        let pressure = record
            .vector_ext
            .value()
            .and_then(|ext| ext.pressure.value())
            .unwrap_or_else(|| panic!("{}: pressure", pin.id));
        let widths: Option<Vec<f64>> = pressure
            .center_segments
            .value()
            .map(|a| a.iter().map(|s| s.width.as_f64().unwrap()).collect());
        assert_eq!(widths.as_deref(), pin.widths, "{}", pin.id);
        let profile: Option<Vec<(f64, f64)>> = pressure.width_profile.value().map(|a| {
            a.iter()
                .map(|s| (s.t.as_f64().unwrap(), s.width.as_f64().unwrap()))
                .collect()
        });
        assert_eq!(profile.as_deref(), pin.profile, "{}", pin.id);
        assert_eq!(
            pressure.stroke_profile.value().map(String::as_str),
            pin.kind,
            "{}",
            pin.id
        );
        assert_eq!(
            pressure.profile_base.value().and_then(Number::as_f64),
            pin.base,
            "{}",
            pin.id
        );
    }
}

#[test]
fn pressure_opacity_is_kept_beside_the_pressure_fields() {
    let document = decoded(pair("pressure-taper-in-opacity").native);
    let body = document.objects()[0].vector.value().unwrap();
    assert_eq!(body.opacity.as_f64(), Some(0.5));
    assert_eq!(body.stroke_width.as_f64(), Some(6.0));
}

fn present(shape: &ParamShape) -> Vec<(&'static str, f64)> {
    let all = [
        ("tl", &shape.tl),
        ("tr", &shape.tr),
        ("br", &shape.br),
        ("bl", &shape.bl),
        ("startAngle", &shape.start_angle),
        ("sweep", &shape.sweep),
        ("innerRadius", &shape.inner_radius),
        ("pointCount", &shape.point_count),
        ("innerRatio", &shape.inner_ratio),
        ("cornerRadius", &shape.corner_radius),
    ];
    all.into_iter()
        .filter_map(|(name, member)| member.value().and_then(Number::as_f64).map(|v| (name, v)))
        .collect()
}

#[test]
fn param_shape_records_match_the_pinned_baseline_facts() {
    for pin in &SHAPE_PINS {
        let document = decoded(pair(pin.id).native);
        let ext = document.objects()[0].vector_ext.value().expect("vectorExt");
        assert!(ext.pressure.missing(), "{}", pin.id);
        let shape = ext.param_shape.value().expect("paramShape");
        assert_eq!(shape.kind, pin.kind, "{}", pin.id);
        assert_eq!(present(shape).as_slice(), pin.members, "{}", pin.id);
        let frame = shape
            .frame
            .value()
            .map(|f| [&f.x1, &f.y1, &f.x2, &f.y2].map(|n| n.as_f64().unwrap()));
        assert_eq!(frame, pin.frame, "{}", pin.id);
        let angle = ext.box_angle.value().and_then(Number::as_f64);
        assert_eq!(angle, pin.angle, "{}", pin.id);
    }
}

#[test]
fn zero_and_absent_radii_stay_distinct() {
    let shape = |m: Value| put(brush(), "/vectorExt", json!({"paramShape": m}));
    let zero = decoded(shape(json!({"kind":"rect","tl":0,"tr":0,"br":0,"bl":0})));
    let none = decoded(shape(json!({"kind":"rect"})));
    let get = |d: &ObjectDocument| {
        d.objects()[0]
            .vector_ext
            .value()
            .unwrap()
            .param_shape
            .value()
            .unwrap()
            .tl
            .clone()
    };
    assert_eq!(get(&zero).value().and_then(Number::as_f64), Some(0.0));
    assert!(get(&none).missing());
    assert_ne!(zero, none);
    for d in [&zero, &none] {
        let bytes = encode_project(d).unwrap();
        assert_eq!(&decode_project(&bytes).unwrap(), d);
    }
}

#[test]
fn profile_base_zero_is_an_explicit_value_not_an_absent_one() {
    let with_base = |base: Option<f64>| {
        let mut pressure = json!({"strokeProfile":"even"});
        if let Some(b) = base {
            pressure["profileBase"] = json!(b);
        }
        decoded(put(brush(), "/vectorExt", json!({ "pressure": pressure })))
    };
    let (zero, absent) = (with_base(Some(0.0)), with_base(None));
    assert_ne!(zero, absent);
    let bytes = encode_project(&zero).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(r#""profileBase":0.0"#), "{text}");
    assert_eq!(decode_project(&bytes).unwrap(), zero);
    assert!(!String::from_utf8(encode_project(&absent).unwrap())
        .unwrap()
        .contains("profileBase"));
}

#[test]
fn pressure_fields_round_trip_literally_with_non_uniform_widths() {
    let ext = json!({"pressure":{
        "centerSegments":[
            {"point":{"x":0.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":5.5,"y":-2.0},"width":0.5},
            {"point":{"x":40.0,"y":20.0},"handleIn":{"x":-5.5,"y":2.0},"handleOut":{"x":6.0,"y":0.0},"width":4.0},
            {"point":{"x":90.0,"y":10.0},"handleIn":{"x":-6.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0},"width":1.25}],
        "widthProfile":[{"t":0.0,"width":0.5},{"t":0.5,"width":4.0},{"t":1.0,"width":1.25}],
        "strokeProfile":"taper-out","profileBase":4.0}});
    let source = record_doc(vec![put(brush(), "/vectorExt", ext)]);
    let document = decode(&source).unwrap();
    let bytes = encode_project(&document).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), source);
    let pressure = document.objects()[0]
        .vector_ext
        .value()
        .unwrap()
        .pressure
        .value()
        .unwrap();
    let anchors = pressure.center_segments.value().unwrap();
    let widths: Vec<f64> = anchors.iter().map(|a| a.width.as_f64().unwrap()).collect();
    assert_eq!(widths, [0.5, 4.0, 1.25]);
    assert_eq!(anchors[0].handle_out.x.as_f64(), Some(5.5));
    assert_eq!(anchors[2].point.x.as_f64(), Some(90.0));
}

#[test]
fn there_is_no_scalar_pressure_field() {
    for (pointer, value) in [
        ("/vectorExt", json!({"pressure": 0.5})),
        ("/vector/pressure", json!(0.5)),
    ] {
        let source = record_doc(vec![put(brush(), pointer, value)]);
        let kind = kind_of(&source);
        assert!(
            matches!(
                kind,
                ObjectCodecErrorKind::Invalid | ObjectCodecErrorKind::Unsupported
            ),
            "{pointer}"
        );
    }
    let base = put(brush(), "/vectorExt", json!({}));
    for member in [
        "pressure",
        "pressureScale",
        "pressureValue",
        "pressureCurve",
    ] {
        let source = record_doc(vec![put(
            base.clone(),
            &format!("/vectorExt/{member}"),
            json!({}),
        )]);
        let outcome = decode(&source);
        if member == "pressure" {
            assert!(
                outcome.is_ok(),
                "the container member is the only pressure name"
            );
        } else {
            assert_eq!(
                outcome.expect_err(member).kind(),
                ObjectCodecErrorKind::Unsupported
            );
        }
    }
}

//! L01-E negative and mutation controls. Every control first proves that the unmutated base is admitted, so a
//! refusal can only come from the mutation; the pair controls prove `check_pair` fails when one
//! baseline-derived value is altered (otherwise the cross-check would be vacuous).
use super::{brush, decode, doc, kind_of, put, record_doc, without};
use crate::object_codec::{decode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};
use ObjectCodecErrorKind::{Invalid, Unsupported};

fn pressure_base() -> Value {
    put(
        brush(),
        "/vectorExt",
        json!({"pressure":{
        "centerSegments":[
            {"point":{"x":0.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":5.0,"y":5.0},"width":1.5},
            {"point":{"x":40.0,"y":20.0},"handleIn":{"x":-5.0,"y":-5.0},"handleOut":{"x":0.0,"y":0.0},"width":4.0}],
        "widthProfile":[{"t":0.0,"width":1.5},{"t":1.0,"width":4.0}],
        "strokeProfile":"even","profileBase":3.0}}),
    )
}
fn shape_base() -> Value {
    put(
        brush(),
        "/vectorExt",
        json!({"paramShape":{"kind":"ellipse","startAngle":10.0,"sweep":270.0,
        "innerRadius":5.0,"box":{"x1":0.0,"y1":0.0,"x2":100.0,"y2":50.0}},"boxAngle":15.0}),
    )
}
pub(super) fn pd() -> Value {
    record_doc(vec![pressure_base()])
}
pub(super) fn sd() -> Value {
    record_doc(vec![shape_base()])
}
pub(super) fn fd() -> Value {
    doc(
        json!([{"layerUid":"L1","pathFx":[
            {"type":"zigzag","enabled":true,"p1":10.0,"p2":2.0,"p3":0.0},{"type":"roughen","p1":3.0}]}]),
        vec![brush()],
    )
}
/// The base is admitted, the mutation is refused with `kind`.
fn refused(base: &Value, mutated: Value, kind: ObjectCodecErrorKind) {
    assert!(decode(base).is_ok(), "control base must be admitted");
    assert_eq!(kind_of(&mutated), kind);
}
fn text(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn control_bases_are_admitted() {
    for base in [pd(), sd(), fd()] {
        assert!(decode(&base).is_ok());
    }
}

#[test]
fn control_unknown_members_are_unsupported_not_dropped() {
    for (base, pointer) in [
        (pd(), "/objects/0/vectorExt/extra"),
        (pd(), "/objects/0/vectorExt/pressure/scalar"),
        (
            pd(),
            "/objects/0/vectorExt/pressure/centerSegments/0/pressure",
        ),
        (pd(), "/objects/0/vectorExt/pressure/widthProfile/0/extra"),
        (sd(), "/objects/0/vectorExt/paramShape/extra"),
        (sd(), "/objects/0/vectorExt/paramShape/box/z"),
        (fd(), "/layers/0/pathFx/0/p4"),
        (fd(), "/layers/0/pathFx/1/id"),
        (fd(), "/layers/0/extra"),
        (fd(), "/pathFx"),
    ] {
        refused(&base, put(base.clone(), pointer, json!(1)), Unsupported);
    }
}

#[test]
fn control_duplicate_members_are_rejected() {
    for (base, needle) in [
        (pd(), "\"strokeProfile\":\"even\""),
        (pd(), "\"profileBase\":3.0"),
        (sd(), "\"boxAngle\":15.0"),
        (sd(), "\"kind\":\"ellipse\""),
        (fd(), "\"type\":\"zigzag\""),
        (fd(), "\"enabled\":true"),
    ] {
        let source = text(&base);
        assert!(decode_project(source.as_bytes()).is_ok());
        assert!(source.contains(needle), "{needle}");
        let twice = source.replacen(needle, &format!("{needle},{needle}"), 1);
        assert!(decode_project(twice.as_bytes()).is_err(), "{needle}");
    }
}

#[test]
fn control_positional_sequences_cannot_replace_the_objects() {
    for (base, pointer, bad) in [
        (pd(), "/objects/0/vectorExt", json!([])),
        (pd(), "/objects/0/vectorExt/pressure", json!([])),
        (
            pd(),
            "/objects/0/vectorExt/pressure/centerSegments/0",
            json!([0, 0, 0, 0]),
        ),
        (
            pd(),
            "/objects/0/vectorExt/pressure/widthProfile/0",
            json!([0, 1]),
        ),
        (sd(), "/objects/0/vectorExt/paramShape", json!(["ellipse"])),
        (
            sd(),
            "/objects/0/vectorExt/paramShape/box",
            json!([0, 0, 1, 1]),
        ),
        (fd(), "/layers/0/pathFx/0", json!(["zigzag"])),
    ] {
        refused(&base, put(base.clone(), pointer, bad), Invalid);
    }
}

#[test]
fn control_pressure_shapes_and_values_are_typed_errors() {
    let center = "/objects/0/vectorExt/pressure/centerSegments";
    let profile = "/objects/0/vectorExt/pressure/widthProfile";
    let stops = |a: f64, b: f64| json!([{"t":a,"width":1.0},{"t":b,"width":2.0}]);
    let base = pd();
    for (pointer, bad, kind) in [
        (profile.to_string(), json!("wide"), Invalid),
        (profile.to_string(), json!({"t":0.0,"width":1.0}), Invalid),
        (profile.to_string(), json!(3), Invalid),
        (center.to_string(), json!("none"), Invalid),
        (format!("{profile}/0/t"), json!(-0.25), Invalid),
        (format!("{profile}/1/t"), json!(1.5), Invalid),
        (profile.to_string(), stops(0.6, 0.4), Invalid),
        (format!("{profile}/0/width"), json!("thin"), Invalid),
        (format!("{center}/0/width"), json!("thick"), Invalid),
        (
            "/objects/0/vectorExt/pressure/strokeProfile".into(),
            json!("wobble"),
            Unsupported,
        ),
        (
            "/objects/0/vectorExt/pressure/strokeProfile".into(),
            json!(""),
            Unsupported,
        ),
        (
            "/objects/0/vectorExt/pressure/strokeProfile".into(),
            json!(7),
            Invalid,
        ),
        (
            "/objects/0/vectorExt/pressure/profileBase".into(),
            json!(-1.0),
            Invalid,
        ),
        (
            "/objects/0/vectorExt/pressure/profileBase".into(),
            json!("6"),
            Invalid,
        ),
        (
            "/objects/0/vectorExt/pressure/profileBase".into(),
            Value::Null,
            Invalid,
        ),
        ("/objects/0/vectorExt/boxAngle".into(), json!("15"), Invalid),
    ] {
        refused(&base, put(base.clone(), &pointer, bad), kind);
    }
    for pointer in [format!("{center}/0/width"), format!("{center}/0/handleIn")] {
        let missing = without(base.clone(), &pointer);
        assert_eq!(kind_of(&missing), Invalid, "{pointer} is required");
    }
    // equal t (a stationary pen) and an empty centreline are baseline-legal and admitted
    assert!(decode(&put(base.clone(), profile, stops(0.5, 0.5))).is_ok());
    assert!(decode(&put(base.clone(), center, json!([]))).is_ok());
}

#[test]
fn control_pressure_never_carries_a_real_stroke() {
    let paint = json!({"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1.0});
    let base = pd();
    refused(
        &base,
        put(base.clone(), "/objects/0/vector/stroke", paint.clone()),
        Invalid,
    );
    let both = put(
        put(base.clone(), "/objects/0/vector/stroke", paint),
        "/objects/0/vector/hasRealStroke",
        json!(true),
    );
    refused(&base, both, Invalid);
    refused(
        &base,
        put(base.clone(), "/objects/0/vector/hasRealStroke", json!(true)),
        Invalid,
    );
    // without the pressure object the same stroke is an ordinary vector record
    let plain = put(
        brush(),
        "/vector/stroke",
        json!({"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1.0}),
    );
    let plain = put(plain, "/vector/hasRealStroke", json!(true));
    assert!(decode(&record_doc(vec![plain])).is_ok());
}

#[test]
fn control_extension_needs_the_vector_family_and_content() {
    let legacy = |ext: Option<Value>| {
        let mut rec = put(brush(), "/family", json!("closed-solid-fill-cubic-path"));
        rec = without(rec, "/vector");
        if let Some(ext) = ext {
            rec = put(rec, "/vectorExt", ext);
        }
        record_doc(vec![rec])
    };
    let base = legacy(None);
    refused(&base, legacy(Some(json!({"boxAngle": 15.0}))), Unsupported);
    refused(
        &base,
        legacy(Some(json!({"paramShape": {"kind":"rect"}}))),
        Unsupported,
    );
    let vector = record_doc(vec![brush()]);
    refused(
        &vector,
        record_doc(vec![put(brush(), "/vectorExt", json!({}))]),
        Invalid,
    );
    // a record that names a layer the document does not declare is refused with its extension
    let stray = put(pressure_base(), "/target/layerUid", json!("L9"));
    refused(&pd(), record_doc(vec![stray]), Invalid);
}

#[test]
fn control_param_shape_refusals() {
    let base = sd();
    let shape = "/objects/0/vectorExt/paramShape";
    for (member, bad, kind) in [
        ("kind", json!("triangle"), Unsupported),
        ("kind", json!("Rect"), Unsupported),
        ("kind", json!(3), Invalid),
        ("tl", json!(4.0), Invalid),
        ("pointCount", json!(5), Invalid),
        ("cornerRadius", json!(1.0), Invalid),
        ("sweep", json!("270"), Invalid),
        ("sweep", Value::Null, Invalid),
        ("innerRadius", json!([5.0]), Invalid),
        ("box", Value::Null, Invalid),
        ("box", json!({"x1":0.0,"y1":0.0,"x2":1.0}), Invalid),
        ("box", json!({"x1":"0","y1":0.0,"x2":1.0,"y2":1.0}), Invalid),
    ] {
        refused(
            &base,
            put(base.clone(), &format!("{shape}/{member}"), bad),
            kind,
        );
    }
    refused(
        &base,
        without(base.clone(), &format!("{shape}/kind")),
        Invalid,
    );
    // zero is a value, not an absence: an explicit 0 sweep and an absent sweep are both admitted
    assert!(decode(&put(base.clone(), &format!("{shape}/sweep"), json!(0.0))).is_ok());
    assert!(decode(&without(base.clone(), &format!("{shape}/sweep"))).is_ok());
}

#[test]
fn control_path_fx_refusals() {
    let base = fd();
    let first = "/layers/0/pathFx/0";
    let stack = "/layers/0/pathFx";
    for (pointer, bad, kind) in [
        (format!("{first}/type"), json!("wiggle"), Unsupported),
        (format!("{first}/type"), json!("Zigzag"), Unsupported),
        (
            "/layers/0/pathFx/1/type".to_string(),
            json!(""),
            Unsupported,
        ),
        (format!("{first}/type"), json!(1), Invalid),
        (format!("{first}/enabled"), Value::Null, Invalid),
        (format!("{first}/enabled"), json!("yes"), Invalid),
        (format!("{first}/p1"), json!("big"), Invalid),
        (format!("{first}/p2"), json!([1]), Invalid),
        (stack.to_string(), json!({"type":"zigzag"}), Invalid),
        (stack.to_string(), json!("zigzag"), Invalid),
        (stack.to_string(), json!([null]), Invalid),
        (stack.to_string(), json!([{"enabled":true}]), Invalid),
        (
            stack.to_string(),
            json!([{"type":"zigzag"}, {"type":"wiggle"}]),
            Unsupported,
        ),
    ] {
        refused(&base, put(base.clone(), &pointer, bad), kind);
    }
    // an explicit null parameter, an absent one and a zero are all admitted and stay distinct
    for value in [Value::Null, json!(0.0)] {
        assert!(decode(&put(base.clone(), &format!("{first}/p2"), value)).is_ok());
    }
    assert!(decode(&without(base.clone(), &format!("{first}/p2"))).is_ok());
}

#[test]
fn control_out_of_range_numbers_are_refused_not_stored() {
    for (base, needle) in [
        (pd(), "\"profileBase\":3.0"),
        (pd(), "\"width\":1.5"),
        (pd(), "\"t\":1.0"),
        (sd(), "\"sweep\":270.0"),
        (sd(), "\"x1\":0.0"),
        (sd(), "\"boxAngle\":15.0"),
        (fd(), "\"p2\":2.0"),
        (fd(), "\"p1\":3.0"),
    ] {
        let source = text(&base);
        assert!(source.contains(needle), "{needle}");
        let key = needle.split(':').next().unwrap();
        for big in ["1e999", "-1e999"] {
            let mutated = source.replacen(needle, &format!("{key}:{big}"), 1);
            assert!(
                decode_project(mutated.as_bytes()).is_err(),
                "{needle} {big}"
            );
        }
    }
}

//! L01-E controls, part 2: encode-side validation, the `check_pair` mutation controls and the stack-order /
//! unknown-type controls. Each pair control first proves the unmutated pair passes the cross-check.
use super::controls::{fd, pd, sd};
use super::{check_pair, decode, doc, kind_of, pair, put, rows, without};
use crate::object_codec::encode_project;
use crate::object_codec::ObjectCodecErrorKind::{Invalid, Unsupported};
use crate::object_document::{PathFxRecord, Presence};
use serde_json::{json, Number, Value};

#[test]
fn control_encode_refuses_what_decode_refuses() {
    let mut document = decode(&fd()).unwrap();
    let bad = PathFxRecord {
        fx_type: "wiggle".into(),
        enabled: Presence::Missing,
        p1: Presence::Missing,
        p2: Presence::Missing,
        p3: Presence::Missing,
    };
    document.layers[0].path_fx = Presence::Present(vec![bad]);
    assert_eq!(encode_project(&document).unwrap_err().kind(), Unsupported);

    let mut document = decode(&sd()).unwrap();
    let mut ext = document.objects[0].vector_ext.value().unwrap().clone();
    let mut shape = ext.param_shape.value().unwrap().clone();
    shape.kind = "triangle".into();
    ext.param_shape = Presence::Present(shape);
    document.objects[0].vector_ext = Presence::Present(ext);
    assert_eq!(encode_project(&document).unwrap_err().kind(), Unsupported);

    let mut document = decode(&pd()).unwrap();
    let mut ext = document.objects[0].vector_ext.value().unwrap().clone();
    let mut pressure = ext.pressure.value().unwrap().clone();
    pressure.profile_base = Presence::Present(Number::from_f64(-2.0).unwrap());
    ext.pressure = Presence::Present(pressure);
    document.objects[0].vector_ext = Presence::Present(ext);
    assert_eq!(encode_project(&document).unwrap_err().kind(), Invalid);
}

fn e(pointer: impl Into<String>, to: Option<Value>) -> (String, Option<Value>) {
    (pointer.into(), to)
}
fn fails(id: &str, edits: Vec<(String, Option<Value>)>) {
    let p = pair(id);
    check_pair(&p.baseline, &p.native)
        .unwrap_or_else(|err| panic!("{id}: unmutated pair fails: {err}"));
    for (pointer, to) in &edits {
        let mutated = match to {
            Some(value) => put(p.native.clone(), pointer, value.clone()),
            None => without(p.native.clone(), pointer),
        };
        assert!(check_pair(&p.baseline, &mutated).is_err(), "{id} {pointer}");
    }
}

#[test]
fn control_pair_check_fails_for_each_altered_baseline_value() {
    let pressure = "/vectorExt/pressure";
    fails(
        "pressure-nonuniform-bulge",
        vec![
            e(
                format!("{pressure}/centerSegments/1/width"),
                Some(json!(6.5)),
            ),
            e(
                format!("{pressure}/centerSegments/0/handleOut/x"),
                Some(json!(13)),
            ),
            e(
                format!("{pressure}/centerSegments/3/point/y"),
                Some(json!(31)),
            ),
            e(format!("{pressure}/widthProfile/2/t"), Some(json!(0.55))),
            e(format!("{pressure}/widthProfile/4/width"), Some(json!(0.8))),
            e(format!("{pressure}/strokeProfile"), Some(json!("even"))),
            e(format!("{pressure}/strokeProfile"), None),
            e(format!("{pressure}/profileBase"), Some(json!(0))),
            e(format!("{pressure}/profileBase"), None),
            e(format!("{pressure}/centerSegments"), None),
            e("/vectorExt", None),
            e("/vectorExt/boxAngle", Some(json!(15))),
            e("/vector/hasRealStroke", Some(json!(true))),
        ],
    );
    // the baseline drops a falsy profileBase / boxAngle: a native 0 would not be the baseline value
    fails(
        "pressure-profile-base-zero-dropped",
        vec![e(format!("{pressure}/profileBase"), Some(json!(0)))],
    );
    fails(
        "box-angle-zero-dropped",
        vec![e("/vectorExt/boxAngle", Some(json!(0)))],
    );
    fails(
        "pressure-flag-only",
        vec![e(format!("{pressure}/centerSegments"), Some(json!([])))],
    );
    let shape = "/vectorExt/paramShape";
    fails(
        "rect-four-radii-box-angle",
        vec![
            e(format!("{shape}/tl"), Some(json!(5))),
            e(format!("{shape}/br"), None),
            e(format!("{shape}/sweep"), Some(json!(90))),
            e(format!("{shape}/kind"), Some(json!("star"))),
            e(format!("{shape}/box/x2"), Some(json!(101))),
            e(format!("{shape}/box"), None),
            e("/vectorExt/boxAngle", Some(json!(0))),
            e("/vectorExt/boxAngle", None),
        ],
    );
    fails(
        "rect-zero-radius-vs-absent",
        vec![e(format!("{shape}/br"), Some(json!(0)))],
    );
    fails(
        "ellipse-sweep-absent",
        vec![e(format!("{shape}/sweep"), Some(json!(359.9)))],
    );
    fails(
        "star-box-translated",
        vec![e(format!("{shape}/box/x1"), Some(json!(0)))],
    );
}

#[test]
fn control_stack_order_is_storage_order_and_unknown_types_are_not_ignored() {
    let (_, _, fxs) = rows();
    let forward = fxs
        .iter()
        .find(|f| f.id == "order-zigzag-then-roughen")
        .unwrap();
    let mut reversed = forward.stack.as_array().unwrap().clone();
    reversed.reverse();
    let layer = |stack: Vec<Value>| {
        decode(&doc(json!([{"layerUid":"L1","pathFx":stack}]), vec![])).unwrap()
    };
    let first = |d: &crate::object_codec::ObjectDocument| {
        d.layers()[0].path_fx().unwrap()[0].fx_type.clone()
    };
    assert_eq!(
        first(&layer(forward.stack.as_array().unwrap().clone())),
        "zigzag"
    );
    assert_eq!(first(&layer(reversed)), "roughen");
    // the baseline silently skips the unknown type (mask [false, true]); the native admission refuses it
    let ignored = fxs
        .iter()
        .find(|f| f.id == "unknown-type-ignored-by-baseline")
        .unwrap();
    assert_eq!(ignored.mask, [false, true]);
    assert!(ignored.native.is_none());
    let source = doc(
        json!([{"layerUid":"L1","pathFx":ignored.stack.clone()}]),
        vec![],
    );
    assert_eq!(kind_of(&source), Unsupported);
}

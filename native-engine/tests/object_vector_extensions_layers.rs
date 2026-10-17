//! L01-E: the per-layer `pathFx` stack through the native codec (rows, pins, presence), the canonical
//! serialization bytes of a document that carries every extension member, and the same members in a real
//! ordered v2 document (formatVersion 2 with a compositionOrder plan).
use super::data::rows;
use super::pins::FX_PINS;
use super::{brush, decode, decoded, doc, put};
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_ordered_project,
    encode_project, encode_storage_project, ObjectCodecErrorKind,
};
use crate::object_document::Presence;
use serde_json::{json, Value};

#[test]
fn layer_path_fx_rows_round_trip_in_storage_order() {
    let (_, _, fxs) = rows();
    let mut seen = 0;
    for fx in &fxs {
        let Some(native) = &fx.native else { continue };
        let source = doc(json!([native]), vec![]);
        let document = decode(&source).unwrap_or_else(|e| panic!("{}: {e}", fx.id));
        let bytes = encode_project(&document).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            source,
            "{}",
            fx.id
        );
        assert_eq!(native["pathFx"], fx.stack, "{}", fx.id);
        assert_eq!(decode_project(&bytes).unwrap(), document, "{}", fx.id);
        assert_eq!(
            encode_storage_project(&decode_storage_project(&bytes).unwrap()).unwrap(),
            bytes
        );
        seen += 1;
    }
    assert_eq!(seen, 8);
}

/// `zigzag/t/10,3,1`-style pins decoded by hand: the stored members, entry by entry.
#[test]
fn layer_path_fx_entries_match_the_pinned_baseline_facts() {
    let (_, _, fxs) = rows();
    for (id, pin) in FX_PINS {
        let fx = fxs.iter().find(|f| f.id == id).unwrap();
        let document = decode(&doc(json!([fx.native.clone().unwrap()]), vec![])).unwrap();
        let stack = document.layers()[0].path_fx.value().expect("pathFx");
        let want: Vec<&str> = pin.split_whitespace().collect();
        assert_eq!(stack.len(), want.len(), "{id}");
        for (i, (entry, text)) in stack.iter().zip(&want).enumerate() {
            let parts: Vec<&str> = text.split('/').collect();
            assert_eq!(entry.fx_type, parts[0], "{id}[{i}]");
            let flag = match parts[1] {
                "t" => Some(true),
                "f" => Some(false),
                _ => None,
            };
            assert_eq!(entry.enabled_flag(), flag, "{id}[{i}]");
            for (k, want_p) in parts[2].split(',').enumerate() {
                let slot = [&entry.p1, &entry.p2, &entry.p3][k];
                match want_p {
                    "-" => assert!(slot.missing(), "{id}[{i}].p{}", k + 1),
                    "n" => assert!(matches!(slot.value(), Some(None)), "{id}[{i}].p{}", k + 1),
                    number => assert_eq!(
                        entry.param(k),
                        Some(number.parse().unwrap()),
                        "{id}[{i}].p{}",
                        k + 1
                    ),
                }
            }
        }
        // the baseline activeStack mask is `enabled !== false` for every (known) entry
        let flags: Vec<bool> = stack
            .iter()
            .map(|e| e.enabled_flag() != Some(false))
            .collect();
        assert_eq!(flags, fx.mask, "{id}");
        assert_eq!(flags.iter().any(|f| *f), fx.has_any, "{id}");
    }
}

#[test]
fn layer_path_fx_absent_empty_and_populated_are_distinct() {
    let layer = |fx: Option<Value>| {
        let mut l = json!({"layerUid":"L1"});
        if let Some(stack) = fx {
            l["pathFx"] = stack;
        }
        decode(&doc(json!([l]), vec![])).unwrap()
    };
    let (absent, empty, one) = (
        layer(None),
        layer(Some(json!([]))),
        layer(Some(json!([{"type":"zigzag"}]))),
    );
    assert!(absent.layers()[0].path_fx.missing());
    assert_eq!(absent.layers()[0].path_fx().map(<[_]>::len), None);
    assert_eq!(empty.layers()[0].path_fx().map(<[_]>::len), Some(0));
    assert_eq!(one.layers()[0].path_fx().map(<[_]>::len), Some(1));
    assert_ne!(absent, empty);
    assert_ne!(empty, one);
    let bytes = encode_project(&empty).unwrap();
    assert!(String::from_utf8(bytes.clone())
        .unwrap()
        .contains(r#""pathFx":[]"#));
    assert_eq!(decode_project(&bytes).unwrap(), empty);
}

const CANONICAL: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L1","pathFx":[{"type":"zigzag","enabled":true,"p1":10,"p2":null,"p3":1},{"type":"roughen","p2":6}]}],"objects":[{"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},{"point":{"x":40,"y":20},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},"fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":6,"strokeCap":"round","strokeJoin":"round","opacity":1},"vectorExt":{"pressure":{"centerSegments":[{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":5,"y":5},"width":0.5},{"point":{"x":40,"y":20},"handleIn":{"x":-5,"y":-5},"handleOut":{"x":0,"y":0},"width":4}],"widthProfile":[{"t":0,"width":0.5},{"t":1,"width":4}],"strokeProfile":"bulge","profileBase":0},"paramShape":{"kind":"rect","tl":0,"box":{"x1":0,"y1":0,"x2":40,"y2":20}},"boxAngle":-15}}]}"#;
const PLAIN: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L1"}],"objects":[]}"#;

#[test]
fn canonical_bytes_are_pinned() {
    for text in [CANONICAL, PLAIN] {
        let document = decode_project(text.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            String::from_utf8(encode_project(&document).unwrap()).unwrap(),
            text
        );
        let stored = decode_storage_project(text.as_bytes()).unwrap();
        assert_eq!(
            String::from_utf8(encode_storage_project(&stored).unwrap()).unwrap(),
            text
        );
    }
}

/// The smallest plan that covers the one authored object of `CANONICAL` (layer L1, stroke v1, frame 0).
fn ordered_plan() -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root",
            "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},
                "elements":[{"layerUid":"L1","strokeId":"v1","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"})
}
fn ordered_source() -> Value {
    let mut source: Value = serde_json::from_str(CANONICAL).unwrap();
    source["formatVersion"] = json!(2);
    source["compositionOrder"] = ordered_plan();
    source
}
fn ordered_bytes(source: &Value) -> Vec<u8> {
    serde_json::to_vec(source).unwrap()
}

/// The v2 storage variant of the extension members: a real ordered document (not the v1 bytes re-read through
/// the storage entry point) keeps vectorExt and pathFx literally, with null / zero / absent and the stack
/// order unchanged, through both the ordered and the explicit storage codec.
#[test]
fn ordered_v2_documents_keep_vector_ext_and_path_fx_null_zero_and_order() {
    let source = ordered_source();
    let bytes = ordered_bytes(&source);
    let ordered = decode_ordered_project(&bytes).unwrap_or_else(|e| panic!("v2 refused: {e}"));
    let encoded = encode_ordered_project(&ordered).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), source);
    let stored = decode_storage_project(&bytes).unwrap();
    assert_eq!(stored.format_version, 2);
    assert_eq!(encode_storage_project(&stored).unwrap(), encoded);
    // the stack: storage order, enabled absent / true, p1 number, p2 null vs number, p3 number vs absent
    let stack = ordered.layers()[0].path_fx().expect("pathFx");
    let types: Vec<&str> = stack.iter().map(|e| e.fx_type.as_str()).collect();
    assert_eq!(types, ["zigzag", "roughen"]);
    assert_eq!(stack[0].enabled_flag(), Some(true));
    assert_eq!(stack[1].enabled_flag(), None);
    assert_eq!(stack[0].param(0), Some(10.0));
    assert!(matches!(stack[0].p2.value(), Some(None)));
    assert_eq!(stack[0].param(2), Some(1.0));
    assert!(stack[1].p1.missing() && stack[1].p3.missing());
    assert_eq!(stack[1].param(1), Some(6.0));
    // the record: explicit zero profileBase and tl, absent tr, the negative boxAngle
    let ext = ordered.objects()[0].vector_ext.value().expect("vectorExt");
    let pressure = ext.pressure.value().expect("pressure");
    assert_eq!(
        pressure.profile_base.value().and_then(|n| n.as_f64()),
        Some(0.0)
    );
    assert_eq!(
        pressure.stroke_profile.value().map(String::as_str),
        Some("bulge")
    );
    let shape = ext.param_shape.value().expect("paramShape");
    assert_eq!(shape.tl.value().and_then(|n| n.as_f64()), Some(0.0));
    assert!(shape.tr.missing());
    assert_eq!(ext.box_angle.value().and_then(|n| n.as_f64()), Some(-15.0));
}

#[test]
fn ordered_v2_stack_order_and_p2_states_are_distinct_documents() {
    let with_stack = |stack: Value| {
        let mut source = ordered_source();
        source["layers"][0]["pathFx"] = stack;
        let ordered = decode_ordered_project(&ordered_bytes(&source)).unwrap();
        let encoded = encode_ordered_project(&ordered).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), source);
        ordered
    };
    let forward = with_stack(json!([{"type":"zigzag"},{"type":"roughen"}]));
    let reversed = with_stack(json!([{"type":"roughen"},{"type":"zigzag"}]));
    assert_ne!(forward, reversed);
    assert_eq!(
        reversed.layers()[0].path_fx().unwrap()[0].fx_type,
        "roughen"
    );
    let (absent, null, zero) = (
        with_stack(json!([{"type":"zigzag"}])),
        with_stack(json!([{"type":"zigzag","p2":null}])),
        with_stack(json!([{"type":"zigzag","p2":0}])),
    );
    assert_ne!(absent, null);
    assert_ne!(null, zero);
    assert_ne!(absent, zero);
    let empty = with_stack(json!([]));
    assert_eq!(empty.layers()[0].path_fx().map(<[_]>::len), Some(0));
    let mut none = ordered_source();
    none["layers"][0].as_object_mut().unwrap().remove("pathFx");
    let none = decode_ordered_project(&ordered_bytes(&none)).unwrap();
    assert!(none.layers()[0].path_fx.missing());
    assert_ne!(none, empty);
}

#[test]
fn ordered_v2_refuses_what_v1_refuses_for_the_extension_members() {
    let kind_of = |source: &Value| {
        decode_ordered_project(&ordered_bytes(source))
            .expect_err("mutated v2 document must be refused")
            .kind()
    };
    let mut unknown_fx = ordered_source();
    unknown_fx["layers"][0]["pathFx"][0]["type"] = json!("wobble");
    assert_eq!(kind_of(&unknown_fx), ObjectCodecErrorKind::Unsupported);
    let mut decreasing = ordered_source();
    decreasing["objects"][0]["vectorExt"]["pressure"]["widthProfile"][1]["t"] = json!(-0.5);
    assert_eq!(kind_of(&decreasing), ObjectCodecErrorKind::Invalid);
    let mut foreign = ordered_source();
    foreign["objects"][0]["vectorExt"]["paramShape"]["sweep"] = json!(90);
    assert_eq!(kind_of(&foreign), ObjectCodecErrorKind::Invalid);
    for bad in [&unknown_fx, &decreasing, &foreign] {
        assert!(decode_storage_project(&ordered_bytes(bad)).is_err());
    }
    // the ordered encoder re-runs the admission on a document altered after decode
    let ordered = decode_ordered_project(&ordered_bytes(&ordered_source())).unwrap();
    let mut altered = ordered.clone();
    if let Presence::Present(stack) = &mut altered.layers[0].path_fx {
        stack[0].fx_type = "wobble".into();
    }
    assert_eq!(
        encode_ordered_project(&altered).unwrap_err().kind(),
        ObjectCodecErrorKind::Unsupported
    );
    assert!(encode_ordered_project(&ordered).is_ok());
}

/// `boxAngle: 0` is stored as an explicit value and differs from an absent `boxAngle` (the baseline `serP`
/// drops a falsy angle, the native record does not conflate the two).
#[test]
fn box_angle_zero_is_an_explicit_value_not_an_absent_one() {
    let with_angle = |angle: Option<f64>| {
        let mut ext = json!({"paramShape":{"kind":"rect"}});
        if let Some(a) = angle {
            ext["boxAngle"] = json!(a);
        }
        decoded(put(brush(), "/vectorExt", ext))
    };
    let (zero, absent) = (with_angle(Some(0.0)), with_angle(None));
    assert_ne!(zero, absent);
    let text = String::from_utf8(encode_project(&zero).unwrap()).unwrap();
    assert!(text.contains(r#""boxAngle":0.0"#), "{text}");
    assert_eq!(decode_project(text.as_bytes()).unwrap(), zero);
    let text = String::from_utf8(encode_project(&absent).unwrap()).unwrap();
    assert!(!text.contains("boxAngle"), "{text}");
}

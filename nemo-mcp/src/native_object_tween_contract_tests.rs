//! D20-D22 (#2358): the four tween commands are admitted exactly as the native planner reads them.
use super::tests::{envelope, P, TWEEN};
use super::*;
use serde_json::{json, Value};

fn options() -> Value {
    json!({"resamplePoints":50,"step":1,"skipManual":true,"easing":[{"x":0,"y":0},{"x":0.5,"y":0.2,"tx":1,"ty":0},{"x":1,"y":1}]})
}
fn span() -> Value {
    json!({"from":0,"to":4,"easing":[{"x":0,"y":0},{"x":1,"y":1}],
        "overrides":[{"aId":"a","bId":"b"},{"aIds":["c","d"],"bId":"e"}],"manualStrokes":["a","c"]})
}
fn valid(suffix: &str) -> Value {
    match suffix {
        "tween.generate" | "tween.regenerate" => {
            json!({"command":format!("object.{suffix}"),"layerUid":"layer-1","options":options(),"selection":[1,6]})
        }
        "tween.split" => {
            json!({"command":"object.tween.split","layerUid":"layer-1","frame":2,"options":options()})
        }
        _ => {
            json!({"command":"object.tween.reassign","layerUid":"layer-1","span":span(),"options":options()})
        }
    }
}
fn set(mut payload: Value, at: &str, value: Value) -> Value {
    let (parent, key) = at.rsplit_once('/').unwrap_or(("", at));
    let target = if parent.is_empty() {
        &mut payload
    } else {
        payload.pointer_mut(parent).unwrap()
    };
    if value.is_null() {
        match target {
            Value::Object(map) => {
                map.remove(key);
            }
            Value::Array(items) => {
                items.remove(key.parse().unwrap());
            }
            _ => unreachable!(),
        }
    } else if let Value::Array(items) = target {
        items[key.parse::<usize>().unwrap()] = value;
    } else {
        target[key] = value;
    }
    payload
}

#[test]
fn each_command_admits_its_documented_shapes() {
    for suffix in TWEEN {
        let operation = format!("{P}{suffix}");
        assert!(request(&operation, &valid(suffix)), "{suffix}");
        assert!(envelope(&operation, valid(suffix)).validate().is_ok());
    }
    let generate = format!("{P}tween.generate");
    for ok in [
        json!({"command":"object.tween.generate","layerUid":"l"}),
        json!({"command":"object.tween.generate","layerUid":"l","starts":[]}),
        json!({"command":"object.tween.generate","layerUid":"l","selection":[0],"options":options()}),
        json!({"command":"object.tween.generate","layerUid":"l","starts":[4294967295u32]}),
    ] {
        assert!(request(&generate, &ok), "{ok}");
    }
}

#[test]
fn a_flawed_member_is_refused_for_every_command() {
    let generate = |at: &str, bad: Value| ("tween.generate", set(valid("tween.generate"), at, bad));
    let regenerate =
        |at: &str, bad: Value| ("tween.regenerate", set(valid("tween.regenerate"), at, bad));
    let split = |at: &str, bad: Value| ("tween.split", set(valid("tween.split"), at, bad));
    let reassign = |at: &str, bad: Value| ("tween.reassign", set(valid("tween.reassign"), at, bad));
    let long = "x".repeat(256);
    let cases = vec![
        generate("/layerUid", json!("")),
        generate("/layerUid", json!(long)),
        generate("/layerUid", json!(5)),
        generate("/extra", json!(1)),
        generate("/command", json!("object.tween.split")),
        generate("/selection", json!([-1])),
        generate("/selection", json!([4294967296u64])),
        generate("/selection", json!([1.5])),
        generate("/selection", json!(vec![0; 4097])),
        generate("/starts", json!([1])),
        generate("/options/resamplePoints", json!(1)),
        generate("/options/resamplePoints", json!(257)),
        generate("/options/step", json!(0)),
        generate("/options/step", json!(1000)),
        generate("/options/skipManual", json!("yes")),
        generate("/options/easing", json!(vec![json!({"x":0,"y":0}); 257])),
        generate("/options/easing/0/x", json!(-0.1)),
        generate("/options/easing/0/x", json!(1.1)),
        generate("/options/easing/0/z", json!(1)),
        generate("/options/easing/1/tx", json!("1")),
        generate("/options/resamplePoints", json!(Value::Null)),
        generate("/options/extra", json!(true)),
        regenerate("/options/step", json!(0)),
        regenerate("/selection", json!("1")),
        split("/frame", json!(-1)),
        split("/frame", json!(null)),
        split("/frame", json!("2")),
        split("/selection", json!([1])),
        split("/span", span()),
        reassign("/span", json!(null)),
        reassign("/span/from", json!(-1)),
        reassign("/span/to", json!(1.5)),
        reassign(
            "/span/overrides",
            json!(vec![json!({"aId":"a","bId":"b"}); 129]),
        ),
        reassign("/span/overrides/0/bId", json!("")),
        reassign("/span/overrides/0/aIds", json!(["a"])),
        reassign("/span/overrides/1/aIds", json!([])),
        reassign("/span/overrides/1/aId", json!("a")),
        reassign("/span/overrides/0/aId", json!(null)),
        reassign("/span/manualStrokes", json!(vec!["m"; 129])),
        reassign("/span/manualStrokes/0", json!("")),
        reassign("/span/easing/0/y", json!("0")),
        reassign("/span/extra", json!(1)),
        reassign("/frame", json!(1)),
        reassign("/starts", json!([0])),
    ];
    for (suffix, payload) in cases {
        assert!(
            !request(&format!("{P}{suffix}"), &payload),
            "{suffix}: {payload}"
        );
    }
}

#[test]
fn starts_and_selection_are_mutually_exclusive_and_split_reassign_need_their_member() {
    let generate = format!("{P}tween.generate");
    let both =
        json!({"command":"object.tween.generate","layerUid":"l","starts":[0],"selection":[1]});
    assert!(!request(&generate, &both));
    let split = format!("{P}tween.split");
    assert!(!request(
        &split,
        &json!({"command":"object.tween.split","layerUid":"l"})
    ));
    let reassign = format!("{P}tween.reassign");
    assert!(!request(
        &reassign,
        &json!({"command":"object.tween.reassign","layerUid":"l"})
    ));
    for (operation, payload) in [
        (&generate, valid("tween.split")),
        (&split, valid("tween.generate")),
        (&reassign, valid("tween.regenerate")),
    ] {
        assert!(!request(operation, &payload), "{operation}");
    }
}

#[test]
fn the_budget_is_sixty_four_kib_for_the_four_commands_and_unchanged_elsewhere() {
    for suffix in TWEEN {
        assert_eq!(
            crate::wire::native_request_byte_limit(&format!("{P}{suffix}")),
            65_536
        );
    }
    for other in ["labs.boil", "widget.write", "layer.tween.remove"] {
        assert_eq!(
            crate::wire::native_request_byte_limit(&format!("{P}{other}")),
            4096,
            "{other}"
        );
    }
    // The widest frame list the planner accepts is admitted from original bytes; one more byte-heavy byte set is not.
    let frames: Vec<u32> = (0..4096).map(|i| 4_000_000_000 + i).collect();
    let wide = json!({"apiVersion":2,"requestId":"w","instanceId":"i","documentId":"d","expectedRevision":1,
        "operation":format!("{P}tween.generate"),
        "payload":{"command":"object.tween.generate","layerUid":"l","starts":frames}});
    let bytes = serde_json::to_vec(&wide).unwrap();
    assert!(bytes.len() > 4096 && bytes.len() < 65_536);
    assert!(crate::wire::decode_native_request(&bytes).is_ok());
    let huge = json!({"apiVersion":2,"requestId":"w","instanceId":"i","documentId":"d","expectedRevision":1,
        "operation":format!("{P}tween.generate"),
        "payload":{"command":"object.tween.generate","layerUid":"l","selection":[0],"pad":"x".repeat(70_000)}});
    assert!(crate::wire::decode_native_request(&serde_json::to_vec(&huge).unwrap()).is_err());
}

#[test]
fn a_missing_revision_is_refused_before_dispatch() {
    for suffix in TWEEN {
        let mut request = envelope(&format!("{P}{suffix}"), valid(suffix));
        request.expected_revision = None;
        assert!(request.validate().is_err(), "{suffix}");
    }
}

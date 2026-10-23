use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

pub(super) const P: &str = "command.document.object.";

/// Literal wire payloads, spelled out independently of the table under test.
pub(super) fn valid() -> Vec<(&'static str, Value)> {
    vec![
        (
            "component.convert",
            json!({"command":"object.component.convert","contextId":"scene-root","layerUids":["layer-a","layer-b"]}),
        ),
        (
            "component.expose",
            json!({"command":"object.component.expose","symbolId":"native-component:6:0","key":"ep_mfz9q1k2_8x2a",
            "targetStrokeId":"stroke-1","targetField":"opacity","label":"Eye","default":100}),
        ),
        (
            "component.place",
            json!({"command":"object.component.place","symbolId":"native-component:6:0","name":"Hand"}),
        ),
        (
            "duplicator.set",
            json!({"command":"object.duplicator.set","layerUid":"layer-a","enabled":true,"seed":482913}),
        ),
        (
            "duplicator.objects",
            json!({"command":"object.duplicator.objects","layerUids":["layer-a","layer-b"],"seed":17}),
        ),
        (
            "duplicator.path",
            json!({"command":"object.duplicator.path","layerUid":"layer-a","pathLayerUid":"layer-b"}),
        ),
    ]
}

pub(super) fn envelope(suffix: &str, payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"component-1","instanceId":"instance-A",
        "documentId":"document-A","expectedRevision":7,"operation":format!("{P}{suffix}"),"payload":payload}))
    .unwrap()
}

pub(super) fn response(value: Value) -> NativeApplicationResponse {
    serde_json::from_value(value).unwrap()
}

fn patched(suffix: &str, key: &str, value: Value) -> Value {
    let mut payload = valid().into_iter().find(|(s, _)| *s == suffix).unwrap().1;
    payload[key] = value;
    payload
}

fn without(suffix: &str, key: &str) -> Value {
    let mut payload = valid().into_iter().find(|(s, _)| *s == suffix).unwrap().1;
    payload.as_object_mut().unwrap().remove(key);
    payload
}

#[test]
fn the_six_declared_commands_are_exactly_the_admitted_operations() {
    let expected = [
        "component.convert",
        "component.expose",
        "component.place",
        "duplicator.set",
        "duplicator.objects",
        "duplicator.path",
    ];
    assert_eq!(SPECS.len(), 6);
    assert_eq!(
        valid().iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        expected
    );
    for suffix in expected {
        assert!(admitted(&format!("{P}{suffix}")), "{suffix}");
    }
    for other in [
        "command.document.object.layer.create",
        "command.document.object.layer.policy.edit",
        "command.document.object.layer.split",
        "command.document.object.component",
        "command.document.object.component.enter",
        "command.document.object.component.convert.extra",
        "command.document.object.duplicator.convert",
        "query.document.object",
        "component.convert",
        "",
    ] {
        assert!(!admitted(other), "{other}");
        assert!(!request(other, &json!({})), "{other}");
    }
}

#[test]
fn every_literal_payload_passes_shape_and_full_admission_and_requires_a_revision() {
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        assert!(request(&operation, &payload), "{suffix}");
        assert!(
            crate::native_contract::validate_request(&operation, &payload),
            "{suffix}"
        );
        let mut command = envelope(suffix, payload.clone());
        assert!(command.validate_input().is_ok(), "{suffix}");
        assert!(command.validate().is_ok(), "{suffix}");
        command.expected_revision = None;
        assert!(
            command.validate_input().is_err(),
            "{suffix} needs expectedRevision"
        );
        command.expected_revision = Some(9_007_199_254_740_992);
        assert!(
            command.validate_input().is_err(),
            "{suffix} unsafe revision"
        );
    }
}

#[test]
fn one_member_mutations_are_refused_by_shape_and_by_input_admission() {
    let optional = [("component.expose", "default"), ("duplicator.set", "seed")];
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        let mut mutants = vec![json!([]), Value::Null, json!("x")];
        for key in payload.as_object().unwrap().keys() {
            if !optional.contains(&(suffix, key.as_str())) {
                mutants.push(without(suffix, key));
            }
            let mut null = payload.clone();
            null[key] = Value::Null;
            // `pathLayerUid` is the one member whose declared domain includes null.
            if !(suffix == "duplicator.path" && key == "pathLayerUid") {
                mutants.push(null);
            }
        }
        let mut extra = payload.clone();
        extra["extra"] = json!(true);
        mutants.push(extra);
        let mut foreign = payload.clone();
        foreign["command"] = json!("object.layer.policy.edit");
        mutants.push(foreign);
        for mutant in mutants {
            assert!(!request(&operation, &mutant), "{suffix}: {mutant}");
            assert!(
                envelope(suffix, mutant.clone()).validate_input().is_err(),
                "{suffix}: {mutant}"
            );
        }
    }
    let other = valid()
        .into_iter()
        .find(|(s, _)| *s == "component.place")
        .unwrap()
        .1;
    assert!(
        !request(&format!("{P}component.convert"), &other),
        "another operation's payload"
    );
}

#[test]
fn optional_members_are_absent_or_valid_and_the_path_member_is_required_but_nullable() {
    let operation = |suffix: &str| format!("{P}{suffix}");
    assert!(request(
        &operation("component.expose"),
        &without("component.expose", "default")
    ));
    assert!(request(
        &operation("component.expose"),
        &patched("component.expose", "default", json!(0))
    ));
    assert!(request(
        &operation("component.expose"),
        &patched("component.expose", "default", json!(-2.5))
    ));
    for bad in [json!(null), json!("100"), json!(true), json!([1])] {
        assert!(
            !request(
                &operation("component.expose"),
                &patched("component.expose", "default", bad.clone())
            ),
            "{bad}"
        );
    }
    assert!(request(
        &operation("duplicator.set"),
        &without("duplicator.set", "seed")
    ));
    assert!(request(
        &operation("duplicator.set"),
        &patched("duplicator.set", "enabled", json!(false))
    ));
    assert!(request(
        &operation("duplicator.path"),
        &patched("duplicator.path", "pathLayerUid", Value::Null)
    ));
    assert!(
        !request(
            &operation("duplicator.path"),
            &without("duplicator.path", "pathLayerUid")
        ),
        "the clear is an explicit null, never an absent member"
    );
}

#[test]
fn bounds_mirror_the_owner_identity_list_key_and_seed_domains() {
    let ok = |suffix: &str, key: &str, value: Value| {
        assert!(
            request(
                &format!("{P}{suffix}"),
                &patched(suffix, key, value.clone())
            ),
            "{suffix} {key} {value}"
        );
    };
    let no = |suffix: &str, key: &str, value: Value| {
        assert!(
            !request(
                &format!("{P}{suffix}"),
                &patched(suffix, key, value.clone())
            ),
            "{suffix} {key} {value}"
        );
    };
    ok("component.place", "symbolId", json!("s".repeat(128)));
    no("component.place", "symbolId", json!("s".repeat(129)));
    ok("component.place", "symbolId", json!("\u{e9}".repeat(64)));
    no("component.place", "symbolId", json!("\u{e9}".repeat(65)));
    no("component.place", "symbolId", json!(""));
    no("component.place", "symbolId", json!("a\u{0}b"));
    ok("component.place", "symbolId", json!("a\u{7}b"));
    ok("component.place", "name", json!(""));
    ok("component.place", "name", json!("n".repeat(2000)));
    no("component.place", "name", json!(7));
    ok("component.convert", "contextId", json!("scene-root"));
    ok(
        "component.convert",
        "contextId",
        json!("native-component:6:0"),
    );
    no("component.convert", "contextId", json!(null));
    let many = |n: usize| json!((0..n).map(|i| format!("l{i}")).collect::<Vec<_>>());
    ok("component.convert", "layerUids", many(256));
    no("component.convert", "layerUids", many(257));
    no("component.convert", "layerUids", json!([]));
    no("component.convert", "layerUids", json!([""]));
    ok("component.convert", "layerUids", json!(["a", "a"]));
    no("duplicator.objects", "layerUids", json!([]));
    ok("component.expose", "key", json!("ep_x"));
    no("component.expose", "key", json!("ep_"));
    no("component.expose", "key", json!("ex_abc"));
    no("component.expose", "key", json!("EP_abc"));
    ok(
        "component.expose",
        "key",
        json!(format!("ep_{}", "k".repeat(125))),
    );
    no(
        "component.expose",
        "key",
        json!(format!("ep_{}", "k".repeat(126))),
    );
    no("component.expose", "targetField", json!(""));
    ok("component.expose", "targetField", json!("__visible"));
    ok("component.expose", "label", json!(""));
    no("component.expose", "label", json!(null));
    ok("duplicator.set", "seed", json!(0));
    ok("duplicator.set", "seed", json!(999_999));
    ok("duplicator.set", "seed", json!(4_294_967_295u64));
    for bad in [
        json!(4_294_967_296u64),
        json!(-1),
        json!(0.5),
        json!("7"),
        json!(null),
    ] {
        no("duplicator.set", "seed", bad.clone());
        no("duplicator.objects", "seed", bad);
    }
    no("duplicator.objects", "seed", json!(null));
    no("duplicator.set", "enabled", json!(1));
    no("duplicator.set", "enabled", json!("true"));
    ok("duplicator.path", "pathLayerUid", json!(""));
    ok("duplicator.path", "pathLayerUid", json!("p".repeat(128)));
    no("duplicator.path", "pathLayerUid", json!("p".repeat(129)));
    no("duplicator.path", "pathLayerUid", json!(7));
}

//! L02-G admission: the per-layer group table through the single native codec.
//! Every refusal is a typed codec error with its exact kind and message; absent,
//! empty and explicit tables stay distinct; nothing is repaired on load.
use crate::object_codec::{
    decode_ordered_project, decode_storage_project, encode_storage_project, ObjectCodecErrorKind,
};
use serde_json::{json, Value};

pub(super) fn record(layer: &str, stroke: &str) -> Value {
    let corner =
        |x: u32| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":layer,"strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(0), corner(10)]},
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1}})
}
/// L0 holds `ids`, L1 holds `x1`; `groups` (when not null) is L0's table.
pub(super) fn doc(version: u32, ids: &[&str], groups: Value) -> Value {
    let mut layer0 = json!({"layerUid":"L0","name":"A","visible":true});
    if !groups.is_null() {
        layer0["groups"] = groups;
    }
    let mut objects: Vec<Value> = ids.iter().map(|id| record("L0", id)).collect();
    objects.push(record("L1", "x1"));
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":2,"layers":[layer0, {"layerUid":"L1","name":"B","visible":true}],
        "objects":objects});
    if version == 2 {
        let mut elements: Vec<Value> = ids
            .iter()
            .enumerate()
            .map(|(n, id)| json!({"layerUid":"L0","strokeId":id,"naturalPosition":n,"order":{}}))
            .collect();
        elements.push(json!({"layerUid":"L1","strokeId":"x1","naturalPosition":0,"order":{}}));
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"L0","naturalPosition":0,"order":{}},
                {"layerUid":"L1","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]});
    }
    document
}
pub(super) fn tab(entries: Value, members: Value) -> Value {
    json!({"contextId":"scene-root","entries":entries,"members":members})
}
pub(super) const IDS: [&str; 5] = ["s1", "s2", "s3", "s4", "s5"];
fn valid() -> Value {
    tab(
        json!([{"gid":"g1","order":["s1","s2"],"parent":"g2"},
            {"gid":"g2","combineMode":"unite","name":"top","order":["g1","s3"]}]),
        json!([{"strokeId":"s1","groupId":"g1"},{"strokeId":"s2","groupId":"g1"},
            {"strokeId":"s3","groupId":"g2"}]),
    )
}
pub(super) fn refusal(document: &Value) -> (ObjectCodecErrorKind, String) {
    let error = decode_storage_project(document.to_string().as_bytes()).unwrap_err();
    (error.kind(), error.message().to_string())
}

#[test]
fn a_valid_table_round_trips_and_absent_empty_and_explicit_stay_distinct() {
    for version in [1, 2] {
        let with = doc(version, &IDS, valid());
        let decoded = decode_storage_project(with.to_string().as_bytes()).unwrap();
        let bytes = encode_storage_project(&decoded).unwrap();
        let again: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(again["layers"], with["layers"], "v{version}");
        assert_eq!(
            decode_storage_project(&bytes).unwrap(),
            decoded,
            "v{version}: decode(encode(x)) == x"
        );
        let empty = doc(version, &IDS, tab(json!([]), json!([])));
        let kept = decode_storage_project(empty.to_string().as_bytes()).unwrap();
        let wire: Value = serde_json::from_slice(&encode_storage_project(&kept).unwrap()).unwrap();
        assert_eq!(wire["layers"][0]["groups"], tab(json!([]), json!([])));
        let none = doc(version, &IDS, Value::Null);
        let plain = decode_storage_project(none.to_string().as_bytes()).unwrap();
        let wire: Value = serde_json::from_slice(&encode_storage_project(&plain).unwrap()).unwrap();
        assert!(
            wire["layers"][0].get("groups").is_none(),
            "v{version}: absent stays absent"
        );
        assert_ne!(kept, plain);
    }
}

#[test]
fn wire_shape_is_strict_object_only_with_null_and_unknown_members_refused() {
    let base = |groups: &str| {
        doc(1, &IDS, Value::Null).to_string().replacen(
            r#""name":"A""#,
            &format!(r#""name":"A","groups":{groups}"#),
            1,
        )
    };
    let cases: [(&str, ObjectCodecErrorKind); 9] = [
        ("null", ObjectCodecErrorKind::Invalid),
        (r#"[]"#, ObjectCodecErrorKind::Invalid),
        (
            r#"{"contextId":"scene-root","entries":[]}"#,
            ObjectCodecErrorKind::Invalid,
        ),
        (
            r#"{"contextId":"scene-root","entries":[],"members":[],"extra":1}"#,
            ObjectCodecErrorKind::Unsupported,
        ),
        (
            r#"{"contextId":"scene-root","entries":[],"entries":[],"members":[]}"#,
            ObjectCodecErrorKind::Invalid,
        ),
        (
            r#"{"contextId":"scene-root","entries":[["g",["s1","s2"]]],"members":[]}"#,
            ObjectCodecErrorKind::Invalid,
        ),
        (
            r#"{"contextId":"scene-root","entries":[{"gid":"g","order":["s1","s2"],"name":null}],"members":[]}"#,
            ObjectCodecErrorKind::Invalid,
        ),
        (
            r#"{"contextId":"scene-root","entries":[{"gid":"g","order":["s1","s2"],"extra":1}],"members":[]}"#,
            ObjectCodecErrorKind::Unsupported,
        ),
        (
            r#"{"contextId":"scene-root","entries":[],"members":[{"strokeId":"s1"}]}"#,
            ObjectCodecErrorKind::Invalid,
        ),
    ];
    for (groups, kind) in cases {
        let error = decode_storage_project(base(groups).as_bytes()).unwrap_err();
        assert_eq!(error.kind(), kind, "{groups}: {}", error.message());
    }
}

#[test]
fn every_invalid_table_is_a_typed_refusal_with_its_exact_message() {
    use ObjectCodecErrorKind::{DuplicateId, Invalid};
    let o = |gid: &str, order: &[&str]| json!({"gid":gid,"order":order});
    let cases: Vec<(&str, Value, ObjectCodecErrorKind, &str)> = vec![
        (
            "empty context",
            json!({"contextId":"","entries":[],"members":[]}),
            Invalid,
            "group table requires a contextId",
        ),
        (
            "v1 other context",
            json!({"contextId":"other","entries":[],"members":[]}),
            Invalid,
            "a v1 group table must name the scene-root context",
        ),
        (
            "empty gid",
            tab(json!([o("", &["s1", "s2"])]), json!([])),
            Invalid,
            "empty group id",
        ),
        (
            "duplicate gid",
            tab(
                json!([o("g", &["s1", "s2"]), o("g", &["s3", "s4"])]),
                json!([]),
            ),
            DuplicateId,
            "duplicate group id",
        ),
        (
            "stroke id as gid",
            tab(json!([o("s4", &["s1", "s2"])]), json!([])),
            Invalid,
            "group id collides with a stroke id",
        ),
        (
            "unknown mode",
            tab(
                json!([{"gid":"g","combineMode":"merge","order":["s1","s2"]}]),
                json!([]),
            ),
            Invalid,
            "unknown group combineMode",
        ),
        (
            "one member",
            tab(json!([o("g", &["s1"])]), json!([])),
            Invalid,
            "a group lists at least two members",
        ),
        (
            "no member",
            tab(json!([o("g", &[])]), json!([])),
            Invalid,
            "a group lists at least two members",
        ),
        (
            "unknown member",
            tab(json!([o("g", &["s1", "zz"])]), json!([])),
            Invalid,
            "group member is not a stroke of the layer context",
        ),
        (
            "other layer's member",
            tab(json!([o("g", &["s1", "x1"])]), json!([])),
            Invalid,
            "group member is not a stroke of the layer context",
        ),
        (
            "lists itself",
            tab(json!([o("g", &["g", "s1"])]), json!([])),
            Invalid,
            "a group lists itself",
        ),
        (
            "child twice",
            tab(
                json!([{"gid":"g1","order":["s1","s2"],"parent":"g2"}, o("g2", &["g1", "g1"])]),
                json!([]),
            ),
            Invalid,
            "a group lists a child group twice",
        ),
        (
            "two listers",
            tab(
                json!([{"gid":"g1","order":["s1","s2"],"parent":"g2"}, o("g2", &["g1", "s3"]), o("g3", &["g1", "s4"])]),
                json!([]),
            ),
            Invalid,
            "a group is listed by two groups",
        ),
        (
            "listed without parent",
            tab(
                json!([o("g1", &["s1", "s2"]), o("g2", &["g1", "s3"])]),
                json!([]),
            ),
            Invalid,
            "a listed group must name its parent",
        ),
        (
            "parent without listing",
            tab(
                json!([o("g1", &["s1", "s2"]), {"gid":"g2","order":["s3","s4"],"parent":"g1"}]),
                json!([]),
            ),
            Invalid,
            "group parent is not the group that lists it",
        ),
        (
            "parent cycle",
            tab(
                json!([{"gid":"g1","order":["g2","s1"],"parent":"g2"}, {"gid":"g2","order":["g1","s2"],"parent":"g1"}]),
                json!([]),
            ),
            Invalid,
            "group nesting is deeper than 16 levels or cyclic",
        ),
        (
            "two tags",
            tab(
                json!([o("g", &["s1", "s2"])]),
                json!([{"strokeId":"s1","groupId":"g"},{"strokeId":"s1","groupId":"g"}]),
            ),
            Invalid,
            "a stroke carries one group tag",
        ),
        (
            "tag outside",
            tab(
                json!([o("g", &["s1", "s2"])]),
                json!([{"strokeId":"zz","groupId":"g"}]),
            ),
            Invalid,
            "group tag names a stroke outside the layer context",
        ),
        (
            "tag unlisted",
            tab(
                json!([o("g", &["s1", "s2"])]),
                json!([{"strokeId":"s4","groupId":"g"}]),
            ),
            Invalid,
            "group tag names a group that does not list the stroke",
        ),
        (
            "tag unknown group",
            tab(
                json!([o("g", &["s1", "s2"])]),
                json!([{"strokeId":"s1","groupId":"nope"}]),
            ),
            Invalid,
            "group tag names a group that does not list the stroke",
        ),
    ];
    assert!(decode_storage_project(doc(1, &IDS, valid()).to_string().as_bytes()).is_ok());
    for (label, groups, kind, message) in cases {
        let got = refusal(&doc(1, &IDS, groups.clone()));
        assert_eq!(got, (kind, message.to_string()), "{label}");
        // v2 has complete contexts, so the v1-only context rule does not apply.
        if label != "v1 other context" {
            let v2 = decode_ordered_project(doc(2, &IDS, groups).to_string().as_bytes());
            let v2 = v2.unwrap_err();
            assert_eq!((v2.kind(), v2.message()), (kind, message), "{label} v2");
        }
    }
}

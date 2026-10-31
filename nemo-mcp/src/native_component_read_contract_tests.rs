use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{decode_native_request, validate_native_response};
use serde_json::json;

const SNAPSHOT: &str = "native-object:document:7";

fn envelope(selector: Value) -> Value {
    json!({"apiVersion":2,"requestId":"component-read","instanceId":"instance","documentId":"document",
        "operation":OP,"payload":{"atRevision":7,"documentSnapshotId":SNAPSHOT,"selector":selector}})
}
fn contexts() -> Value {
    json!({"kind":"contexts","cursor":0,"limit":4})
}
fn answer(result: Value) -> Value {
    json!({"apiVersion":2,"requestId":"component-read","instanceId":"instance","documentId":"document",
        "contentRevision":7,"ok":true,"result":result})
}
fn head(kind: &str) -> Value {
    json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"kind":kind})
}
fn with(mut base: Value, extra: Value) -> Value {
    for (key, value) in extra.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}
fn contexts_result() -> Value {
    with(
        head("contexts"),
        json!({"contextCount":3,"cursor":0,"nextCursor":2,"items":[
            {"contextId":"scene-root","contextKind":"scene","name":null,"totalFrames":24,"layerCount":4},
            {"contextId":"sym_a","contextKind":"component","name":"Comp","totalFrames":12,"layerCount":2}]}),
    )
}
fn context_result() -> Value {
    with(
        head("context"),
        json!({"contextId":"sym_a","contextKind":"component","name":"Comp","totalFrames":12,
            "layerCount":2,"fps":24,"instanceCount":1,"exposedPropertyCount":0,"hasCameraKeys":false}),
    )
}
fn layers_result() -> Value {
    with(
        head("layers"),
        json!({"contextId":"sym_a","layerCount":2,"cursor":0,"nextCursor":null,"items":[
            {"layerUid":"ly_a","name":"Inner A","layerKind":"drawing","visible":false,"locked":true,"parentLayerUid":null,"threeD":false,"hasMotion":true},
            {"layerUid":"ly_b","name":null,"layerKind":"widget"}]}),
    )
}
fn instances_result() -> Value {
    with(
        head("instances"),
        json!({"contextId":"sym_a","instanceCount":3,"cursor":1,"nextCursor":null,
            "items":[{"layerUid":"ly_1"},{"layerUid":"ly_2"}]}),
    )
}
fn duplicator_result() -> Value {
    with(
        head("duplicator"),
        json!({"layerUid":"ly_dup","mode":"radial","rows":2,"cols":3,"spacingX":150.5,"spacingY":-20,"count":8,"radius":200,
            "startAngle":0,"radialOrient":true,"pathAlignTangent":false,"pathLayerUid":null,"seed":4242,"sourceMode":"shape",
            "staggerRandom":{"position":true,"rotation":false,"scale":false,"opacity":false,"hue":true},
            "timeOffset":{"enabled":true,"offsetFrames":2.5,"direction":"centerOut"}}),
    )
}
fn good_cases() -> Vec<(Value, Value)> {
    vec![
        (contexts(), contexts_result()),
        (
            json!({"kind":"context","contextId":"sym_a"}),
            context_result(),
        ),
        (
            json!({"kind":"layers","contextId":"sym_a","cursor":0,"limit":16}),
            layers_result(),
        ),
        (
            json!({"kind":"instances","contextId":"sym_a","cursor":1,"limit":16}),
            instances_result(),
        ),
        (
            json!({"kind":"duplicator","layerUid":"ly_dup"}),
            duplicator_result(),
        ),
    ]
}
fn payload_of(selector: &Value) -> Value {
    envelope(selector.clone())["payload"].clone()
}

#[test]
fn published_schema_declares_the_forbidden_revision_request_and_closed_shapes() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let requests = schema["$defs"]["Request"]["oneOf"].as_array().unwrap();
    let matching: Vec<_> = requests
        .iter()
        .filter(|branch| branch["allOf"][1]["properties"]["operation"]["const"] == OP)
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(
        matching[0]["allOf"][0],
        json!({"$ref":"#/$defs/ForbiddenRevisionRequest"})
    );
    let mut resources = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let document: Value = serde_json::from_str(text).unwrap();
        let id = document["$id"].as_str().unwrap().to_owned();
        resources = resources
            .add(id, jsonschema::Resource::from_contents(document))
            .unwrap();
    }
    let registry = resources.prepare().unwrap();
    let validator = jsonschema::draft202012::options()
        .with_registry(&registry)
        .build(&schema)
        .unwrap();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert!(branches.contains(&json!({"$ref":"#/$defs/ObjectComponentReadResult"})));
    for (selector, result) in good_cases() {
        assert!(
            validator.is_valid(&envelope(selector.clone())),
            "{selector}"
        );
        assert!(validator.is_valid(&answer(result)), "{selector}");
    }
    let mut revisioned = envelope(contexts());
    revisioned["expectedRevision"] = json!(7);
    assert!(!validator.is_valid(&revisioned));
    for selector in [
        json!({"kind":"contexts","cursor":0,"limit":17}),
        json!({"kind":"contexts","cursor":-1,"limit":4}),
        json!({"kind":"context","contextId":""}),
        json!({"kind":"context","contextId":"a","cursor":0}),
        json!({"kind":"other"}),
    ] {
        assert!(!validator.is_valid(&envelope(selector)));
    }
    let mut bad = answer(contexts_result());
    bad["result"]["extra"] = json!(true);
    assert!(!validator.is_valid(&bad));
    let mut bad = answer(layers_result());
    bad["result"]["items"][0]["layerKind"] = json!("mystery");
    assert!(!validator.is_valid(&bad));
    // The drawing-gate facts are booleans (the parent a uid or null); an unknown member is still refused.
    for (key, value) in [("visible", json!("yes")), ("locked", json!(1)), ("threeD", json!(null)), ("hasMotion", json!("no")), ("parentLayerUid", json!(7)), ("extra", json!(true))] {
        let mut bad = answer(layers_result());
        bad["result"]["items"][0][key] = value;
        assert!(!validator.is_valid(&bad), "{key}");
    }
    let mut bad = answer(context_result());
    bad["result"]["fps"] = json!(0);
    assert!(!validator.is_valid(&bad));
    let mut bad = answer(contexts_result());
    bad["result"]["items"] = json!(vec![
        json!({"contextId":"c","contextKind":"component","name":null,"totalFrames":1,"layerCount":0});
        17
    ]);
    assert!(!validator.is_valid(&bad));
}

#[test]
fn the_descriptor_example_is_a_valid_request_and_answer() {
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-component-read.json"
    ))
    .unwrap();
    assert_eq!(descriptor["operations"], json!([OP]));
    let example = &descriptor["examples"][0];
    assert!(request(&example["request"]["payload"]));
    assert!(result(
        &example["request"]["payload"],
        &example["response"]["result"]
    ));
    let r: NativeApplicationRequest = serde_json::from_value(example["request"].clone()).unwrap();
    let a: NativeApplicationResponse = serde_json::from_value(example["response"].clone()).unwrap();
    r.validate().unwrap();
    validate_native_response(&a, &r).unwrap();
}

#[test]
fn request_is_exact_pinned_and_bounded() {
    for (selector, _) in good_cases() {
        assert!(request(&payload_of(&selector)), "{selector}");
    }
    for selector in [
        json!({"kind":"contexts","cursor":0,"limit":0}),
        json!({"kind":"contexts","cursor":0,"limit":17}),
        json!({"kind":"contexts","cursor":9007199254740992u64,"limit":1}),
        json!({"kind":"contexts","cursor":-1,"limit":1}),
        json!({"kind":"contexts","cursor":0}),
        json!({"kind":"contexts","cursor":0,"limit":1,"extra":1}),
        json!({"kind":"context"}),
        json!({"kind":"context","contextId":""}),
        json!({"kind":"context","contextId":"x".repeat(129)}),
        json!({"kind":"context","contextId":7}),
        json!({"kind":"layers","contextId":"a","cursor":0}),
        json!({"kind":"instances","cursor":0,"limit":1}),
        json!({"kind":"other"}),
        json!("contexts"),
        Value::Null,
    ] {
        assert!(!request(&payload_of(&selector)), "{selector}");
    }
    for payload in [
        Value::Null,
        json!({}),
        json!({"atRevision":7,"documentSnapshotId":SNAPSHOT}),
        json!({"atRevision":7,"selector":contexts()}),
        json!({"atRevision":-1,"documentSnapshotId":SNAPSHOT,"selector":contexts()}),
        json!({"atRevision":7,"documentSnapshotId":"","selector":contexts()}),
        json!({"atRevision":7,"documentSnapshotId":SNAPSHOT,"selector":contexts(),"extra":1}),
    ] {
        assert!(!request(&payload), "{payload}");
    }
}

#[test]
fn result_answers_its_own_request_pin_kind_and_page() {
    for (selector, answer) in good_cases() {
        assert!(result(&payload_of(&selector), &answer), "{selector}");
    }
    let contexts_payload = payload_of(&contexts());
    for selector in [Value::Null, json!("contexts"), json!({}), json!({"kind":"other"})] {
        let payload = payload_of(&selector);
        assert!(!result(&payload, &contexts_result()), "{selector}");
    }
    for (key, value) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("foreign")),
        ("kind", json!("layers")),
        ("extra", json!(true)),
        ("contextCount", json!(1)),
        ("cursor", json!(1)),
        ("nextCursor", json!(1)),
        ("nextCursor", json!(null)),
        ("nextCursor", json!(3)),
        ("items", json!([])),
    ] {
        let mut bad = contexts_result();
        bad[key] = value;
        assert!(!result(&contexts_payload, &bad), "{key}");
    }
    // Only the first item of the first page is the scene, and it must be.
    let mut bad = contexts_result();
    bad["items"][0]["contextId"] = json!("sym_x");
    bad["items"][0]["contextKind"] = json!("component");
    assert!(!result(&contexts_payload, &bad));
    let mut bad = contexts_result();
    bad["items"][1]["contextId"] = json!("scene-root");
    bad["items"][1]["contextKind"] = json!("scene");
    assert!(!result(&contexts_payload, &bad));
    let mut bad = contexts_result();
    bad["items"][0]["name"] = json!("Scene");
    assert!(!result(&contexts_payload, &bad));
    // An oversized component name cut by the owner is flagged `nameTruncated: true`; nothing else is admitted.
    let mut cut = contexts_result();
    cut["items"][1]["nameTruncated"] = json!(true);
    assert!(result(&contexts_payload, &cut));
    for flag in [json!(false), json!("yes"), json!(1), Value::Null] {
        let mut bad = cut.clone();
        bad["items"][1]["nameTruncated"] = flag;
        assert!(!result(&contexts_payload, &bad));
    }
    let mut bad = cut.clone();
    bad["items"][0]["nameTruncated"] = json!(true);
    assert!(!result(&contexts_payload, &bad), "the scene has no name to cut");
    let mut bad = cut.clone();
    bad["items"][1]["name"] = Value::Null;
    assert!(!result(&contexts_payload, &bad), "an absent name is not truncated");
    // A page longer than its limit.
    let mut over = contexts_payload.clone();
    over["selector"]["limit"] = json!(1);
    assert!(!result(&over, &contexts_result()));
    // The summary: kind, context and scene invariants.
    let context_payload = payload_of(&json!({"kind":"context","contextId":"sym_a"}));
    for (key, value) in [
        ("contextId", json!("sym_b")),
        ("contextKind", json!("scene")),
        ("fps", json!(0)),
        ("hasCameraKeys", json!("no")),
        ("instanceCount", json!(-1)),
        ("totalFrames", json!(4294967296u64)),
    ] {
        let mut bad = context_result();
        bad[key] = value;
        assert!(!result(&context_payload, &bad), "{key}");
    }
    let scene_payload = payload_of(&json!({"kind":"context","contextId":"scene-root"}));
    let mut scene = context_result();
    scene["contextId"] = json!("scene-root");
    scene["contextKind"] = json!("scene");
    assert!(
        !result(&scene_payload, &scene),
        "a scene has no name or fps"
    );
    scene["name"] = Value::Null;
    scene["fps"] = Value::Null;
    assert!(result(&scene_payload, &scene));
    // Layers and instances name the requested context; the scene is not instanced.
    let layers_payload =
        payload_of(&json!({"kind":"layers","contextId":"sym_a","cursor":0,"limit":16}));
    let mut bad = layers_result();
    bad["contextId"] = json!("sym_b");
    assert!(!result(&layers_payload, &bad));
    let mut bad = layers_result();
    bad["items"][0]["layerKind"] = json!("mystery");
    assert!(!result(&layers_payload, &bad));
    for (key, value) in [("visible", json!("yes")), ("locked", json!(1)), ("threeD", json!(null)), ("hasMotion", json!("no")), ("parentLayerUid", json!(7)), ("extra", json!(true))] {
        let mut bad = layers_result();
        bad["items"][0][key] = value;
        assert!(!result(&layers_payload, &bad), "{key}");
    }
    let scene_instances =
        payload_of(&json!({"kind":"instances","contextId":"scene-root","cursor":0,"limit":4}));
    let mut bad = instances_result();
    bad["contextId"] = json!("scene-root");
    bad["cursor"] = json!(0);
    assert!(!result(&scene_instances, &bad));
    assert!(!result(&Value::Null, &contexts_result()));
}

#[test]
fn registered_public_input_forbids_expected_revision_and_duplicates() {
    let r: NativeApplicationRequest =
        decode_native_request(envelope(contexts()).to_string().as_bytes()).unwrap();
    r.validate().unwrap();
    assert!(crate::native_contract::forbids_expected_revision(OP));
    assert!(crate::native_contract::validate_request(
        OP,
        &payload_of(&contexts())
    ));
    let mut bad = envelope(contexts());
    bad["expectedRevision"] = json!(7);
    assert!(decode_native_request(bad.to_string().as_bytes())
        .and_then(|r| r
            .validate()
            .map_err(|e| std::io::Error::other(e.to_string())))
        .is_err());
    let mut bad = envelope(contexts());
    bad["payload"]["selector"]["limit"] = json!(99);
    assert!(!crate::native_contract::validate_request(
        OP,
        &bad["payload"]
    ));
    let dup = envelope(contexts()).to_string().replacen(
        "\"atRevision\":7",
        "\"atRevision\":7,\"atRevision\":7",
        1,
    );
    assert!(decode_native_request(dup.as_bytes()).is_err());
}

#[test]
fn wire_result_rejects_forged_owner_revision_and_shape() {
    let r: NativeApplicationRequest = serde_json::from_value(envelope(contexts())).unwrap();
    let good: NativeApplicationResponse =
        serde_json::from_value(answer(contexts_result())).unwrap();
    validate_native_response(&good, &r).unwrap();
    for (key, value) in [
        ("requestId", json!("foreign")),
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(8)),
    ] {
        let mut bad = answer(contexts_result());
        bad[key] = value;
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("foreign")),
        ("extra", json!(true)),
        ("items", json!("none")),
    ] {
        let mut bad = answer(contexts_result());
        bad["result"][key] = value;
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &r).is_err(),
            "{key}"
        );
    }
    // A refusal answers the same request and may report the replacement document.
    let refusal = json!({"apiVersion":2,"requestId":"component-read","instanceId":"instance",
        "documentId":"document","contentRevision":7,"ok":false,
        "error":{"code":"not_found","message":"no such context"}});
    validate_native_response(&serde_json::from_value(refusal).unwrap(), &r).unwrap();
    let replaced = json!({"apiVersion":2,"requestId":"component-read","instanceId":"instance",
        "documentId":"other","contentRevision":7,"ok":false,
        "error":{"code":"wrong_document","message":"replaced"}});
    assert!(response_matches(
        &r,
        &serde_json::from_value(replaced).unwrap()
    ));
    let foreign = json!({"apiVersion":2,"requestId":"component-read","instanceId":"instance",
        "documentId":"other","contentRevision":7,"ok":false,
        "error":{"code":"not_found","message":"x"}});
    assert!(!response_matches(
        &r,
        &serde_json::from_value(foreign.clone()).unwrap()
    ));
    // Another operation's envelope is not this contract's to correlate.
    let mut elsewhere: NativeApplicationRequest = serde_json::from_value(envelope(contexts())).unwrap();
    elsewhere.operation = "query.document.object".to_string();
    assert!(response_matches(
        &elsewhere,
        &serde_json::from_value(foreign).unwrap()
    ));
}

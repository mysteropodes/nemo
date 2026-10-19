//! Literal D11 DTO/correlation controls, independent of native sampling implementation.
use super::{request, result, OP};
use crate::contract::NativeApplicationRequest;
use serde_json::{json, Value};
fn payload(keys: bool) -> Value {
    let mut view = json!({"kind":"rawSample","target":{"contextId":"scene-root","layerUid":"data:layer 雪"},"property":"opacity","frame":-0.5});
    if keys {
        view.as_object_mut().unwrap().remove("frame");
        view["kind"] = json!("authoredKeys");
        view["offset"] = json!(0);
        view["limit"] = json!(8);
    }
    json!({"atRevision":7,"documentSnapshotId":"blob:opaque snapshot","view":view})
}
fn output(keys: bool) -> Value {
    let p = payload(keys);
    let mut v = json!({"instanceId":"instance","documentId":"document","atRevision":7,"documentSnapshotId":p["documentSnapshotId"],"kind":p["view"]["kind"],"target":p["view"]["target"],"property":"opacity","dimension":1});
    if keys {
        v["trackPresent"] = json!(true);
        v["items"] = json!([
        {"frame":0,"v":[17]}, {"frame":4,"v":[80],"hold":false,"curvePoints":null,"hIn":[0,0]},
        {"frame":12,"v":[0],"curvePoints":[],"hOut":[1,2]}]);
        v["nextOffset"] = Value::Null;
    } else {
        v["frame"] = json!(-0.5);
        v["value"] = json!([17]);
    }
    v
}

#[test]
fn actual_context_view_is_closed_and_correlated_in_the_mcp_contract() {
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-property-read.json"
    ))
    .unwrap();
    let example = descriptor["examples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["request"]["payload"]["view"]["kind"] == "expressionContext")
        .unwrap();
    let p = &example["request"]["payload"];
    let v = &example["response"]["result"];
    let document = v["documentId"].as_str().unwrap();
    assert!(request(p));
    assert!(result(document, p, v));
    for (pointer, replacement) in [
        ("/frame", json!(99)),
        ("/self/property", json!("foreign")),
        ("/self/index", json!(-1)),
        ("/comp/numLayers", json!(99)),
        ("/comp/frameDuration", json!(0)),
        ("/comp/duration", json!(99)),
        ("/compUnavailable", json!("projectMetadata")),
        ("/keyCount", json!(65537)),
        ("/target/layerUid", json!("foreign")),
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(!result(document, p, &bad), "{pointer}");
    }
    for pointer in ["", "/self", "/comp"] {
        let mut bad = v.clone();
        bad.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        assert!(!result(document, p, &bad), "{pointer}");
    }
    let mut unavailable = v.clone();
    unavailable["comp"] = Value::Null;
    unavailable["compUnavailable"] = json!("projectMetadata");
    assert!(result(document, p, &unavailable));
    unavailable["compUnavailable"] = Value::Null;
    assert!(!result(document, p, &unavailable));

    // Real views include unresolved element holders and resolved parent names.
    let mut parent = v.clone();
    parent["self"]["parent"] = json!({"name":"Parent","index":1});
    parent["self"]["hasParent"] = json!(true);
    assert!(result(document, p, &parent));
    let mut unresolved = v.clone();
    unresolved["self"] = Value::Null;
    assert!(result(document, p, &unresolved));
    let mut unnamed = v.clone();
    unnamed["self"]["name"] = Value::Null;
    assert!(result(document, p, &unnamed));
    // Preserve the baseline alias's (totalFrames || 1) / fps behavior.
    let mut empty = v.clone();
    empty["comp"]["frames"] = json!(0);
    empty["comp"]["duration"] = json!(1. / 25.);
    assert!(result(document, p, &empty));
    for pointer in ["/self", "/self/parent", "/comp"] {
        let object = parent.pointer(pointer).unwrap().as_object().unwrap();
        for field in object.keys() {
            let mut bad = parent.clone();
            bad.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(!result(document, p, &bad), "missing {pointer}/{field}");
        }
    }
    for (pointer, replacement) in [
        ("/self/name", json!(false)),
        ("/self/index", json!(u64::from(u32::MAX) + 1)),
        ("/self/isShape", json!(0)),
        ("/self/hasParent", json!("true")),
        ("/self/parent/name", Value::Null),
        ("/self/parent/index", json!(-1)),
        ("/comp/width", json!(0)),
        ("/comp/height", json!(0)),
        ("/comp/frames", json!(-1)),
        ("/comp/layers", json!(false)),
        ("/comp/name", Value::Null),
        ("/comp/fps", json!(0)),
        ("/comp/fps", json!(-25)),
        ("/comp/fps", json!("25")),
        ("/keyCount", json!(-1)),
        ("/keyCount", json!(true)),
    ] {
        let mut bad = parent.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(!result(document, p, &bad), "{pointer}");
    }
}
#[test]
fn property_read_shapes_and_presence_are_closed_without_inventing_track_defaults() {
    for keys in [false, true] {
        let p = payload(keys);
        assert!(request(&p));
        assert!(result("document", &p, &output(keys)));
        let mut q: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"read","instanceId":"instance","documentId":"document","operation":OP,"payload":p})).unwrap();
        assert!(q.validate().is_ok());
        q.expected_revision = Some(7);
        assert!(q.validate_input().is_err());
        for pointer in ["", "/view", "/view/target"] {
            let object = p.pointer(pointer).unwrap().as_object().unwrap();
            for key in object.keys() {
                let mut bad = p.clone();
                bad.pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(!request(&bad));
                let mut bad = p.clone();
                bad.pointer_mut(pointer).unwrap()[key] = Value::Null;
                assert!(!request(&bad));
            }
            let mut bad = p.clone();
            bad.pointer_mut(pointer).unwrap()["extra"] = json!(0);
            assert!(!request(&bad));
            let mut bad = p.clone();
            *bad.pointer_mut(pointer).unwrap() = json!([]);
            assert!(!request(&bad));
        }
    }
    let p = payload(true);
    let mut v = output(true);
    for present in [false, true] {
        v["trackPresent"] = json!(present);
        v["items"] = json!([]);
        assert!(result("document", &p, &v));
    }
    v["items"] = output(true)["items"].clone();
    v["trackPresent"] = json!(false);
    assert!(!result("document", &p, &v));
    v = output(true);
    v["items"][0]["curvePoints"] = Value::Null;
    assert!(result("document", &p, &v));
    v["items"][0]["hold"] = Value::Null;
    assert!(!result("document", &p, &v));
}
#[test]
fn property_read_result_dimensions_order_pagination_and_every_identity_are_verified() {
    for keys in [false, true] {
        let p = payload(keys);
        let v = output(keys);
        for key in [
            "atRevision",
            "documentSnapshotId",
            "kind",
            "target",
            "property",
            "documentId",
        ] {
            let mut bad = v.clone();
            bad[key] = json!("foreign");
            assert!(!result("document", &p, &bad), "{key}");
        }
        for dimension in [0, 5] {
            let mut bad = v.clone();
            bad["dimension"] = json!(dimension);
            assert!(!result("document", &p, &bad));
        }
        let mut bad = v.clone();
        bad["extra"] = json!(0);
        assert!(!result("document", &p, &bad));
        let mut bad = v.clone();
        bad["dimension"] = json!(2);
        assert!(!result("document", &p, &bad));
    }
    let p = payload(false);
    let mut v = output(false);
    v["frame"] = json!(0);
    assert!(!result("document", &p, &v));
    v = output(false);
    v["value"] = json!(["17"]);
    assert!(!result("document", &p, &v));
    let p = payload(true);
    let v = output(true);
    for (pointer, value) in [
        ("/items/0/extra", json!(0)),
        ("/items/0/v", json!([])),
        ("/items/0/frame", json!(4)),
        ("/items/1/curvePoints", json!([{"x":0,"y":1,"extra":2}])),
        ("/items/1/curvePoints", json!([{"x":0,"y":1,"tx":null}])),
        (
            "/items/1/curvePoints",
            json!([{"x":0,"y":0},{"x":0,"y":1},{"x":1,"y":1}]),
        ),
        (
            "/items/1/curvePoints",
            json!([{"x":0.1,"y":0},{"x":1,"y":1}]),
        ),
        (
            "/items/1/curvePoints",
            json!([{"x":0,"y":0},{"x":0.9,"y":1}]),
        ),
        ("/items/1/hIn", json!([0])),
        ("/items/1/hIn", Value::Null),
        ("/trackPresent", Value::Null),
        ("/nextOffset", json!(0)),
        ("/nextOffset", json!(4)),
    ] {
        let mut bad = v.clone();
        if let Some(x) = bad.pointer_mut(pointer) {
            *x = value;
        } else {
            bad["items"][0]["extra"] = value;
        }
        assert!(!result("document", &p, &bad), "{pointer}");
    }
    let mut paged = v.clone();
    paged["nextOffset"] = json!(3);
    let mut full = p.clone();
    full["view"]["limit"] = json!(3);
    assert!(result("document", &full, &paged));
    assert!(
        !result("document", &p, &paged),
        "short nonterminal pages cannot silently shrink the requested page"
    );
    let mut p2 = p.clone();
    p2["view"]["limit"] = json!(2);
    assert!(!result("document", &p2, &v));
    let mut p2 = full;
    p2["view"]["offset"] = json!(u32::MAX);
    assert!(!result("document", &p2, &paged));
    let mut p2 = p.clone();
    p2["view"]["property"] = json!("x".repeat(3072));
    let mut huge = v;
    huge["property"] = p2["view"]["property"].clone();
    assert!(!result("document", &p2, &huge));
}

#[test]
fn property_read_admitted_short_curves_and_explicit_tangents_are_preserved() {
    let p = payload(true);
    let mut v = output(true);
    for points in [
        json!([]),
        json!([{"x":2,"y":-1}]),
        json!([{"x":0,"y":0,"tx":0,"ty":1},{"x":1,"y":1}]),
    ] {
        v["items"][0]["curvePoints"] = points;
        assert!(result("document", &p, &v));
    }
    for (name, value) in [
        ("offset", json!(-1)),
        ("offset", json!(u32::MAX as u64 + 1)),
        ("limit", json!(0)),
        ("limit", json!(9)),
        ("limit", json!(1.5)),
    ] {
        let mut q = p.clone();
        q["view"][name] = value;
        assert!(!request(&q));
    }
    let mut q = payload(false);
    q["view"]["frame"] = json!("2");
    assert!(!request(&q));
}

#[test]
fn exact_authored_key_lock_metadata_is_nullable_and_kept_separate_from_original_curve_oracles() {
    let q = payload(true);
    for lock in [Value::Null, json!("in"), json!("out"), json!("layer")] {
        let mut value = output(true);
        value["items"][0]["lockTo"] = lock;
        assert!(result("document", &q, &value));
    }
    for lock in [
        json!("unknown"),
        json!(true),
        json!(1),
        json!({}),
        json!([]),
    ] {
        let mut value = output(true);
        value["items"][0]["lockTo"] = lock;
        assert!(!result("document", &q, &value));
    }
}

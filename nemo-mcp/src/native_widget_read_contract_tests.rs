//! Literal protocol controls for the widget view of the existing query.document.object operation.
use super::{request, result};
use serde_json::{json, Value};

const OP: &str = "query.document.object";

fn payload() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"widget",
        "contextId":"scene-root","frame":5,"workId":"native-work:9","viewGeneration":3,"cursor":0,"limit":2}})
}
fn axis(value: Value) -> Value {
    json!({"key":"xc_1","min":-100,"max":100,"rest":0,"label":"X","value":value})
}
fn stick() -> Value {
    json!({"layerUid":"L1","visible":true,"locked":false,"kind":"joystick","x":axis(json!(50.5)),"y":axis(Value::Null),
        "center":[960,540],"angle":30,"size":[200,200],"puck":[50,-100],"rest":[0,0],"anchor":[960,540],"linear":[1,0,0,1],"unavailable":null})
}
fn fader() -> Value {
    json!({"layerUid":"L2","visible":false,"locked":true,"kind":"slider","x":axis(json!(0)),"y":null,
        "center":[1,2],"angle":-0.5,"size":[200,26],"puck":[0,0],"rest":[0,0],"anchor":[1,2],"linear":[0,1,-1,0],"unavailable":null})
}
fn response(items: Vec<Value>, total: u64, next: Value) -> Value {
    json!({"kind":"widget","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root",
        "frame":5,"workId":"native-work:9","viewGeneration":3,"cursor":0,"total":total,"nextCursor":next,"items":items})
}
fn changed(value: &Value, pointer: &str, replacement: Value) -> Value {
    let mut value = value.clone();
    *value.pointer_mut(pointer).unwrap() = replacement;
    value
}

#[test]
fn a_page_of_widgets_correlates_with_its_request() {
    let (p, r) = (payload(), response(vec![stick(), fader()], 3, json!(2)));
    assert!(request(&p) && result("document", &p, &r));
    assert!(crate::native_contract::validate_request(OP, &p));
    assert!(crate::native_contract::validate_result(
        OP, "document", &p, &r
    ));
    assert!(result("document", &p, &response(vec![], 0, Value::Null)));
    assert!(!result("replacement", &p, &r));
}

#[test]
fn an_unavailable_item_names_its_reason_and_carries_no_geometry() {
    let p = payload();
    let mut lost = stick();
    for member in ["center", "angle", "size", "puck", "rest", "anchor", "linear"] {
        lost[member] = Value::Null;
    }
    for reason in [
        "expression",
        "sample",
        "parent",
        "parent_geometry",
        "canvas",
    ] {
        lost["unavailable"] = json!(reason);
        assert!(
            result(
                "document",
                &p,
                &response(vec![lost.clone()], 1, Value::Null)
            ),
            "{reason}"
        );
    }
    lost["unavailable"] = json!("guess");
    assert!(!result(
        "document",
        &p,
        &response(vec![lost.clone()], 1, Value::Null)
    ));
    // A missing control keeps the pad (centre, size, rest) and drops only the puck.
    let mut control = stick();
    control["puck"] = Value::Null;
    control["unavailable"] = json!("control");
    assert!(result(
        "document",
        &p,
        &response(vec![control], 1, Value::Null)
    ));
    let mut half = stick();
    half["unavailable"] = json!("expression");
    assert!(
        !result("document", &p, &response(vec![half], 1, Value::Null)),
        "a reason with geometry"
    );
    assert!(
        !result(
            "document",
            &p,
            &response(
                vec![changed(&stick(), "/puck", Value::Null)],
                1,
                Value::Null
            )
        ),
        "no reason and no puck"
    );
}

#[test]
fn the_request_is_closed_and_bounded() {
    let p = payload();
    for (pointer, value) in [
        ("/view/kind", json!("widgets")),
        ("/view/contextId", json!("component:x")),
        ("/view/limit", json!(0)),
        ("/view/limit", json!(5)),
        ("/view/cursor", json!(1025)),
        ("/view/frame", json!(-1)),
        ("/view/viewGeneration", json!(0)),
        ("/view/workId", json!("")),
        ("/documentSnapshotId", json!("")),
    ] {
        assert!(
            !request(&changed(&p, pointer, value.clone())),
            "{pointer} {value}"
        );
    }
    for member in ["layerUid", "page"] {
        let mut extra = p.clone();
        extra["view"][member] = json!("x");
        assert!(!request(&extra), "{member}");
    }
    let mut missing = p.clone();
    missing["view"].as_object_mut().unwrap().remove("limit");
    assert!(!request(&missing));
}

#[test]
fn pagination_and_item_shape_are_checked_against_the_request() {
    let p = payload();
    let good = response(vec![stick(), fader()], 3, json!(2));
    for (pointer, value) in [
        ("/nextCursor", Value::Null),
        ("/nextCursor", json!(1)),
        ("/total", json!(1)),
        ("/cursor", json!(1)),
        ("/atRevision", json!(8)),
        ("/frame", json!(6)),
        ("/workId", json!("native-work:8")),
        ("/viewGeneration", json!(4)),
        ("/kind", json!("subselect")),
        ("/documentSnapshotId", json!("native-object:document:6")),
    ] {
        assert!(
            !result("document", &p, &changed(&good, pointer, value.clone())),
            "{pointer} {value}"
        );
    }
    let three = response(vec![stick(), fader(), stick()], 3, Value::Null);
    assert!(!result("document", &p, &three), "more items than the limit");
    for (pointer, value) in [
        ("/0/kind", json!("dial")),
        ("/0/x/min", json!("a")),
        ("/0/x/value", json!("1")),
        ("/0/y", Value::Null),
        ("/1/y", axis(json!(0))),
        ("/0/center", json!([1])),
        ("/0/linear", json!([1, 0, 0])),
        ("/0/linear", Value::Null),
        ("/0/anchor", Value::Null),
        ("/0/locked", json!("no")),
        ("/0/layerUid", json!("")),
        ("/0/extra", json!(1)),
    ] {
        let mut items = json!([stick(), fader()]);
        let slot = items.pointer_mut(pointer).map(|slot| *slot = value.clone());
        if slot.is_none() {
            let (parent, member) = pointer.rsplit_once('/').unwrap();
            items.pointer_mut(parent).unwrap()[member] = value.clone();
        }
        let mut answer = good.clone();
        answer["items"] = items;
        assert!(!result("document", &p, &answer), "{pointer} {value}");
    }
}

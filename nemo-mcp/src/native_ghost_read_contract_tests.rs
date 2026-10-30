//! Literal protocol controls for the ghosts view of the existing query.document.object operation.
use super::{request, result};
use serde_json::{json, Value};

const OP: &str = "query.document.object";

fn payload() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"ghosts",
        "contextId":"scene-root","frame":5,"workId":"native-work:9","viewGeneration":3,"layerUid":"L1","onion":true,
        "ghostAll":false,"from":2,"to":8,"previousOpacity":30,"nextOpacity":30.5,"mode":"tinted","ghost":1,"cursor":0}})
}
fn ghost() -> Value {
    json!({"index":1,"count":3,"frame":4,"kind":"previous","opacity":0.3,"translation":[0,0]})
}
fn piece(from: u64, count: u64, len: usize) -> Value {
    let segments: Vec<Value> = (0..len)
        .map(|n| {
            if n % 2 == 0 {
                json!([1.5, 2])
            } else {
                json!([1, 2, 0, 0, 3.25, -1])
            }
        })
        .collect();
    json!({"path":0,"from":from,"count":count,"sourceId":"s1","closed":false,"width":2,"fill":null,"stroke":"#ff4d4dff",
        "opacity":0.3,"segments":segments})
}
fn response(items: Vec<Value>, total: u64, next: Value) -> Value {
    json!({"kind":"ghosts","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root",
        "frame":5,"workId":"native-work:9","viewGeneration":3,"layerUid":"L1","ghost":ghost(),"cursor":0,"total":total,
        "next":next,"items":items})
}
fn changed(value: &Value, pointer: &str, replacement: Value) -> Value {
    let mut value = value.clone();
    *value.pointer_mut(pointer).unwrap() = replacement;
    value
}

#[test]
fn a_page_of_ghost_geometry_correlates_with_its_request() {
    let p = payload();
    let r = response(vec![piece(0, 5, 3)], 5, json!({"ghost":1,"cursor":3}));
    assert!(request(&p) && result("document", &p, &r));
    assert!(crate::native_contract::validate_request(OP, &p));
    assert!(crate::native_contract::validate_result(
        OP, "document", &p, &r
    ));
    assert!(!result("replacement", &p, &r));
    // The last page of a ghost frame names the next ghost frame; the last frame names none.
    let done = response(vec![piece(0, 3, 3)], 3, json!({"ghost":2,"cursor":0}));
    assert!(result("document", &p, &done));
    let mut last = payload();
    last["view"]["ghost"] = json!(2);
    let mut tail = response(vec![piece(0, 3, 3)], 3, Value::Null);
    tail["ghost"]["index"] = json!(2);
    assert!(result("document", &last, &tail));
    // No ghost frame at all is one empty page.
    let mut none = payload();
    none["view"]["ghost"] = json!(0);
    let mut empty = response(vec![], 0, Value::Null);
    empty["ghost"] = Value::Null;
    assert!(result("document", &none, &empty));
}

#[test]
fn a_wrong_continuation_or_overrun_is_refused() {
    let p = payload();
    assert!(
        !result(
            "document",
            &p,
            &response(vec![piece(0, 5, 3)], 5, json!({"ghost":1,"cursor":2}))
        ),
        "cursor skips"
    );
    assert!(
        !result(
            "document",
            &p,
            &response(vec![piece(0, 5, 3)], 5, Value::Null)
        ),
        "unfinished frame without a next page"
    );
    assert!(
        !result(
            "document",
            &p,
            &response(vec![piece(0, 3, 3)], 3, Value::Null)
        ),
        "a following ghost frame is not named"
    );
    assert!(
        !result(
            "document",
            &p,
            &response(vec![piece(0, 5, 6)], 5, Value::Null)
        ),
        "a piece beyond its path"
    );
    assert!(
        !result(
            "document",
            &p,
            &response(vec![piece(0, 5, 3)], 2, json!({"ghost":2,"cursor":0}))
        ),
        "more than the total"
    );
    for (pointer, value) in [
        ("/frame", json!(6)),
        ("/workId", json!("native-work:1")),
        ("/viewGeneration", json!(4)),
        ("/layerUid", json!("L2")),
        ("/atRevision", json!(8)),
        ("/documentSnapshotId", json!("native-object:document:8")),
    ] {
        let r = changed(
            &response(vec![piece(0, 3, 3)], 3, json!({"ghost":2,"cursor":0})),
            pointer,
            value,
        );
        assert!(!result("document", &p, &r), "{pointer}");
    }
    assert!(!result(
        "document",
        &p,
        &changed(
            &response(vec![piece(0, 3, 3)], 3, json!({"ghost":2,"cursor":0})),
            "/ghost/index",
            json!(0)
        )
    ));
}

#[test]
fn items_are_closed_shapes() {
    let p = payload();
    let page = |item: Value| response(vec![item], 3, json!({"ghost":2,"cursor":0}));
    assert!(result("document", &p, &page(piece(0, 3, 3))));
    for (pointer, value) in [
        ("/fill", json!("#fff")),
        ("/stroke", json!("red")),
        ("/opacity", json!(2)),
        ("/width", json!("2")),
        ("/closed", json!(1)),
        ("/segments/0", json!([1, 2, 3])),
        ("/segments/1", json!(["a", "b"])),
    ] {
        assert!(
            !result(
                "document",
                &p,
                &page(changed(&piece(0, 3, 3), pointer, value))
            ),
            "{pointer}"
        );
    }
    let mut extra = piece(0, 3, 3);
    extra["extra"] = json!(1);
    assert!(!result("document", &p, &page(extra)));
}

#[test]
fn the_request_is_closed_and_bounded() {
    let p = payload();
    let mut tinted = payload();
    tinted["view"]["previousTint"] = json!("#ff4d4d");
    tinted["view"]["nextTint"] = json!("#4d8cff");
    tinted["view"]["shift"] = json!({"x":2,"y":-3.5,"next":true});
    assert!(request(&tinted));
    for (pointer, value) in [
        ("/view/kind", json!("ghost")),
        ("/view/contextId", json!("other")),
        ("/view/frame", json!(-1)),
        ("/view/from", json!(4294967296u64)),
        ("/view/workId", json!("")),
        ("/view/viewGeneration", json!(0)),
        ("/view/layerUid", json!("")),
        ("/view/onion", json!(1)),
        ("/view/previousOpacity", json!(101)),
        ("/view/nextOpacity", json!(-1)),
        ("/view/mode", json!("outlines")),
        ("/view/ghost", json!(4096)),
        ("/view/cursor", json!(262145)),
        ("/view/previousTint", json!("red")),
        ("/view/shift", json!({"x":1,"y":2})),
    ] {
        let mut bad = tinted.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(!request(&bad), "{pointer}");
    }
    let mut extra = p.clone();
    extra["view"]["limit"] = json!(2);
    assert!(!request(&extra));
    let mut missing = p.clone();
    missing["view"].as_object_mut().unwrap().remove("cursor");
    assert!(!request(&missing));
}

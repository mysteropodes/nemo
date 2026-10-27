use super::{request, result};
use serde_json::{json, Value};

fn payload() -> Value {
    let point =
        |x: f64| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"documentSnapshotId":"native-object:document-A:3","lifecycleGeneration":1,
        "record":{"schemaVersion":1,"family":"editable-vector-path",
            "target":{"contextId":"scene-root","layerUid":"L","strokeId":"","frameScope":{"kind":"authored","frame":0}},
            "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[point(0.0), point(10.0)]},
            "fill":null,"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":8,"strokeCap":"round","strokeJoin":"round",
                "miterLimit":10,"dashOffset":0,"opacity":0},
            "vectorExt":{"bitmapBrush":{"bitmapBrushSpec":{"tip":"soft","size":8},"brushGroupId":"bmb_1","preTextureOpacity":1}}},
        "companion":{"x":5,"y":0,"width":24,"height":14,"opacity":1},
        "source":{"kind":"file","path":"/tmp/nemo-native-import-1/source-0001.png"}})
}

#[test]
fn the_stroke_request_is_admitted_as_the_owner_reads_it_and_nothing_else() {
    assert!(request(&payload()));
    let mut cases: Vec<(&str, Value)> = vec![
        ("/record/target/strokeId", json!("named")),
        ("/record/target/contextId", json!("component-1")),
        ("/record/target/frameScope/kind", json!("reference")),
        ("/record/family", json!("closed-solid-fill-cubic-path")),
        ("/record/geometry/segments", json!([])),
        ("/record/vectorExt/bitmapBrush/brushGroupId", json!("")),
        ("/record/vectorExt/bitmapBrush/bitmapBrushSpec", json!(null)),
        ("/companion/width", json!(0)),
        ("/companion/opacity", json!(1.5)),
        ("/companion/x", json!("5")),
        ("/source/kind", json!("bytes")),
        ("/source/path", json!("")),
        ("/lifecycleGeneration", json!(0)),
        ("/documentSnapshotId", json!("")),
    ];
    cases.push(("/companion/extra", json!(1)));
    for (at, bad) in cases {
        let mut p = payload();
        let (parent, key) = at.rsplit_once('/').unwrap();
        p.pointer_mut(parent).unwrap()[key] = bad;
        assert!(!request(&p), "{at}");
    }
    let mut extra = payload();
    extra["placement"] = json!({"kind":"newLayer"});
    assert!(!request(&extra), "no other placement");
    extra["placement"] = json!("front");
    assert!(!request(&extra), "front is the absent default");
    extra["placement"] = json!("behind");
    assert!(request(&extra), "drawMode behind");
    extra["other"] = json!(1);
    assert!(!request(&extra), "no other member");
}

#[test]
fn the_disposition_is_one_history_entry_or_none() {
    assert!(result(&json!({"applied":true,"historyEntriesAdded":1})));
    assert!(result(&json!({"applied":false,"historyEntriesAdded":0})));
    for bad in [
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":true}),
        json!({"applied":true,"historyEntriesAdded":1,"x":1}),
    ] {
        assert!(!result(&bad), "{bad}");
    }
}

#[test]
fn the_shared_contract_dispatches_both_halves_to_this_module() {
    use crate::native_contract::{
        has_request_validator, has_result_validator, validate_request, validate_result,
    };
    assert!(has_request_validator(super::OP) && has_result_validator(super::OP));
    assert!(validate_request(super::OP, &payload()));
    let applied = json!({"applied":true,"historyEntriesAdded":1});
    assert!(validate_result(super::OP, "document-A", &payload(), &applied));
    assert!(!validate_result(
        super::OP,
        "document-A",
        &payload(),
        &json!({"applied":true,"historyEntriesAdded":0})
    ));
}

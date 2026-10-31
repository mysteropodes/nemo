//! Literal protocol controls for the rig view of the existing query.document.object operation.
use super::{request, result};
use serde_json::{json, Value};

const OP: &str = "query.document.object";

fn payload(part: &str, bind: u64, vertex: u64, limit: u64) -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"rig",
        "contextId":"scene-root","frame":5,"workId":"native-work:9","viewGeneration":3,"layerUid":"L1","part":part,
        "bind":bind,"vertex":vertex,"cursor":0,"limit":limit}})
}
fn response(
    part: &str,
    bind: u64,
    vertex: u64,
    items: Vec<Value>,
    total: u64,
    next: Value,
    reason: Value,
) -> Value {
    json!({"kind":"rig","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root",
        "frame":5,"workId":"native-work:9","viewGeneration":3,"layerUid":"L1","part":part,"bind":bind,"vertex":vertex,
        "cursor":0,"total":total,"nextCursor":next,"unavailable":reason,"items":items})
}
fn good() -> Vec<(&'static str, u64, Value)> {
    vec![
        (
            "bones",
            0,
            json!({"boneId":"bone1","closed":false,"radius":null,"count":2,"center":[10,5.5]}),
        ),
        (
            "bones",
            0,
            json!({"boneId":"bone2","closed":true,"radius":30,"count":3,"center":[0,0]}),
        ),
        (
            "vertices",
            0,
            json!({"boneId":"bone1","vi":0,"point":[0,10],"handleIn":[0,0],"handleOut":[5,0]}),
        ),
        (
            "chains",
            0,
            json!({"endKey":"bone2:1","root":{"boneId":"bone1","vi":0},"joint":{"boneId":"bone1","vi":1},
            "end":{"boneId":"bone2","vi":1},"l1":10,"l2":12.5,"flip":false}),
        ),
        (
            "binds",
            0,
            json!({"index":0,"strokeId":"s1","meshId":null,"points":3,"rotate":false,"state":"posed"}),
        ),
        (
            "binds",
            0,
            json!({"index":1,"strokeId":null,"meshId":"im_1","points":4,"rotate":true,"state":"unavailable"}),
        ),
        ("bound", 0, json!([0, 2, 20.5, -3])),
        (
            "weights",
            1,
            json!({"entry":0,"boneId":"bone1","ptIndex":2,"offset":null,"w":0.75}),
        ),
        (
            "weights",
            1,
            json!({"entry":1,"boneId":"bone1","ptIndex":null,"offset":12.5,"w":0.25}),
        ),
    ]
}

#[test]
fn every_part_correlates_with_its_request() {
    for (part, bind, item) in good() {
        // A weights request names the bind and the vertex (here 1 and 2); every other part names neither.
        let (bind, vertex) = if part == "weights" { (bind, 2) } else { (0, 0) };
        let p = payload(part, bind, vertex, 8);
        let r = response(
            part,
            bind,
            vertex,
            vec![item.clone()],
            1,
            Value::Null,
            Value::Null,
        );
        assert!(request(&p) && result("document", &p, &r), "{part} {item}");
        assert!(crate::native_contract::validate_request(OP, &p), "{part}");
        assert!(
            crate::native_contract::validate_result(OP, "document", &p, &r),
            "{part}"
        );
        assert!(!result("replacement", &p, &r), "{part}: another document");
    }
}

#[test]
fn an_unavailable_layer_names_its_reason_and_lists_nothing() {
    let p = payload("bones", 0, 0, 8);
    for reason in ["expression", "sample", "parent", "motion"] {
        assert!(
            result(
                "document",
                &p,
                &response("bones", 0, 0, vec![], 0, Value::Null, json!(reason))
            ),
            "{reason}"
        );
    }
    assert!(!result(
        "document",
        &p,
        &response("bones", 0, 0, vec![], 0, Value::Null, json!("guess"))
    ));
    let item = good()[0].2.clone();
    assert!(
        !result(
            "document",
            &p,
            &response("bones", 0, 0, vec![item], 1, Value::Null, json!("motion"))
        ),
        "a reason with geometry"
    );
}

#[test]
fn the_request_is_closed_and_bounded_per_part() {
    for (part, limit, ok) in [
        ("bones", 8, true),
        ("bones", 9, false),
        ("vertices", 16, true),
        ("vertices", 17, false),
        ("bound", 64, true),
        ("bound", 65, false),
        ("weights", 8, true),
        ("weights", 9, false),
        ("shapes", 1, false),
        ("bones", 0, false),
    ] {
        assert_eq!(request(&payload(part, 0, 0, limit)), ok, "{part} {limit}");
    }
    // `bind` and `vertex` select a weights vertex and are zero for every other part.
    assert!(request(&payload("weights", 4, 9, 8)));
    for part in ["bones", "vertices", "chains", "binds", "bound"] {
        assert!(
            !request(&payload(part, 1, 0, 1)) && !request(&payload(part, 0, 1, 1)),
            "{part}"
        );
    }
    let p = payload("bones", 0, 0, 8);
    for (pointer, value) in [
        ("/view/kind", json!("rigs")),
        ("/view/contextId", json!("component:x")),
        ("/view/cursor", json!(100001)),
        ("/view/frame", json!(-1)),
        ("/view/viewGeneration", json!(0)),
        ("/view/workId", json!("")),
        ("/view/layerUid", json!("")),
        ("/view/bind", json!(4294967296u64)),
        ("/documentSnapshotId", json!("")),
    ] {
        let mut bad = p.clone();
        *bad.pointer_mut(pointer).unwrap() = value.clone();
        assert!(!request(&bad), "{pointer} {value}");
    }
    for member in ["page", "extra"] {
        let mut extra = p.clone();
        extra["view"][member] = json!("x");
        assert!(!request(&extra), "{member}");
    }
    let mut missing = p.clone();
    missing["view"].as_object_mut().unwrap().remove("layerUid");
    assert!(!request(&missing));
    // The payload itself must be the closed three-member object.
    let mut unpinned = p.clone();
    unpinned.as_object_mut().unwrap().remove("documentSnapshotId");
    for bad in [Value::Null, json!([]), json!({}), unpinned] {
        assert!(!request(&bad), "{bad}");
    }
}

#[test]
fn pagination_and_item_shape_are_checked_against_the_request() {
    let p = payload("bones", 0, 0, 2);
    // A malformed answer envelope, or one whose page numbers are not integers, is refused before any pin is compared.
    assert!(!result("document", &p, &Value::Null));
    assert!(!result("document", &p, &json!({"kind":"rig"})));
    let mut untotaled = response("bones", 0, 0, vec![], 0, Value::Null, Value::Null);
    untotaled["total"] = json!("none");
    assert!(!result("document", &p, &untotaled));
    let mut unpaged = p.clone();
    unpaged["view"]["cursor"] = json!("first");
    assert!(!result("document", &unpaged, &response("bones", 0, 0, vec![], 0, Value::Null, Value::Null)));
    // An item of a part the contract does not list is never well-formed.
    let mut other = p.clone();
    other["view"]["part"] = json!("nothing");
    let mut listed = response("nothing", 0, 0, vec![json!({})], 1, Value::Null, Value::Null);
    listed["part"] = json!("nothing");
    assert!(!result("document", &other, &listed));
    let (a, b) = (good()[0].2.clone(), good()[1].2.clone());
    let page = response(
        "bones",
        0,
        0,
        vec![a.clone(), b.clone()],
        3,
        json!(2),
        Value::Null,
    );
    assert!(result("document", &p, &page));
    for (pointer, value) in [
        ("/nextCursor", Value::Null),
        ("/nextCursor", json!(1)),
        ("/total", json!(1)),
        ("/cursor", json!(1)),
        ("/atRevision", json!(8)),
        ("/frame", json!(6)),
        ("/layerUid", json!("L2")),
        ("/part", json!("vertices")),
        ("/bind", json!(1)),
        ("/workId", json!("native-work:8")),
        ("/viewGeneration", json!(4)),
        ("/kind", json!("widget")),
        ("/documentSnapshotId", json!("native-object:document:6")),
    ] {
        let mut bad = page.clone();
        *bad.pointer_mut(pointer).unwrap() = value.clone();
        assert!(!result("document", &p, &bad), "{pointer} {value}");
    }
    assert!(
        !result(
            "document",
            &p,
            &response(
                "bones",
                0,
                0,
                vec![a.clone(), b.clone(), a.clone()],
                3,
                Value::Null,
                Value::Null
            )
        ),
        "more items than the limit"
    );
    for (pointer, value) in [
        ("/count", json!(1)),
        ("/closed", json!("no")),
        ("/center", json!([1])),
        ("/boneId", json!("")),
        ("/radius", json!("x")),
        ("/extra", json!(1)),
    ] {
        let mut bad = a.clone();
        match bad.pointer_mut(pointer) {
            Some(slot) => *slot = value.clone(),
            None => bad[pointer.trim_start_matches('/')] = value.clone(),
        }
        assert!(
            !result(
                "document",
                &p,
                &response("bones", 0, 0, vec![bad], 1, Value::Null, Value::Null)
            ),
            "{pointer} {value}"
        );
    }
    // An item of another part is refused; a bind names exactly one target, a weight exactly one of ptIndex and offset.
    let vertex = good()[2].2.clone();
    assert!(!result(
        "document",
        &p,
        &response("bones", 0, 0, vec![vertex], 1, Value::Null, Value::Null)
    ));
    let q = payload("binds", 0, 0, 8);
    let mut both = good()[4].2.clone();
    both["meshId"] = json!("im_1");
    assert!(!result(
        "document",
        &q,
        &response("binds", 0, 0, vec![both], 1, Value::Null, Value::Null)
    ));
    let q = payload("weights", 0, 0, 8);
    let mut neither = good()[7].2.clone();
    neither["ptIndex"] = Value::Null;
    assert!(!result(
        "document",
        &q,
        &response("weights", 0, 0, vec![neither], 1, Value::Null, Value::Null)
    ));
}

fn transport_schema() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let mut registry = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let resource: Value = serde_json::from_str(text).unwrap();
        let id = resource["$id"].as_str().unwrap().to_owned();
        registry = registry
            .add(id, jsonschema::Resource::from_contents(resource))
            .unwrap();
    }
    jsonschema::draft202012::options()
        .with_registry(&registry.prepare().unwrap())
        .build(&schema)
        .unwrap()
}
fn wrapped(body: Value, response: bool) -> Value {
    if response {
        json!({"apiVersion":2,"requestId":"rig-schema","instanceId":"instance","documentId":"document","contentRevision":7,"ok":true,"result":body})
    } else {
        json!({"apiVersion":2,"requestId":"rig-schema","instanceId":"instance","documentId":"document","operation":OP,"payload":body})
    }
}

#[test]
fn the_complete_transport_schema_admits_every_part_and_refuses_what_the_contract_refuses() {
    let schema = transport_schema();
    for (part, bind, item) in good() {
        let (bind, vertex) = if part == "weights" { (bind, 2) } else { (0, 0) };
        let (p, r) = (
            payload(part, bind, vertex, 8),
            response(
                part,
                bind,
                vertex,
                vec![item.clone()],
                1,
                Value::Null,
                Value::Null,
            ),
        );
        assert!(
            schema.is_valid(&wrapped(p.clone(), false)),
            "{part} request"
        );
        assert!(
            schema.is_valid(&wrapped(r.clone(), true)),
            "{part} response {item}"
        );
        let mut wrong = r.clone();
        wrong["items"] = json!([good()[2].2.clone()]);
        assert_eq!(
            schema.is_valid(&wrapped(wrong, true)),
            part == "vertices",
            "{part}: an item of another part"
        );
    }
    for (part, limit, ok) in [
        ("bones", 9, false),
        ("vertices", 17, false),
        ("bound", 64, true),
        ("weights", 9, false),
    ] {
        assert_eq!(
            schema.is_valid(&wrapped(payload(part, 0, 0, limit), false)),
            ok,
            "{part} {limit}"
        );
    }
    assert!(
        !schema.is_valid(&wrapped(payload("bones", 1, 0, 8), false)),
        "a selector on a part that takes none"
    );
    let lost = response("bones", 0, 0, vec![], 0, Value::Null, json!("motion"));
    assert!(schema.is_valid(&wrapped(lost, true)));
    let mut guess = response(
        "bones",
        0,
        0,
        vec![good()[0].2.clone()],
        1,
        Value::Null,
        json!("motion"),
    );
    assert!(
        !schema.is_valid(&wrapped(guess.clone(), true)),
        "a reason with geometry"
    );
    guess["unavailable"] = json!("guess");
    assert!(!schema.is_valid(&wrapped(guess, true)));
}

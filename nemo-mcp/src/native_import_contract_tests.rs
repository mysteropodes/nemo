use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

fn record(layer: &str, x: i64) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":""},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative","segments":[
            {"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":x + 10,"y":5},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,
            "strokeWidth":2,"strokeCap":"butt","strokeJoin":"miter","opacity":1}})
}

fn valid() -> Value {
    json!({"command":"object.import.batch",
        "layers":[{"sourceId":"w","name":"Wire"}],
        "records":[{"sourceId":"w:0","record":record("w", 1)},{"sourceId":"w:1","record":record("w", 2)},
            {"sourceId":"e:0","record":record("L0", 3)}],
        "skipped":[{"kind":"element","sourceId":"svg:9","reason":"unsupported-element:Raster"}],
        "unavailable":[]})
}

fn edited(edit: impl Fn(&mut Value)) -> Value {
    let mut payload = valid();
    edit(&mut payload);
    payload
}

fn envelope(payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"import-1","instanceId":"instance-A",
        "documentId":"document-A","expectedRevision":7,"operation":OP,"payload":payload}),
    )
    .unwrap()
}

fn response(value: Value) -> NativeApplicationResponse {
    serde_json::from_value(value).unwrap()
}

fn receipt() -> Value {
    json!({"applied":true,"historyEntriesAdded":1,
        "layers":[{"sourceId":"w","layerUid":"native-layer:8:0"}],
        "objects":[{"sourceId":"w:0","strokeId":"native-stroke:8:0"},
            {"sourceId":"w:1","strokeId":"native-stroke:8:1"},
            {"sourceId":"e:0","strokeId":"native-stroke:8:2"}],
        "skipped":[{"kind":"element","sourceId":"svg:9","reason":"unsupported-element:Raster"}]})
}

fn many_anchors(n: usize) -> Value {
    json!((0..n)
        .map(|i| json!({"point":{"x":i,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}))
        .collect::<Vec<_>>())
}

#[test]
fn only_the_import_operation_is_admitted_and_it_needs_a_revision() {
    assert!(admitted(OP));
    for other in [
        "command.document.object.layer.create",
        "command.document.object.import",
        "command.document.object.import.batch.extra",
        "query.document.object",
        "",
    ] {
        assert!(!admitted(other), "{other}");
    }
    assert!(crate::native_contract::validate_request(OP, &valid()));
    let mut command = envelope(valid());
    assert!(command.validate_input().is_ok());
    assert!(command.validate().is_ok());
    command.expected_revision = None;
    assert!(command.validate_input().is_err(), "needs expectedRevision");
    command.expected_revision = Some(9_007_199_254_740_992);
    assert!(command.validate_input().is_err(), "unsafe revision");
}

#[test]
fn the_payload_is_closed_member_by_member() {
    assert!(request(&valid()));
    assert!(request(&edited(|p| {
        p.as_object_mut().unwrap().remove("skipped");
        p.as_object_mut().unwrap().remove("unavailable");
    })));
    for key in ["command", "layers", "records"] {
        assert!(
            !request(&edited(|p| {
                p.as_object_mut().unwrap().remove(key);
            })),
            "{key}"
        );
        assert!(!request(&edited(|p| p[key] = Value::Null)), "{key} null");
    }
    for (key, value) in [
        ("extra", json!(1)),
        ("command", json!("object.fill.set")),
        ("layers", json!({})),
        ("skipped", json!("x")),
        ("unavailable", json!(null)),
    ] {
        assert!(!request(&edited(|p| p[key] = value.clone())), "{key}");
    }
    assert!(!request(&json!([])));
    assert!(!request(&Value::Null));
    assert!(!request(&edited(
        |p| p["layers"][0]["color"] = json!("#fff")
    )));
    assert!(!request(&edited(|p| p["layers"][0]["name"] = json!(""))));
    assert!(!request(&edited(
        |p| p["records"][0]["layerUid"] = json!("x")
    )));
    assert!(!request(&edited(|p| p["skipped"][0]["extra"] = json!(1))));
}

#[test]
fn source_ids_follow_the_owner_id_table() {
    for bad in [
        json!(""),
        json!(" "),
        json!("\t\n"),
        json!("a".repeat(256)),
        json!("a\u{0}b"),
        json!(7),
        Value::Null,
    ] {
        assert!(
            !request(&edited(|p| p["layers"][0]["sourceId"] = bad.clone())),
            "{bad}"
        );
    }
    assert!(request(&edited(
        |p| p["layers"][0]["sourceId"] = json!("a".repeat(255))
    )));
    assert!(request(&edited(
        |p| p["layers"][0]["sourceId"] = json!("é".repeat(127))
    )));
    assert!(
        !request(&edited(
            |p| p["layers"][0]["sourceId"] = json!("é".repeat(128))
        )),
        "255 BYTES, not characters"
    );
    assert!(!request(&edited(|p| p["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sourceId":"w","name":"Again"})))));
    assert!(!request(&edited(
        |p| p["records"][1]["sourceId"] = json!("w:0")
    )));
    assert!(
        !request(&edited(|p| p["skipped"][0]["sourceId"] = json!("w:0"))),
        "a record and an element skip are both strokes"
    );
    assert!(
        request(&edited(|p| p["skipped"].as_array_mut().unwrap().push(
            json!({"kind":"layer","sourceId":"w:0","reason":"hidden"})
        ))),
        "layer skips have their own id space"
    );
    assert!(!request(&edited(|p| p["skipped"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"kind":"asset","sourceId":"a","reason":"x"}),
            json!({"kind":"asset","sourceId":"a","reason":"y"})
        ]))));
}

#[test]
fn skips_and_unavailable_rows_are_typed() {
    for kind in ["layer", "element", "asset"] {
        assert!(
            request(&edited(
                |p| p["skipped"] = json!([{"kind":kind,"sourceId":"s","reason":"why"}])
            )),
            "{kind}"
        );
    }
    assert!(!request(&edited(
        |p| p["skipped"][0]["kind"] = json!("stroke")
    )));
    assert!(!request(&edited(|p| p["skipped"][0]["reason"] = json!(""))));
    for need in ["raster-asset", "text", "font"] {
        assert!(
            request(&edited(
                |p| p["unavailable"] = json!([{"sourceId":"p","need":need}])
            )),
            "{need}"
        );
    }
    assert!(!request(&edited(
        |p| p["unavailable"] = json!([{"sourceId":"p","need":"gpu"}])
    )));
    assert!(!request(&edited(
        |p| p["unavailable"] = json!([{"sourceId":"p","need":"text","extra":1}])
    )));
}

#[test]
fn a_record_is_a_schema_one_vector_path_with_an_empty_stroke_id_and_bounded_anchors() {
    let on = |edit: fn(&mut Value)| request(&edited(|p| edit(&mut p["records"][0]["record"])));
    assert!(
        !on(|r| r["target"]["strokeId"] = json!("native-stroke:1:0")),
        "the owner mints it"
    );
    assert!(!on(
        |r| r["target"]["frameScope"]["kind"] = json!("reference")
    ));
    assert!(!on(
        |r| r["target"]["frameScope"]["frame"] = json!(4_294_967_296_u64)
    ));
    assert!(on(
        |r| r["target"]["frameScope"]["frame"] = json!(4_294_967_295_u64)
    ));
    assert!(!on(|r| r["target"]["layerUid"] = json!("")));
    assert!(!on(|r| r["family"] = json!("closed-solid-fill-cubic-path")));
    assert!(!on(|r| r["schemaVersion"] = json!(2)));
    assert!(
        !on(|r| r["geometry"]["segments"] = many_anchors(1)),
        "one anchor"
    );
    assert!(on(|r| r["geometry"]["segments"] = many_anchors(2)));
    assert!(on(|r| r["geometry"]["segments"] = many_anchors(256)));
    assert!(!on(|r| r["geometry"]["segments"] = many_anchors(257)));
    assert!(!on(|r| r["geometry"]["handleSpace"] = json!("absolute")));
    assert!(!on(
        |r| r["geometry"]["coordinateSpace"] = json!("layer-local")
    ));
    assert!(!on(
        |r| r["geometry"]["segments"][0]["point"]["x"] = Value::Null
    ));
    assert!(!on(|r| r["geometry"]["segments"][0]["extra"] = json!(1)));
    assert!(!on(|r| r["paintOrder"] = json!("strokeFirst")));
}

#[test]
fn paints_and_stroke_members_are_closed() {
    let on = |edit: fn(&mut Value)| request(&edited(|p| edit(&mut p["records"][0]["record"])));
    assert!(on(
        |r| r["fill"] = json!({"kind":"solid","r":0,"g":0.5,"b":1,"a":0.5})
    ));
    assert!(!on(
        |r| r["fill"] = json!({"kind":"gradient","r":0,"g":0,"b":0,"a":1})
    ));
    assert!(!on(
        |r| r["fill"] = json!({"kind":"solid","r":2,"g":0,"b":0,"a":1})
    ));
    assert!(!on(|r| r["vector"]["hasRealStroke"] = json!(false)));
    assert!(!on(|r| r["vector"]["stroke"] = Value::Null));
    assert!(on(|r| {
        r["vector"]["stroke"] = Value::Null;
        r["vector"]["hasRealStroke"] = json!(false);
    }));
    assert!(!on(|r| r["vector"]["strokeCap"] = json!("triangle")));
    assert!(!on(|r| r["vector"]["strokeJoin"] = json!("smooth")));
    assert!(on(|r| {
        r["vector"]["miterLimit"] = json!(4);
        r["vector"]["dashArray"] = json!([4, 2]);
        r["vector"]["dashOffset"] = json!(1.5);
    }));
    assert!(!on(|r| r["vector"]["dashArray"] = json!([4, "a"])));
    assert!(!on(|r| r["vector"]["opacity"] = Value::Null));
    assert!(!on(|r| r["vector"]["hairline"] = json!(true)));
}

#[test]
fn the_receipt_is_compact_closed_and_bounded() {
    assert!(result(&receipt()));
    assert!(result(
        &json!({"applied":false,"historyEntriesAdded":0,"layers":[],"objects":[],"skipped":[]})
    ));
    let mutate = |edit: fn(&mut Value)| {
        let mut r = receipt();
        edit(&mut r);
        result(&r)
    };
    assert!(!mutate(|r| r["historyEntriesAdded"] = json!(0)));
    assert!(!mutate(|r| r["applied"] = json!(false)));
    assert!(!mutate(|r| r["document"] = json!({})));
    assert!(!mutate(|r| {
        r.as_object_mut().unwrap().remove("objects");
    }));
    assert!(!mutate(|r| r["layers"][0]["layerUid"] = json!("")));
    assert!(!mutate(|r| r["objects"][0]["strokeId"] = json!(7)));
    assert!(!mutate(|r| r["skipped"][0]["kind"] = json!("stroke")));
    assert!(
        !mutate(|r| {
            r["objects"] = json!((0..60)
            .map(|i| json!({"sourceId":format!("source-{i}"),"strokeId":format!("native-stroke:8:{i}")}))
            .collect::<Vec<_>>())
        }),
        "over 3072 bytes"
    );
}

#[test]
fn the_revision_advances_by_exactly_the_applied_edit() {
    let changed = receipt();
    let noop =
        json!({"applied":false,"historyEntriesAdded":0,"layers":[],"objects":[],"skipped":[]});
    assert!(revision_matches(Some(7), 8, &changed));
    assert!(revision_matches(Some(7), 7, &noop));
    assert!(!revision_matches(Some(7), 7, &changed));
    assert!(!revision_matches(Some(7), 8, &noop));
    assert!(!revision_matches(None, 8, &changed));
    assert!(!revision_matches(Some(u64::MAX), 0, &changed));
}

#[test]
fn the_response_must_come_from_the_same_owner_and_document_with_a_correlated_receipt() {
    let command = envelope(valid());
    let ok = response(
        json!({"apiVersion":2,"requestId":"import-1","instanceId":"instance-A","documentId":"document-A",
        "contentRevision":8,"ok":true,"result":receipt()}),
    );
    assert!(response_matches(&command, &ok));
    for (key, value) in [
        ("instanceId", json!("instance-B")),
        ("documentId", json!("document-B")),
        ("contentRevision", json!(9)),
    ] {
        let mut bad = serde_json::to_value(&ok).unwrap();
        bad[key] = value;
        assert!(!response_matches(&command, &response(bad)), "{key}");
    }
    let refusal = response(
        json!({"apiVersion":2,"requestId":"import-1","instanceId":"instance-A","documentId":"document-A",
        "contentRevision":7,"ok":false,"error":{"code":"stale_revision","message":"stale"}}),
    );
    assert!(
        !response_matches(&command, &refusal),
        "a success correlation needs a result"
    );
    let mut foreign = command.clone();
    foreign.operation = "command.document.object.layer.visibility.set".into();
    assert!(
        response_matches(&foreign, &refusal),
        "undeclared operations are not judged here"
    );
}

#[test]
fn the_catalog_declares_the_operation_once_and_its_example_is_a_valid_correlated_pair() {
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .expect("declared");
    assert_eq!(descriptor["id"], "native.object.import");
    assert_eq!(descriptor["authority"]["fallback"], "none");
    assert_eq!(descriptor["authority"]["adapterOwnsWritableMirror"], false);
    assert_eq!(descriptor["resourceBoundary"]["maxRequestBytes"], 4096);
    let example = &descriptor["examples"][0];
    let command: NativeApplicationRequest =
        serde_json::from_value(example["request"].clone()).unwrap();
    assert!(command.validate().is_ok());
    let reply: NativeApplicationResponse =
        serde_json::from_value(example["response"].clone()).unwrap();
    assert!(response_matches(&command, &reply));
    assert!(result(reply.result.as_ref().unwrap()));
}

/// The shared parity table (`tools/gen_contract_cases.mjs`): the JS contract judges the same rows in
/// `tests/native-import-contract.test.cjs`. The `valid` flags are authored from the owner rules, not computed.
const PARITY: &str =
    include_str!("../../tests/fixtures/native-import-adapters/contract-cases.json");

fn slot<'a>(node: &'a mut Value, key: &Value) -> &'a mut Value {
    match key {
        Value::String(name) => &mut node[name.as_str()],
        Value::Number(index) => &mut node[index.as_u64().unwrap() as usize],
        other => panic!("bad path step {other}"),
    }
}

fn apply(root: &mut Value, edit: &Value) {
    let path = edit["path"].as_array().unwrap();
    let (last, parents) = path.split_last().unwrap();
    let mut node = root;
    for step in parents {
        node = slot(node, step);
    }
    match edit["op"].as_str().unwrap() {
        "set" => *slot(node, last) = edit["value"].clone(),
        "remove" => {
            node.as_object_mut().unwrap().remove(last.as_str().unwrap());
        }
        "push" => slot(node, last)
            .as_array_mut()
            .unwrap()
            .push(edit["value"].clone()),
        "repeat" => {
            let first = slot(node, last)[0].clone();
            let count = edit["count"].as_u64().unwrap() as usize;
            *slot(node, last) = Value::Array(vec![first; count]);
        }
        other => panic!("unknown edit {other}"),
    }
}

#[test]
fn the_shared_parity_table_is_judged_as_declared() {
    let table: Value = serde_json::from_str(PARITY).unwrap();
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 82);
    assert_eq!(rows.iter().filter(|r| r["valid"] == true).count(), 21);
    for row in rows {
        let mut payload = table["base"].clone();
        for edit in row["edits"].as_array().unwrap() {
            apply(&mut payload, edit);
        }
        assert_eq!(
            request(&payload),
            row["valid"].as_bool().unwrap(),
            "{}",
            row["label"]
        );
    }
}

#[test]
fn import_success_maps_every_declared_source_once_and_preserves_skips() {
    let p = valid();
    assert!(receipt_matches(&p, &receipt()));
    for mutate in [
        |v: &mut Value| {
            v["layers"] = json!([]);
        },
        |v: &mut Value| {
            v["objects"].as_array_mut().unwrap().pop();
        },
        |v: &mut Value| {
            v["layers"][0]["sourceId"] = json!("foreign");
        },
        |v: &mut Value| {
            v["objects"][1]["sourceId"] = json!("w:0");
        },
        |v: &mut Value| {
            v["objects"][1]["strokeId"] = json!("native-stroke:8:0");
        },
        |v: &mut Value| {
            v["skipped"][0]["reason"] = json!("changed");
        },
    ] {
        let mut r = receipt();
        mutate(&mut r);
        assert!(result(&r));
        assert!(!receipt_matches(&p, &r));
        let q = envelope(p.clone());
        let reply = response(
            json!({"apiVersion":2,"requestId":"import-1","instanceId":"instance-A","documentId":"document-A","contentRevision":8,"ok":true,"result":r}),
        );
        assert!(!response_matches(&q, &reply));
    }
    let noop = json!({"applied":false,"historyEntriesAdded":0,"layers":[],"objects":[],"skipped":p["skipped"]});
    assert!(!receipt_matches(&p, &noop));
    let empty =
        json!({"command":"object.import.batch","layers":[],"records":[],"skipped":p["skipped"]});
    assert!(receipt_matches(&empty, &noop));
    let mut unavailable = p;
    unavailable["unavailable"] = json!([{"sourceId":"raster","need":"raster-asset"}]);
    assert!(!receipt_matches(&unavailable, &receipt()));
}

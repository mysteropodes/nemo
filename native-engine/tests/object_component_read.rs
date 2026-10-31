//! #2256: the read-only component model over the resolver's real admitted worlds. The expected values are read
//! from the world's own JSON (`componentTables`, `layers`), never from the resolver or the code under test.
use super::*;
use crate::evaluation::object_component_kernels::object_context_resolver::world::json as world_json;
use crate::object_codec::{decode_project, decode_storage_project};
use crate::request_receipts::DispatchErrorCode;

const V1: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L"}],"objects":[]}"#;

fn snapshot(kind: &str) -> ObjectSnapshot {
    let document = decode_storage_project(world_json(kind).as_bytes()).expect("admitted world");
    ObjectSnapshot::new("cmp-instance", document).expect("snapshot")
}
fn request(snapshot: &ObjectSnapshot, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"component-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object.component.read",
        "payload":{"atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"selector":selector}}))
    .unwrap()
}
fn ask(snapshot: &ObjectSnapshot, selector: Value) -> ResponseEnvelope {
    let response = snapshot.dispatch(request(snapshot, selector)).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn answer(snapshot: &ObjectSnapshot, selector: Value) -> Value {
    let response = ask(snapshot, selector);
    assert!(response.ok, "{:?}", response.error);
    response.result.unwrap()
}
fn refused(snapshot: &ObjectSnapshot, selector: Value) -> DispatchErrorCode {
    let response = ask(snapshot, selector);
    assert!(!response.ok && response.result.is_none());
    response.error.unwrap().code()
}
/// The authored world, read straight from its JSON.
fn world(kind: &str) -> Value {
    serde_json::from_str(world_json(kind)).unwrap()
}
fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[key].as_str().unwrap().to_string())
        .collect()
}
fn all_pages(snapshot: &ObjectSnapshot, selector: Value, limit: u64) -> Vec<Value> {
    let (mut cursor, mut items) = (0u64, Vec::new());
    loop {
        let mut page_selector = selector.clone();
        page_selector["cursor"] = json!(cursor);
        page_selector["limit"] = json!(limit);
        let page = answer(snapshot, page_selector);
        assert_eq!(page["cursor"], cursor);
        items.extend(page["items"].as_array().unwrap().iter().cloned());
        match page["nextCursor"].as_u64() {
            Some(next) => {
                assert!(next > cursor, "a page must advance");
                cursor = next;
            }
            None => return items,
        }
    }
}

#[test]
fn the_contexts_page_lists_the_scene_then_every_component_in_table_order() {
    let snap = snapshot("D");
    let doc = world("D");
    let tables = &doc["componentTables"]["components"]["components"];
    let page = answer(&snap, json!({"kind":"contexts","cursor":0,"limit":16}));
    assert_eq!(page["kind"], "contexts");
    assert_eq!(page["atRevision"], snap.content_revision());
    assert_eq!(page["documentSnapshotId"], snap.snapshot_id());
    assert_eq!(page["nextCursor"], Value::Null);
    let mut want = vec!["scene-root".to_string()];
    want.extend(strings(tables, "symbolId"));
    assert_eq!(page["contextCount"], want.len());
    assert_eq!(strings(&page["items"], "contextId"), want);
    let scene = &page["items"][0];
    assert_eq!(scene["contextKind"], "scene");
    assert_eq!(scene["name"], Value::Null);
    assert_eq!(scene["totalFrames"], doc["totalFrames"]);
    assert_eq!(scene["layerCount"], doc["layers"].as_array().unwrap().len());
    for (at, component) in tables.as_array().unwrap().iter().enumerate() {
        let item = &page["items"][at + 1];
        assert_eq!(item["contextKind"], "component");
        assert_eq!(item["totalFrames"], component["totalFrames"]);
        assert_eq!(
            item["layerCount"],
            component["layers"].as_array().unwrap().len()
        );
        assert_eq!(
            item["name"],
            component.get("name").cloned().unwrap_or(Value::Null)
        );
    }
}

#[test]
fn a_context_page_is_cut_by_its_limit_and_walks_to_the_end() {
    let snap = snapshot("D2");
    let one = answer(&snap, json!({"kind":"contexts","cursor":0,"limit":1}));
    assert_eq!(one["items"].as_array().unwrap().len(), 1);
    assert_eq!(one["nextCursor"], 1);
    let walked = all_pages(&snap, json!({"kind":"contexts"}), 1);
    assert_eq!(
        strings(&Value::Array(walked), "contextId"),
        [
            "scene-root",
            "sym_comp",
            "sym_comp_b",
            "sym_1788000123456_5000"
        ]
    );
    let past = answer(&snap, json!({"kind":"contexts","cursor":4,"limit":4}));
    assert_eq!(past["items"], json!([]));
    assert_eq!(past["nextCursor"], Value::Null);
    assert_eq!(
        refused(&snap, json!({"kind":"contexts","cursor":5,"limit":4})),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn one_component_context_reports_its_summary_instances_and_exposure() {
    let snap = snapshot("D");
    let doc = world("D");
    for component in doc["componentTables"]["components"]["components"]
        .as_array()
        .unwrap()
    {
        let id = component["symbolId"].as_str().unwrap();
        let summary = answer(&snap, json!({"kind":"context","contextId":id}));
        assert_eq!(summary["kind"], "context");
        assert_eq!(summary["contextId"], id);
        assert_eq!(summary["contextKind"], "component");
        assert_eq!(summary["totalFrames"], component["totalFrames"]);
        assert_eq!(
            summary["layerCount"],
            component["layers"].as_array().unwrap().len()
        );
        assert_eq!(
            summary["fps"],
            component.get("fps").cloned().unwrap_or(Value::Null)
        );
        assert_eq!(
            summary["exposedPropertyCount"],
            component
                .get("exposedProps")
                .map_or(0, |p| p.as_array().unwrap().len())
        );
        // A component record carries no camera keys natively.
        assert_eq!(summary["hasCameraKeys"], false);
        let placed: Vec<String> = doc["componentTables"]["components"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["symbolId"] == id && i["contextId"] == "scene-root")
            .map(|i| i["layerUid"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(summary["instanceCount"], placed.len());
        let listed = all_pages(&snap, json!({"kind":"instances","contextId":id}), 2);
        let mut got = strings(&Value::Array(listed), "layerUid");
        let mut want = placed;
        got.sort();
        want.sort();
        assert_eq!(got, want);
    }
}

#[test]
fn the_scene_summary_counts_its_instance_layers_and_has_no_camera_keys_here() {
    let snap = snapshot("D");
    let doc = world("D");
    let summary = answer(&snap, json!({"kind":"context","contextId":"scene-root"}));
    assert_eq!(summary["contextKind"], "scene");
    assert_eq!(
        summary["layerCount"],
        doc["layers"].as_array().unwrap().len()
    );
    let instances = doc["componentTables"]["components"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["contextId"] == "scene-root")
        .count();
    assert_eq!(summary["instanceCount"], instances);
    assert_eq!(summary["hasCameraKeys"], false);
}

#[test]
fn layers_page_in_document_order_with_uid_name_and_kind() {
    let snap = snapshot("D");
    let doc = world("D");
    let walked = all_pages(&snap, json!({"kind":"layers","contextId":"scene-root"}), 3);
    let want = strings(&doc["layers"], "layerUid");
    assert_eq!(strings(&Value::Array(walked.clone()), "layerUid"), want);
    let placed: Vec<&str> = doc["componentTables"]["components"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["contextId"] == "scene-root")
        .map(|i| i["layerUid"].as_str().unwrap())
        .collect();
    for (item, layer) in walked.iter().zip(doc["layers"].as_array().unwrap()) {
        assert_eq!(
            item["name"],
            layer.get("name").cloned().unwrap_or(Value::Null)
        );
        let uid = layer["layerUid"].as_str().unwrap();
        if placed.contains(&uid) {
            assert_eq!(item["layerKind"], "instance", "{uid}");
        }
    }
    let inner = &doc["componentTables"]["components"]["components"][0];
    let id = inner["symbolId"].as_str().unwrap();
    let layers = all_pages(&snap, json!({"kind":"layers","contextId":id}), 16);
    assert_eq!(
        strings(&Value::Array(layers), "layerUid"),
        strings(&inner["layers"], "layerUid")
    );
}

#[test]
fn a_v1_document_declares_only_the_scene() {
    let snap = ObjectSnapshot::new("v1", decode_project(V1.as_bytes()).unwrap()).unwrap();
    let page = answer(&snap, json!({"kind":"contexts","cursor":0,"limit":4}));
    assert_eq!(strings(&page["items"], "contextId"), ["scene-root"]);
    assert_eq!(page["items"][0]["layerCount"], 1);
    let layers = answer(
        &snap,
        json!({"kind":"layers","contextId":"scene-root","cursor":0,"limit":4}),
    );
    assert_eq!(layers["items"][0]["layerUid"], "L");
    assert_eq!(layers["items"][0]["name"], Value::Null);
    assert_eq!(layers["items"][0]["layerKind"], "drawing");
}

#[test]
fn unknown_ids_stale_pins_and_malformed_selectors_are_typed_refusals() {
    let snap = snapshot("D");
    for selector in [
        json!({"kind":"context","contextId":"sym_nope"}),
        json!({"kind":"layers","contextId":"sym_nope","cursor":0,"limit":4}),
        json!({"kind":"instances","contextId":"sym_nope","cursor":0,"limit":4}),
    ] {
        assert_eq!(refused(&snap, selector), DispatchErrorCode::NotFound);
    }
    // The scene is not instanced by anything.
    assert_eq!(
        refused(
            &snap,
            json!({"kind":"instances","contextId":"scene-root","cursor":0,"limit":4})
        ),
        DispatchErrorCode::InvalidRequest
    );
    for selector in [
        json!({"kind":"contexts","cursor":0,"limit":0}),
        json!({"kind":"contexts","cursor":0,"limit":17}),
        json!({"kind":"contexts","cursor":-1,"limit":4}),
        json!({"kind":"contexts","cursor":0}),
        json!({"kind":"contexts","cursor":0,"limit":4,"extra":1}),
        json!({"kind":"context","contextId":""}),
        json!({"kind":"context","contextId":"scene-root","cursor":0}),
        json!({"kind":"other"}),
        json!("contexts"),
    ] {
        assert_eq!(
            refused(&snap, selector.clone()),
            DispatchErrorCode::InvalidRequest,
            "{selector}"
        );
    }
    for stale in [
        json!({"atRevision":snap.content_revision() + 1,"documentSnapshotId":snap.snapshot_id()}),
        json!({"atRevision":snap.content_revision(),"documentSnapshotId":"native-object:other:0"}),
    ] {
        let mut stale_request = request(&snap, json!({"kind":"contexts","cursor":0,"limit":4}));
        stale_request.payload["atRevision"] = stale["atRevision"].clone();
        stale_request.payload["documentSnapshotId"] = stale["documentSnapshotId"].clone();
        let response = snap.dispatch(stale_request).unwrap();
        assert_eq!(
            response.error.unwrap().code(),
            DispatchErrorCode::StaleRevision
        );
    }
    let mut extra = request(&snap, json!({"kind":"contexts","cursor":0,"limit":4}));
    extra.payload["extra"] = json!(1);
    assert_eq!(
        snap.dispatch(extra).unwrap().error.unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    let mut with_revision = request(&snap, json!({"kind":"contexts","cursor":0,"limit":4}));
    with_revision.expected_revision = Some(snap.content_revision());
    assert_eq!(
        snap.dispatch(with_revision).unwrap().error.unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn an_item_that_cannot_fit_the_response_alone_is_unavailable_never_truncated() {
    let mut doc = world("D");
    let long = "n".repeat(4000);
    doc["layers"][0]["name"] = json!(long);
    let document =
        decode_storage_project(doc.to_string().as_bytes()).expect("a long name is admitted");
    let snap = ObjectSnapshot::new("cmp-long", document).unwrap();
    let response = ask(
        &snap,
        json!({"kind":"layers","contextId":"scene-root","cursor":0,"limit":1}),
    );
    assert!(!response.ok);
    assert_eq!(
        response.error.unwrap().code(),
        DispatchErrorCode::Unavailable
    );
    // A later page that does fit still answers.
    let page = answer(
        &snap,
        json!({"kind":"layers","contextId":"scene-root","cursor":1,"limit":2}),
    );
    assert!(!page["items"].as_array().unwrap().is_empty());
}

#[test]
fn a_context_whose_summary_cannot_fit_alone_is_cut_and_flagged_and_the_listing_goes_on() {
    let mut doc = world("D2");
    let wide = "é\u{1F600}x".repeat(1500);
    doc["componentTables"]["components"]["components"][0]["name"] = json!(wide);
    let document =
        decode_storage_project(doc.to_string().as_bytes()).expect("a long component name is admitted");
    let snap = ObjectSnapshot::new("cmp-wide", document).unwrap();
    let ids = ["scene-root", "sym_comp", "sym_comp_b", "sym_1788000123456_5000"];
    // The oversized context asked for alone: kept, its name a whole-character prefix, flagged, the cursor moves past it.
    let alone = answer(&snap, json!({"kind":"contexts","cursor":1,"limit":1}));
    let item = &alone["items"][0];
    assert_eq!(item["contextId"], ids[1]);
    assert_eq!(item["nameTruncated"], true);
    let kept = item["name"].as_str().expect("a cut name stays a string");
    assert!(!kept.is_empty() && kept.len() < wide.len() && wide.starts_with(kept));
    assert_eq!(item["totalFrames"], doc["componentTables"]["components"]["components"][0]["totalFrames"]);
    assert_eq!(alone["nextCursor"], 2);
    // A listing from the top walks past it: every context once, only the oversized one flagged, the others untouched.
    let walked = all_pages(&snap, json!({"kind":"contexts"}), 16);
    assert_eq!(strings(&Value::Array(walked.clone()), "contextId"), ids);
    let flagged: Vec<&Value> = walked.iter().filter(|c| c.get("nameTruncated").is_some()).collect();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0]["contextId"], ids[1]);
    // The cut is the longest that fits: one more character would not.
    let more = kept.chars().count() + 1;
    let mut probe = alone.clone();
    probe["items"][0]["name"] = json!(wide.chars().take(more).collect::<String>());
    let envelope = snap.dispatch(request(&snap, json!({"kind":"contexts","cursor":1,"limit":1}))).unwrap();
    let mut grown = envelope.clone();
    grown.result = Some(probe);
    assert!(serde_json::to_vec(&grown).unwrap().len() > 4096);
}

#[test]
fn a_long_name_cuts_the_page_early_and_the_cursor_resumes_without_loss() {
    let mut doc = world("D");
    let long = "n".repeat(900);
    for index in 0..6 {
        doc["layers"][index]["name"] = json!(format!("{long}{index}"));
    }
    let document =
        decode_storage_project(doc.to_string().as_bytes()).expect("long names are admitted");
    let snap = ObjectSnapshot::new("cmp-cut", document).unwrap();
    let first = answer(
        &snap,
        json!({"kind":"layers","contextId":"scene-root","cursor":0,"limit":16}),
    );
    let taken = first["items"].as_array().unwrap().len();
    assert!(
        taken >= 1 && taken < 6,
        "the byte bound cut the page: {taken}"
    );
    assert_eq!(first["nextCursor"], taken);
    let walked = all_pages(&snap, json!({"kind":"layers","contextId":"scene-root"}), 16);
    assert_eq!(
        strings(&Value::Array(walked), "layerUid"),
        strings(&doc["layers"], "layerUid")
    );
}

#[test]
fn the_scene_reports_whether_the_stored_project_camera_has_keys() {
    let project = |camera: Option<Value>| {
        let mut meta = json!({"canvasW":320,"canvasH":180,"fps":24});
        if let Some(keys) = camera {
            meta["cameraKeys"] = keys;
        }
        json!({"format":"nemo.native-project-document","formatVersion":1,"project":meta,
            "drawing":world("D"),"viewportReference":null,"assets":[]})
    };
    let summary = |camera: Option<Value>| {
        let owner = crate::history::NativeObjectHistory::from_project_file(
            "camera-read",
            &serde_json::to_vec(&project(camera)).unwrap(),
        )
        .expect("a project around the component world");
        let snap = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let scene = answer(&snap, json!({"kind":"context","contextId":"scene-root"}));
        let component = answer(&snap, json!({"kind":"context","contextId":"sym_comp"}));
        (
            scene["hasCameraKeys"].clone(),
            component["hasCameraKeys"].clone(),
        )
    };
    let keys = json!([{"frame":0,"x":160,"y":90,"w":320}]);
    assert_eq!(summary(Some(keys)), (json!(true), json!(false)));
    assert_eq!(summary(Some(json!([]))), (json!(false), json!(false)));
    assert_eq!(summary(None), (json!(false), json!(false)));
}

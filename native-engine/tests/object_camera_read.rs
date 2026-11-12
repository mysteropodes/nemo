//! #2357 G44: the camera read query (keys, layerOn) through the real public envelope, its ownership by the
//! snapshot dispatcher, and the AE camera kernel fed from the very same pinned snapshot.
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const READ: &str = "query.document.object.project.camera.read";

fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":320,"canvasH":180,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}
    });
    NativeObjectHistory::from_project_file("camera-read", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn request(p: &ObjectSnapshot, selector: Value) -> OpacityRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":"camera-read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":READ,
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector}
    }))
    .unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    let response = p.dispatch(request(p, selector)).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn keys(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| json!({"frame":i*4,"x":10+i,"y":20,"w":300,"rot":1.5,"ease":[0.25,0.1,0.25,1.0]}))
            .collect(),
    )
}

#[test]
fn settings_report_presence_count_and_layer_flag_without_inventing_state() {
    let absent = pin(&owner(json!({})));
    let r = &query(&absent, json!({"kind":"settings"}))["result"]["result"];
    assert_eq!(r, &json!({"keysPresent":false,"keyCount":0}));
    let empty = pin(&owner(json!({"cameraKeys":[],"cameraLayerOn":false})));
    let r = &query(&empty, json!({"kind":"settings"}))["result"]["result"];
    assert_eq!(r, &json!({"keysPresent":true,"keyCount":0,"layerOn":false}));
    let on = pin(&owner(json!({"cameraKeys":keys(3),"cameraLayerOn":true})));
    let r = &query(&on, json!({"kind":"settings"}))["result"]["result"];
    assert_eq!(r, &json!({"keysPresent":true,"keyCount":3,"layerOn":true}));
}

#[test]
fn key_pages_carry_stored_keys_and_chain_to_completion_within_the_bound() {
    let p = pin(&owner(json!({"cameraKeys":keys(40),"cameraLayerOn":true})));
    let (mut cursor, mut seen) = (0u64, 0u64);
    loop {
        let page = query(&p, json!({"kind":"keyPage","cursor":cursor,"limit":16}));
        let body = &page["result"]["result"];
        assert_eq!(body["kind"], "cameraKeyPage");
        assert_eq!(body["keyCount"], 40);
        for item in body["items"].as_array().unwrap() {
            seen += 1;
            assert_eq!(item["ordinal"], seen);
            assert_eq!(item["frame"].as_f64(), Some(((seen - 1) * 4) as f64));
            assert_eq!(item["ease"], json!([0.25, 0.1, 0.25, 1.0]));
        }
        match body["nextCursor"].as_u64() {
            Some(next) => {
                assert!(next > cursor);
                cursor = next;
            }
            None => break,
        }
    }
    assert_eq!(seen, 40);
}

#[test]
fn stale_wrong_context_and_bad_selectors_are_typed_refusals_and_nothing_mutates() {
    let o = owner(json!({"cameraKeys":keys(2),"cameraLayerOn":true}));
    let p = pin(&o);
    let before = o.content_revision();
    let mut stale = request(&p, json!({"kind":"settings"}));
    stale.payload["atRevision"] = json!(before + 1);
    let code = |q: OpacityRequest| {
        let v = serde_json::to_value(p.dispatch(q).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        v["error"]["code"].clone()
    };
    assert_eq!(code(stale), "stale_revision");
    let mut ctx = request(&p, json!({"kind":"settings"}));
    ctx.payload["contextId"] = json!("native-component:1:0");
    assert_eq!(code(ctx), "unavailable");
    assert_eq!(
        code(request(&p, json!({"kind":"keyPage","cursor":0,"limit":17}))),
        "invalid_request"
    );
    assert_eq!(
        code(request(&p, json!({"kind":"keyPage","cursor":3,"limit":1}))),
        "invalid_request"
    );
    assert_eq!(
        code(request(&p, json!({"kind":"other"}))),
        "invalid_request"
    );
    assert_eq!(o.content_revision(), before);
}

//! `object.component.state.edit` through the one history owner (#2256 C-comp-owned-state): the camera keys and the markers a
//! component owns (`sym.cameraKeys`, `sym.markers`) are written by one closed edit of its stored record. One edit is one
//! revision and one undo entry, an edit that changes nothing publishes nothing, exact undo/redo bytes, and every refusal
//! publishes nothing.
use super::fixture::{decode, document, layer, record};
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

const OP: &str = "command.document.object.component.state.edit";

fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|f| json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
fn base(component_extra: Value) -> Value {
    let layers = [layer("ly_inst", "Inst", &[]), layer("ly_plain", "Plain", &[(0, "x1")])];
    let instance = json!({"contextId":"scene-root","layerUid":"ly_inst","symbolId":"sym_x","symPlayMode":"once",
        "symSpeed":1,"symPlacedAt":0,"symSingleFrame":0});
    let mut inner = record("xi", "s0", 0);
    inner["target"]["contextId"] = json!("sym_x");
    let mut component = json!({"symbolId":"sym_x","totalFrames":24,
        "layers":[{"layerUid":"xi","name":"Layer 1","visible":true,"locked":false,"timing":timing(&[0])}],
        "objects":[inner]});
    for (key, value) in component_extra.as_object().expect("object") {
        component[key] = value.clone();
    }
    document(
        &layers,
        json!({"componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[component],"instances":[instance]}}}),
    )
}
fn owner(extra: Value) -> NativeObjectHistory {
    NativeObjectHistory::new("state-edit", decode(&base(extra))).unwrap()
}
fn bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    encode_storage_project(snapshot.document()).unwrap()
}
fn component(owner: &NativeObjectHistory) -> Value {
    let doc: Value = serde_json::from_slice(&bytes(owner)).unwrap();
    doc["componentTables"]["components"]["components"][0].clone()
}
/// JSON equality that reads `0.0` and `0` as the same number (the stored camera keys are f64).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}
macro_rules! assert_same {
    ($a:expr, $b:expr) => {{
        let (a, b) = (&$a, &$b);
        assert!(same(a, b), "left: {a}\nright: {b}");
    }};
}
fn send(
    owner: &mut NativeObjectHistory,
    body: Value,
) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
    let mut payload = json!({"command":"object.component.state.edit","contextId":"sym_x"});
    for (key, value) in body.as_object().expect("object") {
        payload[key] = value.clone();
    }
    owner.dispatch_component_setup(OP, &payload)
}
fn applied(owner: &mut NativeObjectHistory, body: Value) {
    assert_eq!(
        send(owner, body).unwrap(),
        CompactResult::Command { applied: true, history_entries_added: 1 }
    );
}
fn unchanged(owner: &mut NativeObjectHistory, body: Value) {
    let before = bytes(owner);
    let depth = owner.history_depths();
    assert_eq!(
        send(owner, body.clone()).unwrap(),
        CompactResult::Command { applied: false, history_entries_added: 0 },
        "{body}"
    );
    assert_eq!(bytes(owner), before, "{body}");
    assert_eq!(owner.history_depths(), depth, "{body}");
}

#[test]
fn a_new_camera_key_takes_the_baseline_defaults_and_keeps_the_frame_order() {
    let mut owner = owner(json!({}));
    assert!(component(&owner).get("cameraKeys").is_none(), "a component never opened stores no camera");
    applied(&mut owner, json!({"kind":"setCameraKey","frame":12,"x":300,"y":200,"w":640}));
    applied(&mut owner, json!({"kind":"setCameraKey","frame":0,"x":160,"y":90,"w":320,"rot":15}));
    let keys = component(&owner)["cameraKeys"].clone();
    assert_same!(
        keys,
        json!([
            {"frame":0,"x":160,"y":90,"w":320,"rot":15,"hOut":[0,0],"hIn":[0,0],"ease":[0.42,0,0.58,1]},
            {"frame":12,"x":300,"y":200,"w":640,"rot":0,"hOut":[0,0],"hIn":[0,0],"ease":[0.42,0,0.58,1]},
        ])
    );
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn a_named_member_of_an_existing_key_changes_and_the_others_keep_their_bytes() {
    let mut owner = owner(json!({"cameraKeys":[
        {"frame":0,"x":10,"y":20,"w":100,"rot":5,"hOut":[1,2],"hIn":[3,4],"ease":[0.1,0.2,0.3,0.4]}]}));
    applied(&mut owner, json!({"kind":"setCameraKey","frame":0,"x":50}));
    applied(&mut owner, json!({"kind":"setCameraKey","frame":0,"hOut":[9,8]}));
    assert_same!(
        component(&owner)["cameraKeys"],
        json!([{"frame":0,"x":50,"y":20,"w":100,"rot":5,"hOut":[9,8],"hIn":[3,4],"ease":[0.1,0.2,0.3,0.4]}])
    );
}

#[test]
fn ease_remove_and_clear_write_the_stored_keys_and_every_edit_undoes_to_the_exact_bytes() {
    let mut owner = owner(json!({}));
    let original = bytes(&owner);
    applied(&mut owner, json!({"kind":"setCameraKey","frame":0,"x":160,"y":90,"w":320}));
    applied(&mut owner, json!({"kind":"setCameraKey","frame":8,"x":200,"y":100,"w":160}));
    applied(&mut owner, json!({"kind":"setCameraEase","frame":0,"ease":[0.25,0.1,0.25,1]}));
    assert_same!(component(&owner)["cameraKeys"][0]["ease"], json!([0.25, 0.1, 0.25, 1]));
    let with_ease = bytes(&owner);
    applied(&mut owner, json!({"kind":"removeCameraKey","frame":8}));
    assert_eq!(component(&owner)["cameraKeys"].as_array().unwrap().len(), 1);
    applied(&mut owner, json!({"kind":"clearCamera"}));
    assert_eq!(component(&owner)["cameraKeys"], json!([]));
    assert_eq!(owner.history_depths(), (5, 0));
    // The result reopens byte for byte.
    let after = bytes(&owner);
    let reopened = decode_storage_project(&after).expect("the document reopens");
    assert_eq!(encode_storage_project(&reopened).unwrap(), after);
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), with_ease);
    owner.redo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(bytes(&owner), after);
    for _ in 0..5 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(bytes(&owner), original);
}

#[test]
fn markers_are_one_per_frame_sorted_and_edited_by_frame() {
    let mut owner = owner(json!({}));
    applied(&mut owner, json!({"kind":"addMarker","frame":9,"name":"late"}));
    applied(&mut owner, json!({"kind":"addMarker","frame":2}));
    assert_eq!(
        component(&owner)["markers"],
        json!([{"frame":2,"name":"","color":"#e8b64c"},{"frame":9,"name":"late","color":"#e8b64c"}])
    );
    // A second marker on a taken frame changes nothing, like AE.
    unchanged(&mut owner, json!({"kind":"addMarker","frame":9,"name":"other"}));
    applied(&mut owner, json!({"kind":"updateMarker","frame":9,"name":"renamed","color":"#ff0000"}));
    applied(&mut owner, json!({"kind":"updateMarker","frame":9,"newFrame":1}));
    assert_eq!(
        component(&owner)["markers"],
        json!([{"frame":1,"name":"renamed","color":"#ff0000"},{"frame":2,"name":"","color":"#e8b64c"}])
    );
    applied(&mut owner, json!({"kind":"removeMarker","frame":1}));
    applied(&mut owner, json!({"kind":"clearMarkers"}));
    assert_eq!(component(&owner)["markers"], json!([]));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(component(&owner)["markers"].as_array().unwrap().len(), 1);
}

#[test]
fn an_edit_that_changes_nothing_publishes_nothing_and_authors_no_empty_list() {
    let mut owner = owner(json!({}));
    unchanged(&mut owner, json!({"kind":"clearMarkers"}));
    unchanged(&mut owner, json!({"kind":"clearCamera"}));
    assert!(component(&owner).get("markers").is_none());
    assert!(component(&owner).get("cameraKeys").is_none());
    applied(&mut owner, json!({"kind":"setCameraKey","frame":3,"x":1,"y":2,"w":30}));
    unchanged(&mut owner, json!({"kind":"setCameraKey","frame":3,"x":1,"y":2,"w":30}));
    unchanged(&mut owner, json!({"kind":"setCameraKey","frame":3}));
}

#[test]
fn refusals_publish_nothing() {
    let mut owner = owner(json!({"cameraKeys":[{"frame":0,"x":1,"y":2,"w":3}],"markers":[{"frame":4,"name":"a","color":"#fff"}]}));
    let original = bytes(&owner);
    let depth = owner.history_depths();
    let cases: Vec<(Value, DispatchErrorCode)> = vec![
        (json!({"kind":"setCameraKey","frame":24,"x":1,"y":2,"w":3}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":5,"x":1,"y":2}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":5,"x":1,"y":2,"w":0}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":5,"x":1,"y":2,"w":3,"ease":[2,0,0.5,1]}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":5,"x":1,"y":2,"w":3,"hOut":[1]}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":-1,"x":1,"y":2,"w":3}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"setCameraKey","frame":5,"x":1,"y":2,"w":3,"extra":1}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"removeCameraKey","frame":9}), DispatchErrorCode::NotFound),
        (json!({"kind":"setCameraEase","frame":9,"ease":[0.4,0,0.6,1]}), DispatchErrorCode::NotFound),
        (json!({"kind":"setCameraEase","frame":0}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"addMarker","frame":24}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"addMarker","frame":1,"color":""}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"updateMarker","frame":8}), DispatchErrorCode::NotFound),
        (json!({"kind":"updateMarker","frame":4,"newFrame":99}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"removeMarker","frame":8}), DispatchErrorCode::NotFound),
        (json!({"kind":"nope"}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"clearMarkers","frame":1}), DispatchErrorCode::InvalidRequest),
        (json!({"kind":"clearMarkers","contextId":"scene-root"}), DispatchErrorCode::Unavailable),
        (json!({"kind":"clearMarkers","contextId":"sym_missing"}), DispatchErrorCode::NotFound),
    ];
    for (body, code) in cases {
        assert_eq!(send(&mut owner, body.clone()).unwrap_err().0, code, "{body}");
        assert_eq!(bytes(&owner), original, "{body}");
        assert_eq!(owner.history_depths(), depth, "{body}");
    }
    // A stale command word or a missing context is malformed.
    for payload in [
        json!({"command":"object.component.convert","contextId":"sym_x","kind":"clearMarkers"}),
        json!({"command":"object.component.state.edit","kind":"clearMarkers"}),
        json!({"command":"object.component.state.edit","contextId":"sym_x"}),
        json!([1]),
    ] {
        assert_eq!(
            owner.dispatch_component_setup(OP, &payload).unwrap_err().0,
            DispatchErrorCode::InvalidRequest,
            "{payload}"
        );
    }
    assert_eq!(bytes(&owner), original);
}

#[cfg(feature = "evaluation")]
mod read {
    use super::*;
    use crate::request_receipts::OpacityRequest;

    fn ask(owner: &NativeObjectHistory, selector: Value) -> Value {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"component-read",
            "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
            "operation":"query.document.object.component.read",
            "payload":{"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),"selector":selector}}))
        .unwrap();
        let response = snapshot.dispatch(request).unwrap();
        assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
        serde_json::to_value(&response).unwrap()
    }

    #[test]
    fn the_stored_markers_and_camera_keys_page_out_in_stored_order_and_the_context_reports_the_camera() {
        let mut owner = owner(json!({}));
        for frame in [9u32, 2, 5] {
            applied(&mut owner, json!({"kind":"addMarker","frame":frame,"name":format!("m{frame}")}));
        }
        applied(&mut owner, json!({"kind":"setCameraKey","frame":4,"x":10,"y":20,"w":100,"rot":3}));
        applied(&mut owner, json!({"kind":"setCameraKey","frame":0,"x":1,"y":2,"w":30}));
        let page = ask(&owner, json!({"kind":"markers","contextId":"sym_x","cursor":0,"limit":2}));
        assert_eq!(page["ok"], true, "{page}");
        let result = &page["result"];
        assert_eq!(result["markerCount"], 3);
        assert_eq!(result["items"][0], json!({"frame":2,"name":"m2","color":"#e8b64c"}));
        assert_eq!(result["items"][1]["frame"], 5);
        assert_eq!(result["nextCursor"], 2);
        let tail = ask(&owner, json!({"kind":"markers","contextId":"sym_x","cursor":2,"limit":2}));
        assert_eq!(tail["result"]["items"][0]["name"], "m9");
        assert!(tail["result"]["nextCursor"].is_null());
        let keys = ask(&owner, json!({"kind":"cameraKeys","contextId":"sym_x","cursor":0,"limit":16}));
        let keys = &keys["result"];
        assert_eq!(keys["keyCount"], 2);
        assert_eq!(keys["items"][0]["frame"], 0);
        assert_eq!(keys["items"][1]["frame"], 4);
        assert_eq!(keys["items"][1]["rot"], 3.0);
        assert_eq!(keys["items"][1]["ease"], json!([0.42, 0.0, 0.58, 1.0]));
        let summary = ask(&owner, json!({"kind":"context","contextId":"sym_x"}));
        assert_eq!(summary["result"]["hasCameraKeys"], true);
        owner.undo(owner.content_revision()).unwrap();
        owner.undo(owner.content_revision()).unwrap();
        let empty = ask(&owner, json!({"kind":"cameraKeys","contextId":"sym_x","cursor":0,"limit":16}));
        assert_eq!(empty["result"]["keyCount"], 0);
        assert_eq!(empty["result"]["items"], json!([]));
        let summary = ask(&owner, json!({"kind":"context","contextId":"sym_x"}));
        assert_eq!(summary["result"]["hasCameraKeys"], false);
    }

    #[test]
    fn the_scene_and_an_unknown_context_have_no_component_state_page() {
        let owner = owner(json!({}));
        for kind in ["markers", "cameraKeys"] {
            let scene = ask(&owner, json!({"kind":kind,"contextId":"scene-root","cursor":0,"limit":4}));
            assert_eq!(scene["ok"], false, "{scene}");
            let missing = ask(&owner, json!({"kind":kind,"contextId":"sym_nope","cursor":0,"limit":4}));
            assert_eq!(missing["ok"], false, "{missing}");
        }
    }
}

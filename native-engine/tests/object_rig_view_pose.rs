//! #2255: the held pose drag (a `preview` of the rig view), the Alt-drag IK edit and the world map of a rig layer with Motion.
//! The invariant under test: the preview of a gesture answers exactly what the edit that follows it leaves behind, and a preview
//! publishes nothing.
use super::object_rig_edit_rows::*;
use super::*;
use crate::object_snapshot::ObjectSnapshot;
use serde_json::{json, Value};

const PROOF: (&str, u64, &str, u32) = ("native-work-4", 2, "scene-root", 0);

fn view(snapshot: &ObjectSnapshot, part: &str, preview: Option<Value>, limit: u64) -> Value {
    let mut view = json!({"kind":"rig","contextId":"scene-root","frame":0,"workId":"native-work-4","viewGeneration":2,
        "layerUid":LAYER,"part":part,"bind":0,"vertex":0,"cursor":0,"limit":limit});
    if let Some(preview) = preview {
        view["preview"] = preview;
    }
    json!({"apiVersion":2,"requestId":"rig-read","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"view":view}})
}

fn read(owner: &NativeObjectHistory, part: &str, preview: Option<Value>) -> Value {
    let snapshot = current(owner);
    let request = serde_json::from_value(view(&snapshot, part, preview, if part == "posed" { 24 } else { 8 })).unwrap();
    let response = snapshot.dispatch_with_rig_view(request, Ok(PROOF)).unwrap();
    assert!(response.ok, "{part}: {:?}", response.error);
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response.result.unwrap()
}

fn refused(owner: &NativeObjectHistory, preview: Value) -> bool {
    let snapshot = current(owner);
    let request = serde_json::from_value(view(&snapshot, "vertices", Some(preview), 8)).unwrap();
    !snapshot.dispatch_with_rig_view(request, Ok(PROOF)).unwrap().ok
}

fn close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= 1e-6,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| close(v, w))),
        _ => a == b,
    }
}

fn stroke() -> Value {
    record(&json!({"id":"s1","closed":false,"pts":[[0,0],[10,0],[20,0]],"hs":[[null,null],[null,null],[null,null]]}))
}

/// Two bones sharing a joint at (10, 0): a 10 + 10 chain along x, the stroke not bound yet.
fn bare_arm(extra: impl FnOnce(&mut Value)) -> NativeObjectHistory {
    // A version 2 document (the one a layer's Motion is consumed in), one stroke on L0.
    let mut value = crate::history::object_history::object_widget_edit_rows::base(2);
    value["objects"] = json!([stroke()]);
    value["compositionOrder"]["contexts"][0]["scopes"] = json!([{"frameScope":{"kind":"authored","frame":0},
        "elements":[{"layerUid":LAYER,"strokeId":"s1","naturalPosition":0,"order":{}}]}]);
    let bone = |a: [i32; 2], b: [i32; 2]| {
        let segs = json!([{"point":a,"handleIn":[0,0],"handleOut":[0,0]},{"point":b,"handleIn":[0,0],"handleOut":[0,0]}]);
        json!({"segments":segs,"restSegments":segs,"closed":false})
    };
    value["layers"][0]["rig"] = json!({"bones":{"bone1":bone([0,0],[10,0]),"bone2":bone([10,0],[20,0])},"ikChains":{},"nextId":3,"binds":[]});
    extra(&mut value);
    owner_of_text(&value.to_string())
}

fn arm(extra: impl FnOnce(&mut Value)) -> NativeObjectHistory {
    let mut owner = bare_arm(extra);
    run_ok(&mut owner, "bind", payload(json!({"kind":"bindStroke","strokeId":"s1","frame":0,"radius":50})));
    owner
}

fn vertices(owner: &NativeObjectHistory, preview: Option<Value>) -> Value {
    read(owner, "vertices", preview)["items"].clone()
}

#[test]
fn a_held_anchor_drag_previews_exactly_what_its_edit_leaves_and_publishes_nothing() {
    let mut owner = arm(|_| {});
    let held = json!({"segment":{"boneId":"bone2","vi":1,"part":"point","to":[18,12]}});
    let before = state(&owner);
    let (v, p) = (vertices(&owner, Some(held.clone())), read(&owner, "posed", Some(held.clone())));
    assert_eq!(state(&owner), before, "a preview spends neither history nor bytes");
    assert_eq!(p["preview"], true);
    assert_eq!(v[3]["point"], json!([18.0, 12.0]), "the candidate rig shows the dragged anchor");
    assert_ne!(p["items"], read(&owner, "posed", None)["items"], "the bound outline deforms in the preview");
    run_ok(&mut owner, "move", payload(json!({"kind":"moveSegment","boneId":"bone2","vi":1,"part":"point","to":[18,12]})));
    assert!(close(&vertices(&owner, None), &v), "{} vs {v}", vertices(&owner, None));
    assert_eq!(read(&owner, "posed", None)["items"], p["items"], "the preview is what the edit then draws");
    assert_eq!(read(&owner, "bound", None)["items"].as_array().unwrap().len(), 3);
}

#[test]
fn a_held_handle_drag_mirrors_the_other_handle_unless_the_tangent_is_broken() {
    let owner = arm(|_| {});
    let both = vertices(&owner, Some(json!({"segment":{"boneId":"bone1","vi":1,"part":"handleOut","to":[14,3]}})));
    assert_eq!((both[1]["handleOut"].clone(), both[1]["handleIn"].clone()), (json!([4.0, 3.0]), json!([-4.0, -3.0])));
    let broken = vertices(&owner, Some(json!({"segment":{"boneId":"bone1","vi":1,"part":"handleOut","to":[14,3],"breakTangent":true}})));
    assert_eq!((broken[1]["handleOut"].clone(), broken[1]["handleIn"].clone()), (json!([4.0, 3.0]), json!([0.0, 0.0])));
}

#[test]
fn the_alt_drag_of_a_chain_end_solves_the_two_bone_chain_in_one_history_entry() {
    let mut owner = arm(|_| {});
    let held = json!({"ik":{"boneId":"bone2","vi":1,"to":[10,10]}});
    let before = state(&owner);
    let preview = vertices(&owner, Some(held));
    assert_eq!(state(&owner), before);
    // l1 = l2 = 10 and the target is 10 * sqrt(2) from the root: the law of cosines bends the joint to (0, 10), the end to (10, 10).
    let want = json!([[0, 0], [0, 10], [0, 10], [10, 10]]);
    let points = |items: &Value| Value::Array(items.as_array().unwrap().iter().map(|v| v["point"].clone()).collect());
    assert!(close(&points(&preview), &want), "{preview}");
    let reply = run_ok(&mut owner, "ik", payload(json!({"kind":"dragIk","end":{"boneId":"bone2","vi":1},"to":[10,10]})));
    assert_eq!(reply["result"]["historyEntriesAdded"], 1);
    assert!(close(&points(&vertices(&owner, None)), &want));
    let chain = &read(&owner, "chains", None)["items"][0];
    assert_eq!((chain["endKey"].clone(), chain["flip"].clone()), (json!("bone2:1"), json!(false)));
    assert!(close(&json!([chain["l1"], chain["l2"]]), &json!([10, 10])));
    assert_eq!(read(&owner, "posed", None)["total"], 3);
    // Save and reload: the solved pose and the filed chain survive the project file.
    let reloaded = owner_of_text(&wire(&owner).to_string());
    assert!(close(&points(&vertices(&reloaded, None)), &want));
    assert_eq!(read(&reloaded, "chains", None)["items"], read(&owner, "chains", None)["items"]);
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    assert_eq!(state(&owner).2, (1, 1), "the drag was one entry: the bind stays, the drag is undone");
    assert!(close(&points(&vertices(&owner, None)), &json!([[0, 0], [10, 0], [10, 0], [20, 0]])));
}

#[test]
fn an_anchor_that_ends_no_two_bone_chain_and_a_malformed_preview_are_typed_refusals() {
    let mut owner = arm(|_| {});
    let before = state(&owner);
    let reply = run(&mut owner, "mid", payload(json!({"kind":"dragIk","end":{"boneId":"bone1","vi":5},"to":[1,1]})));
    assert_eq!((reply["ok"].clone(), code(&reply)), (json!(false), "invalid_request"));
    let reply = run(&mut owner, "ghost", payload(json!({"kind":"dragIk","end":{"boneId":"nope","vi":0},"to":[1,1]})));
    assert_eq!(reply["ok"], false);
    assert_eq!(state(&owner), before, "a refusal publishes nothing");
    assert!(refused(&owner, json!({})), "a preview names exactly one gesture");
    assert!(refused(&owner, json!({"ik":{"boneId":"bone1","vi":5,"to":[1,1]}})), "no chain ends there");
    assert!(refused(&owner, json!({"segment":{"boneId":"nope","vi":0,"part":"point","to":[1,1]}})));
    assert!(refused(&owner, json!({"segment":{"boneId":"bone1","vi":9,"part":"point","to":[1,1]}})));
    assert!(refused(&owner, json!({"segment":{"boneId":"bone1","vi":0,"part":"knee","to":[1,1]}})));
}

#[test]
fn a_locked_layer_refuses_the_ik_drag() {
    let mut owner = bare_arm(|value| value["layers"][0]["locked"] = json!(true));
    let before = state(&owner);
    let reply = run(&mut owner, "ik", payload(json!({"kind":"dragIk","end":{"boneId":"bone2","vi":1},"to":[10,10]})));
    assert_eq!((reply["ok"].clone(), code(&reply)), (json!(false), "unavailable"));
    assert_eq!(state(&owner), before);
}

fn motion(rotation: f64, scale: [f64; 2], position: [f64; 2]) -> impl FnOnce(&mut Value) {
    move |value| {
        value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":LAYER},"properties":[
            {"property":"rotation","static":[rotation]},{"property":"scale","static":scale},{"property":"position","static":position}]}]);
    }
}

#[test]
fn a_rotated_scaled_layer_carries_its_world_map_instead_of_a_refusal() {
    let owner = arm(motion(90.0, [200.0, 100.0], [5.0, 7.0]));
    let page = read(&owner, "vertices", None);
    assert_eq!(page["unavailable"], Value::Null);
    assert_eq!(page["total"], 4, "the geometry is still listed, in the layer's own space");
    // Pivot = the centre of the stroke bounds (10, 0); x' = pivot + R(90) S(2, 1) (p - pivot) + (5, 7).
    let m = page["matrix"].clone();
    assert!(close(&m, &json!([0, 2, -1, 0, 15, -13])), "{m}");
    let centre = |m: &Value, p: [f64; 2]| {
        let n = |i: usize| m[i].as_f64().unwrap();
        [n(0) * p[0] + n(2) * p[1] + n(4), n(1) * p[0] + n(3) * p[1] + n(5)]
    };
    assert!(close(&json!(centre(&m, [10.0, 0.0])), &json!([15, 7])), "the pivot moves by the position only");
    // Only the first page carries the map.
    let snapshot = current(&owner);
    let mut later = view(&snapshot, "vertices", None, 2);
    later["payload"]["view"]["cursor"] = json!(2);
    let response = snapshot.dispatch_with_rig_view(serde_json::from_value(later).unwrap(), Ok(PROOF)).unwrap();
    assert_eq!(response.result.unwrap()["matrix"], Value::Null);
}

#[test]
fn an_unmoved_layer_has_no_map_and_the_preview_of_a_moved_one_matches_its_edit() {
    assert_eq!(read(&arm(|_| {}), "vertices", None)["matrix"], Value::Null);
    let mut owner = arm(motion(30.0, [100.0, 100.0], [0.0, 0.0]));
    let held = json!({"segment":{"boneId":"bone2","vi":1,"part":"point","to":[18,12]}});
    let previewed = read(&owner, "posed", Some(held));
    assert!(previewed["matrix"].is_array(), "the posed bounds move the pivot, so the preview carries its own map");
    run_ok(&mut owner, "move", payload(json!({"kind":"moveSegment","boneId":"bone2","vi":1,"part":"point","to":[18,12]})));
    let after = read(&owner, "posed", None);
    assert!(close(&previewed["matrix"], &after["matrix"]), "{} vs {}", previewed["matrix"], after["matrix"]);
}

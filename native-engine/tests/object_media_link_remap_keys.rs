//! `editTimeRemapKeys` through the real staged dispatcher: the selected Time Remap keys of one video placement take one
//! property key action (shift, delete, tint, curve) as ONE revision and ONE history entry, persist in the saved
//! project, and every refusal leaves revision, history and file untouched.
use super::*;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

fn owner() -> NativeObjectHistory {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}}],
        "assetInventory":{"assetIds":["v1","a1"],"placements":[
            {"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":5},
            {"id":"p2","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"M"},"offsetFrames":0}]}});
    NativeObjectHistory::from_project_file("remap-keys", wire.to_string().as_bytes()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let pin = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), pin.snapshot_id())
        .unwrap()
}
fn placement(o: &NativeObjectHistory, index: usize) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["assetInventory"]["placements"][index]
        .clone()
}
fn frames(o: &NativeObjectHistory) -> Vec<Value> {
    placement(o, 0)["timeRemap"]["keys"]
        .as_array()
        .map(|keys| keys.iter().map(|k| k["frame"].clone()).collect())
        .unwrap_or_default()
}
fn dispatch(o: &mut NativeObjectHistory, id: &str, payload: Value) -> Value {
    let request = json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"operation":OP_PLACEMENT_EDIT,
        "expectedRevision":o.content_revision(),"payload":payload});
    let typed: OpacityRequest = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn keys_edit(id: &str, selections: Value, action: Value) -> Value {
    json!({"kind":"editTimeRemapKeys","placementId":id,"selections":selections,"action":action})
}
// Three keys on p1: frames 0, 5 and 10 with values 0, 7 and 19.
fn keyed() -> NativeObjectHistory {
    let mut o = owner();
    let enable = json!({"kind":"enableTimeRemap","placementId":"p1","inFrame":0,"outFrame":10,"lastSourceFrame":19});
    assert_eq!(dispatch(&mut o, "e1", enable)["result"]["applied"], true);
    let key = json!({"kind":"setTimeRemapKey","placementId":"p1","frame":5,"value":7});
    assert_eq!(dispatch(&mut o, "k1", key)["result"]["applied"], true);
    assert_eq!(frames(&o), [json!(0), json!(5), json!(10)]);
    o
}

#[test]
fn shifting_selected_remap_keys_is_one_undoable_transaction_that_persists() {
    let mut o = keyed();
    let before = placement(&o, 0);
    let revision = o.content_revision();
    let shift = keys_edit(
        "p1",
        json!([{"ranges":[[5, 10]]}]),
        json!({"kind":"shift","delta":2}),
    );
    let reply = dispatch(&mut o, "s1", shift);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(o.content_revision(), revision + 1);
    assert_eq!(frames(&o), [json!(0), json!(7), json!(12)]);
    let moved = placement(&o, 0);
    assert_eq!(moved["timeRemap"]["keys"][1]["v"], json!([7]));
    assert_eq!(moved["timeRemap"]["keys"][2]["v"], json!([19]));
    assert_eq!(
        moved["offsetFrames"], 5,
        "the offset and the other placement are untouched"
    );
    assert_eq!(placement(&o, 1)["offsetFrames"], 0);
    assert_eq!(o.history_depths(), (3, 0));
    // The saved file reopens with the moved keys.
    let reopened = NativeObjectHistory::from_project_file("reopen", &saved(&o)).unwrap();
    assert_eq!(placement(&reopened, 0), moved);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(placement(&o, 0), before);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(placement(&o, 0), moved);
}

#[test]
fn deleting_keys_keeps_the_rest_and_deleting_every_key_turns_the_remap_off() {
    let mut o = keyed();
    let three = placement(&o, 0);
    let delete = keys_edit("p1", json!([{"frame":5}]), json!({"kind":"delete"}));
    assert_eq!(dispatch(&mut o, "d1", delete)["result"]["applied"], true);
    assert_eq!(frames(&o), [json!(0), json!(10)]);
    let all = keys_edit(
        "p1",
        json!([{"ranges":[[0, 10]]}]),
        json!({"kind":"delete"}),
    );
    assert_eq!(dispatch(&mut o, "d2", all)["result"]["applied"], true);
    let off = placement(&o, 0);
    assert!(
        off.get("timeRemap").is_none(),
        "no keys left: the remap is disabled"
    );
    assert_eq!(off["offsetFrames"], 5);
    assert_eq!(o.history_depths(), (4, 0));
    o.undo(o.content_revision()).unwrap();
    assert_eq!(frames(&o), [json!(0), json!(10)]);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(placement(&o, 0), three);
    let reopened = NativeObjectHistory::from_project_file("reopen", &saved(&o)).unwrap();
    assert_eq!(placement(&reopened, 0), three);
}

#[test]
fn tint_hold_and_a_repeated_no_op_edit_the_selection_only() {
    let mut o = keyed();
    let tint = keys_edit(
        "p1",
        json!([{"frame":0},{"frame":10}]),
        json!({"kind":"color","color":"#4EA9FF"}),
    );
    assert_eq!(
        dispatch(&mut o, "c1", tint.clone())["result"]["applied"],
        true
    );
    let tinted = placement(&o, 0);
    assert_eq!(tinted["timeRemap"]["keys"][0]["color"], "#4ea9ff");
    assert!(tinted["timeRemap"]["keys"][1].get("color").is_none());
    assert_eq!(tinted["timeRemap"]["keys"][2]["color"], "#4ea9ff");
    // The same tint again changes nothing: no revision, no history entry.
    let again = dispatch(&mut o, "c2", tint);
    assert_eq!(
        again["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.history_depths(), (3, 0));
    let hold = keys_edit(
        "p1",
        json!([{"frame":5}]),
        json!({"kind":"preset","preset":"hold"}),
    );
    assert_eq!(dispatch(&mut o, "h1", hold)["result"]["applied"], true);
    assert_eq!(placement(&o, 0)["timeRemap"]["keys"][1]["hold"], true);
    let clear = keys_edit(
        "p1",
        json!([{"frame":0}]),
        json!({"kind":"color","color":null}),
    );
    assert_eq!(dispatch(&mut o, "c3", clear)["result"]["applied"], true);
    assert!(placement(&o, 0)["timeRemap"]["keys"][0]
        .get("color")
        .is_none());
    o.undo(o.content_revision()).unwrap();
    assert_eq!(placement(&o, 0)["timeRemap"]["keys"][0]["color"], "#4ea9ff");
}

#[test]
fn stale_unknown_and_unsupported_edits_refuse_without_touching_anything() {
    let mut o = keyed();
    let before = (o.content_revision(), o.history_depths(), saved(&o));
    let delete = json!({"kind":"delete"});
    for (id, payload, code) in [
        (
            "unknown-frame",
            keys_edit("p1", json!([{"frame":6}]), delete.clone()),
            "not_found",
        ),
        (
            "unknown-range",
            keys_edit("p1", json!([{"ranges":[[20, 30]]}]), delete.clone()),
            "not_found",
        ),
        (
            "unknown-placement",
            keys_edit("zz", json!([{"frame":0}]), delete.clone()),
            "not_found",
        ),
        (
            "no-remap",
            keys_edit("p2", json!([{"frame":0}]), delete.clone()),
            "invalid_request",
        ),
        (
            "both",
            keys_edit("p1", json!([{"frame":0,"ranges":[[0, 1]]}]), delete.clone()),
            "invalid_request",
        ),
        (
            "neither",
            keys_edit("p1", json!([{}]), delete.clone()),
            "invalid_request",
        ),
        (
            "empty",
            keys_edit("p1", json!([]), delete.clone()),
            "invalid_request",
        ),
        (
            "repeated",
            keys_edit("p1", json!([{"frame":0},{"frame":0}]), delete.clone()),
            "invalid_request",
        ),
        (
            "backwards",
            keys_edit("p1", json!([{"ranges":[[10, 0]]}]), delete.clone()),
            "invalid_request",
        ),
        (
            "unknown-action",
            keys_edit("p1", json!([{"frame":0}]), json!({"kind":"bake"})),
            "invalid_request",
        ),
        (
            "not-an-object",
            keys_edit("p1", json!([{"frame":0}]), json!("delete")),
            "invalid_request",
        ),
        (
            "subdivide",
            keys_edit(
                "p1",
                json!([{"frame":0},{"frame":5}]),
                json!({"kind":"subdivide"}),
            ),
            "unavailable",
        ),
        (
            "before-the-start",
            keys_edit(
                "p1",
                json!([{"frame":0}]),
                json!({"kind":"shift","delta":-1}),
            ),
            "unavailable",
        ),
        (
            "past-the-end",
            keys_edit(
                "p1",
                json!([{"frame":10}]),
                json!({"kind":"shift","delta":110}),
            ),
            "unavailable",
        ),
        (
            "onto-a-key",
            keys_edit(
                "p1",
                json!([{"frame":5}]),
                json!({"kind":"shift","delta":5}),
            ),
            "unavailable",
        ),
        (
            "bad-color",
            keys_edit(
                "p1",
                json!([{"frame":0}]),
                json!({"kind":"color","color":"blue"}),
            ),
            "invalid_request",
        ),
        (
            "extra-field",
            json!({"kind":"editTimeRemapKeys","placementId":"p1","selections":[{"frame":0}],"action":delete,"x":1}),
            "invalid_request",
        ),
    ] {
        let response = dispatch(&mut o, id, payload);
        assert_eq!(response["ok"], false, "{id}: {response}");
        assert_eq!(response["error"]["code"], code, "{id}");
        assert_eq!(
            (o.content_revision(), o.history_depths(), saved(&o)),
            before,
            "{id}"
        );
    }
}

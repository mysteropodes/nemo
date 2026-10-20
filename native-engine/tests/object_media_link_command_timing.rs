//! T12 through the real staged dispatcher: video timing edits are one undoable
//! transaction each, persist in the saved project, and refuse malformed payloads
//! and non-video placements without touching revision, history or the file.
use super::*;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};

fn owner() -> NativeObjectHistory {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}}],
        "assetInventory":{"assetIds":["v1","a1"],"placements":[
            {"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":5}]}});
    NativeObjectHistory::from_project_file("placement-timing", wire.to_string().as_bytes()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let pin = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), pin.snapshot_id())
        .unwrap()
}
fn placement(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["assetInventory"]["placements"][0].clone()
}
fn dispatch(o: &mut NativeObjectHistory, id: &str, payload: Value) -> Value {
    let request = json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"operation":OP_PLACEMENT_EDIT,
        "expectedRevision":o.content_revision(),"payload":payload});
    let typed: OpacityRequest = serde_json::from_value(request).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn enable(in_frame: i64, out_frame: i64, last: i64) -> Value {
    json!({"kind":"enableTimeRemap","placementId":"p1","inFrame":in_frame,"outFrame":out_frame,"lastSourceFrame":last})
}
fn mode(mode: &str) -> Value {
    json!({"kind":"setInterpMode","placementId":"p1","interpMode":mode})
}

#[test]
fn timing_edits_persist_and_undo_redo_one_transaction_each() {
    let mut o = owner();
    assert_eq!(
        dispatch(&mut o, "e1", enable(0, 10, 19))["result"]["applied"],
        true
    );
    assert_eq!(
        dispatch(&mut o, "m1", mode("blend"))["result"]["applied"],
        true
    );
    let remove = json!({"kind":"removeTimeRemapKey","placementId":"p1","frame":10});
    assert_eq!(dispatch(&mut o, "k1", remove)["result"]["applied"], true);
    let timed = placement(&o);
    let linear = json!([{"x":0,"y":0},{"x":1,"y":1}]);
    assert_eq!(
        timed["timeRemap"],
        json!({"keys":[{"frame":0,"v":[0],"curvePoints":linear,"hIn":[0,0],"hOut":[0,0]}]})
    );
    assert_eq!(timed["interpMode"], "blend");
    assert_eq!(timed["frameBlend"], true);
    assert_eq!(timed["offsetFrames"], 5);
    assert_eq!(o.history_depths(), (3, 0));
    // The saved file reopens with the same timing.
    let reopened = NativeObjectHistory::from_project_file("reopen", &saved(&o)).unwrap();
    assert_eq!(placement(&reopened), timed);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(
        placement(&o)["timeRemap"]["keys"].as_array().unwrap().len(),
        2
    );
    o.undo(o.content_revision()).unwrap();
    assert!(placement(&o).get("interpMode").is_none());
    assert!(placement(&o).get("frameBlend").is_none());
    o.redo(o.content_revision()).unwrap();
    o.redo(o.content_revision()).unwrap();
    assert_eq!(placement(&o), timed);
    // The same value again is applied:false with no history entry; a second
    // enable is refused like the baseline's "already active".
    let same = dispatch(&mut o, "m2", mode("blend"));
    assert_eq!(
        same["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(dispatch(&mut o, "e2", enable(0, 4, 2))["ok"], false);
    let disable = json!({"kind":"disableTimeRemap","placementId":"p1"});
    assert_eq!(dispatch(&mut o, "d1", disable)["result"]["applied"], true);
    assert!(placement(&o).get("timeRemap").is_none());
    assert_eq!(placement(&o)["interpMode"], "blend");
}

#[test]
fn malformed_payloads_and_non_video_placements_leave_everything_untouched() {
    let mut o = owner();
    let audio = json!({"kind":"place","placement":{"id":"p2","assetId":"a1",
        "target":{"kind":"audioTrack","trackUid":"t1"},"offsetFrames":0}});
    let placed = dispatch(&mut o, "place-audio", audio);
    assert_eq!(
        placed["ok"], false,
        "the audio track owner is absent in this fixture"
    );
    let before = (o.content_revision(), o.history_depths(), saved(&o));
    for (id, payload) in [
        (
            "missing",
            json!({"kind":"enableTimeRemap","placementId":"p1","inFrame":0,"outFrame":9}),
        ),
        (
            "string",
            json!({"kind":"enableTimeRemap","placementId":"p1","inFrame":"0","outFrame":9,"lastSourceFrame":3}),
        ),
        (
            "whole-track",
            json!({"kind":"setTimeRemap","placementId":"p1","timeRemap":null}),
        ),
        (
            "no-remap",
            json!({"kind":"removeTimeRemapKey","placementId":"p1","frame":0}),
        ),
        (
            "disable-extra",
            json!({"kind":"disableTimeRemap","placementId":"p1","frame":0}),
        ),
        (
            "extra",
            json!({"kind":"setInterpMode","placementId":"p1","interpMode":"blend","x":1}),
        ),
        (
            "number",
            json!({"kind":"setInterpMode","placementId":"p1","interpMode":1}),
        ),
        ("empty", mode("")),
        (
            "unknown",
            json!({"kind":"setInterpMode","placementId":"zz","interpMode":"blend"}),
        ),
    ] {
        let response = dispatch(&mut o, id, payload);
        assert_eq!(response["ok"], false, "{id}");
        assert_eq!(
            (o.content_revision(), o.history_depths(), saved(&o)),
            before,
            "{id}"
        );
    }
}

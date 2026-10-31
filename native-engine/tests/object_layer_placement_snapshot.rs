//! T12 placement view through the real immutable reader and the existing timing-query dispatcher.
use super::*;
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;

fn owner(timing: Value) -> NativeObjectHistory {
    let mut placement = json!({"id":"p1","assetId":"v1",
        "target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":5});
    for (k, v) in timing.as_object().unwrap() {
        placement[k] = v.clone();
    }
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}}],
        "assetInventory":{"assetIds":["v1","a1"],"placements":[placement]}});
    NativeObjectHistory::from_project_file("placement-read", wire.to_string().as_bytes()).unwrap()
}
fn query(s: &ObjectSnapshot, layer: &str, frame: u32) -> Value {
    json!({"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "contextId":"scene-root","layerUid":layer,"frame":frame,"view":"placement"})
}
fn read(s: &ObjectSnapshot, q: &Value) -> Result<Value, DispatchErrorCode> {
    s.query_layer_placement_json(q.to_string().as_bytes())
}
fn snapshot(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}

#[test]
fn a_video_layer_reports_its_placement_its_remap_keys_and_the_eased_source_frame() {
    let remap = json!({"timeRemap":{"keys":[{"frame":0,"v":[0],
        "curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},{"frame":24,"v":[47]}]},
        "interpMode":"blend","frameBlend":true});
    let s = snapshot(&owner(remap));
    let value = read(&s, &query(&s, "L", 24)).unwrap();
    assert_eq!(
        value,
        json!({"kind":"layerPlacement","instanceId":s.instance_id(),"documentId":s.document_id(),
            "atRevision":0,"documentSnapshotId":s.snapshot_id(),"contextId":"scene-root",
            "layerUid":"L","frame":24,"placement":{"placementId":"p1","assetId":"v1",
            "assetKind":"video","offsetFrames":5,"timeRemap":{"keyCount":2,"keyAtFrame":true,
            "value":47.0,"keys":[{"frame":0,"value":0},{"frame":24,"value":47}]},
            "interpMode":"blend","frameBlend":true,"sourceFrameCount":null}})
    );
    let between = read(&s, &query(&s, "L", 3)).unwrap();
    assert_eq!(between["placement"]["timeRemap"]["keyAtFrame"], false);
    // The linear first key: frame 6 of 24 maps to a quarter of 47 source frames.
    let eased = read(&s, &query(&s, "L", 6)).unwrap();
    assert_eq!(eased["placement"]["timeRemap"]["value"], json!(11.75));
}

#[test]
fn a_key_list_past_the_read_budget_is_dropped_and_the_counted_facts_stay() {
    let keys: Vec<Value> = (0..300)
        .map(|i| json!({"frame":i,"v":[f64::from(i) + 0.123_456_789_012]}))
        .collect();
    let s = snapshot(&owner(json!({"timeRemap":{"keys":keys}})));
    let value = read(&s, &query(&s, "L", 7)).unwrap();
    let remap = &value["placement"]["timeRemap"];
    assert_eq!(remap["keys"], Value::Null);
    assert_eq!(remap["keyCount"], 300);
    assert_eq!(remap["keyAtFrame"], true);
    assert_eq!(remap["value"], json!(7.123_456_789_012));
    assert!(value.to_string().len() <= MAX_MESSAGE_BYTES - HOST_FILL_BYTES);
}

#[test]
fn absent_timing_and_layers_without_video_stay_distinct() {
    let s = snapshot(&owner(json!({})));
    let plain = read(&s, &query(&s, "L", 0)).unwrap()["placement"].clone();
    assert_eq!(plain["timeRemap"], Value::Null);
    assert_eq!(plain["interpMode"], Value::Null);
    assert_eq!(plain["frameBlend"], Value::Null);
    assert_eq!(plain["sourceFrameCount"], Value::Null);
    assert_eq!(
        read(&s, &query(&s, "M", 0)).unwrap()["placement"],
        Value::Null
    );
}

#[test]
fn stale_unknown_out_of_range_and_other_context_selectors_are_refused() {
    let s = snapshot(&owner(json!({})));
    let mut stale = query(&s, "L", 0);
    stale["atRevision"] = json!(1);
    assert_eq!(read(&s, &stale), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(
        read(&s, &query(&s, "Z", 0)),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        read(&s, &query(&s, "L", 120)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut context = query(&s, "L", 0);
    context["contextId"] = json!("component:x");
    assert_eq!(read(&s, &context), Err(DispatchErrorCode::Unavailable));
    let mut extra = query(&s, "L", 0);
    extra["placementId"] = json!("p1");
    assert_eq!(read(&s, &extra), Err(DispatchErrorCode::InvalidRequest));
}

#[test]
fn the_timing_dispatcher_routes_the_placement_view_and_refuses_an_unknown_view() {
    let s = snapshot(&owner(json!({})));
    let request = |payload: Value| -> OpacityRequest {
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":"r","instanceId":s.instance_id(),
            "documentId":s.document_id(),"operation":"query.document.object.layer.timing",
            "payload":payload}),
        )
        .unwrap()
    };
    let routed = s
        .dispatch_layer_timing(&request(query(&s, "L", 0)))
        .unwrap();
    let routed = serde_json::to_value(routed).unwrap();
    assert_eq!(routed["ok"], true);
    assert_eq!(routed["result"]["kind"], "layerPlacement");
    let mut other = query(&s, "L", 0);
    other["view"] = json!("bogus");
    let refused = serde_json::to_value(s.dispatch_layer_timing(&request(other)).unwrap()).unwrap();
    assert_eq!(refused["ok"], false);
}

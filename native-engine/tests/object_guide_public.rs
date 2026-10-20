//! Real public guide envelopes on the existing revision/history and snapshot owners.
//! Source-only until a separately booked compile/runtime and profile registration.
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const EDIT: &str = "command.document.object.project.guide.edit";
const READ: &str = "query.document.object.project.guide.read";

fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}});
    NativeObjectHistory::from_project_file("public-guide", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":EDIT,
        "payload":{"command":"object.project.guide.edit","action":action}}),
    )
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, q: OpacityRequest) -> Value {
    serde_json::to_value(
        o.dispatch_object_command(&q)
            .unwrap()
            .expect("registered guide edit"),
    )
    .unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let q = command(o, id, action);
    dispatch(o, q)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":READ,"payload":{"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector}}),
    )
    .unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let result = serde_json::to_value(p.dispatch(q).unwrap()).unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 4096);
    result
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "guide-read", selector))
}
#[test]
fn guide_move_replay_old_snapshot_history_and_fresh_owner_reopen_preserve_other_metadata() {
    let mut o = owner(json!({"guides":{"h":[-12.25,80.5]},"workspace":{"panelOrder":null},
        "comments":[{"text":"Keep","authorName":"Writer"}],"bpm":137.5}));
    let old = pin(&o); let before = saved(&o);
    let q = command(&o,"guide-move",json!({"kind":"move","axis":"h","ordinal":1,"position":-7.25}));
    let response = dispatch(&mut o,q.clone());
    assert!(response["ok"].as_bool().unwrap());
    assert_eq!(response["result"],json!({"applied":true,"historyEntriesAdded":1}));
    assert_eq!(dispatch(&mut o,q),response); assert_eq!(o.content_revision(),1); assert_eq!(o.history_depths(),(1,0));
    let changed = saved(&o); let after:Value=serde_json::from_slice(&changed).unwrap();
    let initial:Value=serde_json::from_slice(&before).unwrap();
    for field in ["workspace","comments","bpm"] { assert_eq!(after["project"][field],initial["project"][field]); }
    assert_eq!(after["drawing"],initial["drawing"]); assert_eq!(after["project"]["guides"],json!({"h":[-7.25,80.5]}));
    assert_eq!(query(&old,json!({"kind":"axisPage","axis":"h","cursor":0,"limit":16}))["result"]["result"]["items"][0]["position"],-12.25);
    o.undo(1).unwrap(); assert_eq!(saved(&o),before);
    let noop=edit(&mut o,"guide-noop",json!({"kind":"move","axis":"h","ordinal":1,"position":-12.25}));
    assert_eq!(noop["result"],json!({"applied":false,"historyEntriesAdded":0})); assert_eq!(o.content_revision(),2); assert_eq!(o.history_depths(),(0,1));
    o.redo(2).unwrap(); assert_eq!(saved(&o),changed);
    let fresh=NativeObjectHistory::from_project_file("guide-reopen",&changed).unwrap(); assert_eq!(saved(&fresh),changed);
}

#[test]
fn guide_absence_empty_axes_add_and_remove_do_not_materialize_unrelated_defaults() {
    for (extra,object,h) in [(json!({}),false,false),(json!({"guides":{}}),true,false),(json!({"guides":{"h":[]}}),true,true)] {
        let mut o=owner(extra); let before=saved(&o);
        let settings=query(&pin(&o),json!({"kind":"settings"}));
        assert_eq!(settings["result"]["result"],json!({"guidesPresent":object,"hPresent":h,"vPresent":false,"hCount":0,"vCount":0}));
        assert_eq!(saved(&o),before);
        let response=edit(&mut o,"guide-add",json!({"kind":"add","axis":"v","position":-8.25})); assert_eq!(response["contentRevision"],1);
        let response=edit(&mut o,"guide-remove",json!({"kind":"remove","axis":"v","ordinal":1})); assert_eq!(response["contentRevision"],2);
        let value:Value=serde_json::from_slice(&saved(&o)).unwrap();
        assert_eq!(value["project"]["guides"].get("h").is_some(),h);
        assert_eq!(value["project"]["guides"]["v"],json!([]));
    }
}

#[test]
fn guide_public_pages_and_refusals_are_pinned_complete_and_atomic() {
    let values:Vec<f64>=(0..35).map(|i|i as f64-12.25).collect();
    let mut o=owner(json!({"guides":{"h":values}})); let before=saved(&o); let p=pin(&o);
    let mut cursor=0; let mut positions=Vec::new();
    loop {
        let response=query(&p,json!({"kind":"axisPage","axis":"h","cursor":cursor,"limit":16})); assert_eq!(response["ok"],true);
        let result=&response["result"]["result"];
        for item in result["items"].as_array().unwrap() { assert_eq!(item["ordinal"].as_u64(),Some(positions.len() as u64+1)); positions.push(item["position"].as_f64().unwrap()); }
        if result["nextCursor"].is_null(){break;} let next=result["nextCursor"].as_u64().unwrap(); assert!(next>cursor);cursor=next;
    }
    assert_eq!(positions,values);
    for action in [json!({"kind":"move","axis":"h","ordinal":0,"position":1}),json!({"kind":"move","axis":"h","ordinal":1,"position":null}),
        json!({"kind":"remove","axis":"x","ordinal":1}),json!({"kind":"add","axis":"h","position":1,"values":[]}),json!({"kind":"replaceGuides","h":[]})] {
        let response=edit(&mut o,"bad-guide",action);assert_eq!(response["ok"],false);assert_eq!(saved(&o),before);assert_eq!(o.history_depths(),(0,0));
    }
    let q=command(&o,"stale-guide",json!({"kind":"add","axis":"h","position":2.5}));
    let mut foreign=q.clone();foreign.instance_id="foreign".into();assert_eq!(dispatch(&mut o,foreign)["ok"],false);
    let mut foreign=q.clone();foreign.document_id="foreign".into();assert_eq!(dispatch(&mut o,foreign)["ok"],false);
    let mut stale=q;stale.expected_revision=Some(1);assert_eq!(dispatch(&mut o,stale)["ok"],false);assert_eq!(saved(&o),before);
    let mut stale=request(&p,"guide-stale-read",json!({"kind":"settings"}));stale.payload["documentSnapshotId"]=json!("wrong");assert_eq!(read(&p,stale)["error"]["code"],"stale_revision");
}

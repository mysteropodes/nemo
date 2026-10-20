//! Actual common public envelope, revision owner/history and pinned snapshot routes.
use crate::history::NativeObjectHistory;
use crate::object_document::ObjectDocument;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json,Value};
const EDIT:&str="command.document.object.project.composition.edit";
const READ:&str="query.document.object.project.composition.read";
fn drawing()->Value { json!({"format":"nemo.native-object-document","formatVersion":1,
    "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}) }
fn owner(extra:Value)->NativeObjectHistory {
    let mut project=json!({"canvasW":320,"canvasH":180,"fps":24});project.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let wire=json!({"format":"nemo.native-project-document","formatVersion":1,"project":project,
        "drawing":drawing(),"viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("public-composition",wire.to_string().as_bytes()).unwrap()
}
fn pin(o:&NativeObjectHistory)->ObjectSnapshot {o.acquire_snapshot(o.content_revision()).unwrap()}
fn command(o:&NativeObjectHistory,id:&str,action:Value)->OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":EDIT,"payload":{"command":"object.project.composition.edit","action":action}})).unwrap()
}
fn dispatch(o:&mut NativeObjectHistory,q:OpacityRequest)->Value {
    serde_json::to_value(o.dispatch_object_command(&q).unwrap().expect("registered composition command")).unwrap()
}
fn request(p:&ObjectSnapshot,id:&str,selector:Value)->OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),"documentId":p.document_id(),
        "operation":READ,"payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":selector}})).unwrap()
}
fn query(p:&ObjectSnapshot,q:OpacityRequest)->Value {serde_json::to_value(p.dispatch(q).unwrap()).unwrap()}
fn saved(o:&NativeObjectHistory)->Vec<u8>{let p=pin(o);o.serialize_project_file(p.content_revision(),p.snapshot_id()).unwrap()}

#[test]
fn public_composition_delta_replays_one_history_entry_and_reads_the_same_native_revision() {
    let mut o=owner(json!({}));let old=pin(&o);let q=command(&o,"add-1",json!({"kind":"addMarker","frame":25.5,"name":"","color":""}));
    let applied=dispatch(&mut o,q.clone());assert_eq!(applied["ok"],true);assert_eq!(applied["contentRevision"],1);
    assert_eq!(applied["result"],json!({"applied":true,"historyEntriesAdded":1}));assert_eq!(o.history_depths(),(1,0));
    assert_eq!(dispatch(&mut o,q),applied);assert_eq!(o.history_depths(),(1,0));assert_eq!(o.content_revision(),1);
    let current=pin(&o);let read=query(&current,request(&current,"page",json!({"kind":"markerPage","cursor":0,"limit":16})));
    assert_eq!(read["ok"],true);assert_eq!(read["contentRevision"],1);
    assert_eq!(read["result"]["documentSnapshotId"],current.snapshot_id());
    assert_eq!(read["result"]["result"]["items"],json!([{"ordinal":1,"frame":25.5,"name":"","color":""}]));
    assert!(old.project().unwrap().project().composition_markers().is_none());
}

#[test]
fn public_noop_retains_redo_and_original_settings_action_namespace_stays_closed() {
    let mut o=owner(json!({}));let q=command(&o,"add",json!({"kind":"addMarker","frame":2}));dispatch(&mut o,q);
    o.undo(1).unwrap();assert_eq!(o.history_depths(),(0,1));let q=command(&o,"noop",json!({"kind":"setBpm"}));
    let r=dispatch(&mut o,q);assert_eq!(r["result"],json!({"applied":false,"historyEntriesAdded":0}));assert_eq!(o.history_depths(),(0,1));
    let mut q=command(&o,"wrong-old",json!({"kind":"addMarker","frame":1}));q.operation="command.document.object.project.settings.edit".into();q.payload["command"]=json!("object.project.settings.edit");
    assert_eq!(dispatch(&mut o,q)["error"]["code"],"invalid_request");
    let q=command(&o,"wrong-new",json!({"kind":"setCanvasSize","width":640,"height":360}));
    assert_eq!(dispatch(&mut o,q)["error"]["code"],"invalid_request");assert_eq!(o.history_depths(),(0,1));
    o.redo(2).unwrap();assert_eq!(pin(&o).project().unwrap().project().composition_markers().unwrap()[0].0,2.0);
}

#[test]
fn actual_public_delta_history_and_file_reopen_retain_all_other_metadata_groups() {
    let extra=json!({"workspace":{"panelOrder":null},"palettes":[{"id":"p","name":"P","colors":["#AbC"]}],
        "guides":{"h":[-0.25]},"comments":[{"text":"Comment","authorName":"Writer"}],
        "customBrushPresets":{"p":{"label":"Preset","nibSize":0.5}}});
    let mut o=owner(extra.clone());let before=saved(&o);let q=command(&o,"tempo",json!({"kind":"setBpm","bpm":137.5,"bpmShow":false}));
    let result=dispatch(&mut o,q);assert_eq!(result["ok"],true);let after=saved(&o);
    let wire:Value=serde_json::from_slice(&after).unwrap();for (key,value) in extra.as_object().unwrap(){assert_eq!(&wire["project"][key],value);}
    let current=pin(&o);let settings=query(&current,request(&current,"tempo-read",json!({"kind":"settings"})));
    assert_eq!(settings["result"]["result"]["bpm"],137.5);assert!(settings["result"]["result"].get("bpmOffset").is_none());
    let reopened=NativeObjectHistory::from_project_file("public-fresh",&after).unwrap();assert_eq!(saved(&reopened),after);
    o.undo(1).unwrap();assert_eq!(saved(&o),before);o.redo(2).unwrap();assert_eq!(saved(&o),after);
}

#[path = "object_composition_public_reads.rs"]
mod public_reads;

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

#[test]
fn common_public_read_guards_refuse_foreign_stale_cancelled_and_nonroot_reads() {
    let mut o=owner(json!({}));let old=pin(&o);let q=command(&o,"tempo",json!({"kind":"setBpm","bpm":132}));dispatch(&mut o,q);let current=pin(&o);
    let stale=request(&old,"stale",json!({"kind":"settings"}));assert_eq!(query(&current,stale)["error"]["code"],"stale_revision");
    for (field,value,code) in [("instanceId",json!("foreign"),"wrong_instance"),("documentId",json!("foreign"),"wrong_document"),
        ("cancelledBeforeDispatch",json!(true),"cancelled_before_dispatch"),("expectedRevision",json!(1),"invalid_request")] {
        let mut raw=serde_json::to_value(request(&current,"guard",json!({"kind":"settings"}))).unwrap();raw[field]=value;
        assert_eq!(query(&current,serde_json::from_value(raw).unwrap())["error"]["code"],code);
    }
    let mut q=request(&current,"context",json!({"kind":"settings"}));q.payload["contextId"]=json!("component");
    assert_eq!(query(&current,q)["error"]["code"],"unavailable");assert_eq!(o.history_depths(),(1,0));
}

#[test]
fn public_marker_pages_charge_complete_envelopes_and_deliver_all_raw_records() {
    let markers:Vec<Value>=(0..7).map(|i|json!({"frame":i as f64/2.0,"name":"é".repeat(512)})).collect();
    let o=owner(json!({"markers":markers}));let current=pin(&o);let mut cursor=0;let mut actual=Vec::new();
    loop {
        let q=request(&current,&"r".repeat(128),json!({"kind":"markerPage","cursor":cursor,"limit":16}));
        let response=current.dispatch(q).unwrap();assert!(serde_json::to_vec(&response).unwrap().len()<=4096);
        let value=serde_json::to_value(response).unwrap();assert_eq!(value["ok"],true);let page=&value["result"]["result"];
        let items=page["items"].as_array().unwrap();assert!(!items.is_empty()&&items.len()<7);actual.extend(items.clone());
        match page["nextCursor"].as_u64(){Some(next)=>{assert!(next>cursor);cursor=next;},None=>break}
    }
    assert_eq!(actual.len(),7);for (i,item) in actual.iter().enumerate(){assert_eq!(item,&json!({"ordinal":i+1,"frame":i as f64/2.0,"name":"é".repeat(512)}));}
}

#[test]
fn common_public_caps_and_closed_actions_refuse_malformed_deltas_without_partial_writes() {
    let mut o=owner(json!({}));let before=saved(&o);
    for (i,action) in [json!([]),json!({"kind":"setCompositionMetadata","markers":[]}),json!({"kind":"setBpm","bpm":null}),
        json!({"kind":"setBpm","unknown":true}),json!({"kind":"removeMarker","ordinal":0}),
        json!({"kind":"addMarker","frame":1,"name":"x".repeat(4096)})].into_iter().enumerate() {
        let q=command(&o,&format!("invalid-{i}"),action);assert_eq!(dispatch(&mut o,q)["error"]["code"],"invalid_request");
        assert_eq!(saved(&o),before);assert_eq!(o.history_depths(),(0,0));
    }
}

#[test]
fn public_bare_drawing_metadata_is_unavailable_and_escaped_oversize_record_refuses() {
    let document:ObjectDocument=serde_json::from_value(drawing()).unwrap();let mut bare=NativeObjectHistory::new("bare",document).unwrap();
    let q=command(&bare,"bare-edit",json!({"kind":"setBpm","bpm":132}));assert_eq!(dispatch(&mut bare,q)["error"]["code"],"unavailable");
    let current=pin(&bare);assert_eq!(query(&current,request(&current,"bare-read",json!({"kind":"settings"})))["error"]["code"],"unavailable");
    let o=owner(json!({"markers":[{"frame":1,"name":"\u{1}".repeat(1024)}]}));let current=pin(&o);
    assert_eq!(query(&current,request(&current,"too-wide",json!({"kind":"markerPage","cursor":0,"limit":16})))["error"]["code"],"unavailable");
}

//! Common pinned-read and complete-envelope refusal controls.
use super::*;

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

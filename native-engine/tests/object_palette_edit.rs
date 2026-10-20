//! Original palette operations on the actual sealed metadata/history owner.
//! Private preparation tests; no public API, UI or installed acceptance inferred.
use crate::history::NativeObjectHistory;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode as Error;
use serde_json::{json,Value};
fn owner(extra:Value)->NativeObjectHistory {
    let mut project=json!({"canvasW":320,"canvasH":180,"fps":24});
    project.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    NativeObjectHistory::from_project_file("palette-fields",json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}}).to_string().as_bytes()).unwrap()
}
fn saved(o:&NativeObjectHistory)->Vec<u8>{let p=o.acquire_snapshot(o.content_revision()).unwrap();o.serialize_project_file(p.content_revision(),p.snapshot_id()).unwrap()}
fn metadata(o:&NativeObjectHistory)->Value{serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"].clone()}
fn payload(action:Value)->Vec<u8>{json!({"command":"object.project.palette.edit","action":action}).to_string().into_bytes()}
fn edit(o:&mut NativeObjectHistory,action:Value)->Result<u64,Error>{let candidate=o.prepare_palette_delta_json(&payload(action))?;o.commit_project_settings(candidate)}
fn fixture()->Value {json!({"palettes":[{"id":"same","name":"First","colors":["#AbC","#abc","#AbC"]},
    {"id":"same","name":"Second","colors":[""]}],"activePaletteIdx":-0.5,"markers":[{"frame":20.5,"name":"M"}],
    "workspace":{"panelOrder":null},"comments":[{"text":"Untouched","authorName":"Author"}]})}
#[test]
fn palette_rename_preserves_raw_order_duplicate_ids_unrelated_fields_history_and_reopen() {
    let mut o=owner(fixture());let before=saved(&o);let old=o.acquire_snapshot(0).unwrap();
    let candidate=o.prepare_palette_delta_json(&payload(json!({"kind":"rename","ordinal":2,"name":"Renamed"}))).unwrap();
    assert_eq!(saved(&o),before);assert_eq!(o.content_revision(),0);
    assert_eq!(o.commit_project_settings(candidate).unwrap(),1);assert_eq!(o.history_depths(),(1,0));let after=saved(&o);
    let mut projected:Value=serde_json::from_slice(&after).unwrap();let original:Value=serde_json::from_slice(&before).unwrap();
    assert_eq!(projected["project"]["palettes"][1]["name"],"Renamed");
    projected["project"]["palettes"]=original["project"]["palettes"].clone();assert_eq!(projected,original);
    assert_eq!(old.project().unwrap().project().palette_metadata().0.unwrap()[1].1,"Second");
    let fresh=NativeObjectHistory::from_project_file("palette-fresh",&after).unwrap();assert_eq!(saved(&fresh),after);
    o.undo(1).unwrap();assert_eq!(saved(&o),before);o.redo(2).unwrap();assert_eq!(saved(&o),after);
}
#[test]
fn explicit_creation_materializes_only_requested_palette_and_duplicate_color_noop_retains_redo() {
    let mut o=owner(json!({}));let absent=saved(&o);
    assert_eq!(o.prepare_palette_delta_json(&payload(json!({"kind":"select","ordinal":1}))).unwrap_err(),Error::NotFound);
    assert_eq!(saved(&o),absent);
    edit(&mut o,json!({"kind":"create","id":"p","name":"New"})).unwrap();
    assert_eq!(metadata(&o)["palettes"],json!([{"id":"p","name":"New","colors":[]}]));
    assert_eq!(metadata(&o)["activePaletteIdx"].as_f64(),Some(0.));
    edit(&mut o,json!({"kind":"appendColor","ordinal":1,"color":"#AbC"})).unwrap();let colored=saved(&o);
    assert_eq!(edit(&mut o,json!({"kind":"appendColor","ordinal":1,"color":"#AbC"})).unwrap(),2);
    o.undo(2).unwrap();let depths=o.history_depths();assert_eq!(depths,(1,1));
    assert_eq!(edit(&mut o,json!({"kind":"rename","ordinal":1,"name":"New"})).unwrap(),3);
    assert_eq!(o.history_depths(),depths);o.redo(3).unwrap();assert_eq!(saved(&o),colored);
}
#[test]
fn large_palette_duplicate_and_color_deltas_use_small_intents_not_whole_array_uploads() {
    let colors:Vec<String>=(0..600).map(|i|format!("#{i:06x}")).collect();
    let mut o=owner(json!({"palettes":[{"id":"source","name":"Source","colors":colors}]}));assert!(saved(&o).len()>4096);
    let delta=payload(json!({"kind":"duplicate","ordinal":1,"id":"copy","name":"Copy"}));assert!(delta.len()<200);
    let candidate=o.prepare_palette_delta_json(&delta).unwrap();o.commit_project_settings(candidate).unwrap();
    assert_eq!(metadata(&o)["palettes"][0]["colors"],json!(colors));assert_eq!(metadata(&o)["palettes"][1]["colors"],json!(colors));
    edit(&mut o,json!({"kind":"appendColor","ordinal":2,"color":"#AbC"})).unwrap();
    edit(&mut o,json!({"kind":"appendColor","ordinal":2,"color":"#abc"})).unwrap();
    let m=metadata(&o);assert_eq!(m["palettes"][0]["colors"].as_array().unwrap().len(),600);
    assert_eq!(&m["palettes"][1]["colors"].as_array().unwrap()[600..],&[json!("#AbC"),json!("#abc")]);
}
#[test]
fn color_reorder_remove_selection_and_last_palette_guard_match_original_operations() {
    let mut o=owner(fixture());edit(&mut o,json!({"kind":"moveColor","ordinal":1,"from":1,"to":3})).unwrap();
    assert_eq!(metadata(&o)["palettes"][0]["colors"],json!(["#abc","#AbC","#AbC"]));
    edit(&mut o,json!({"kind":"removeColor","ordinal":1,"index":2})).unwrap();assert_eq!(metadata(&o)["palettes"][0]["colors"],json!(["#abc","#AbC"]));
    edit(&mut o,json!({"kind":"select","ordinal":2})).unwrap();assert_eq!(metadata(&o)["activePaletteIdx"].as_f64(),Some(1.));
    edit(&mut o,json!({"kind":"remove","ordinal":2})).unwrap();assert_eq!(metadata(&o)["activePaletteIdx"].as_f64(),Some(0.));
    let before=saved(&o);assert_eq!(edit(&mut o,json!({"kind":"remove","ordinal":1})).unwrap_err(),Error::InvalidRequest);assert_eq!(saved(&o),before);
}
#[test]
fn stale_and_foreign_palette_candidates_cannot_publish_or_change_history() {
    let mut o=owner(fixture());let wire=saved(&o);let mut other=NativeObjectHistory::from_project_file("other-palette",&wire).unwrap();
    let change=json!({"kind":"rename","ordinal":1,"name":"Forbidden"});
    let foreign=o.prepare_palette_delta_json(&payload(change.clone())).unwrap();assert!(other.commit_project_settings(foreign).is_err());
    assert_eq!(saved(&other),wire);assert_eq!(other.history_depths(),(0,0));
    let stale=o.prepare_palette_delta_json(&payload(change)).unwrap();edit(&mut o,json!({"kind":"select","ordinal":1})).unwrap();
    let before=saved(&o);let depths=o.history_depths();assert!(o.commit_project_settings(stale).is_err());assert_eq!(saved(&o),before);assert_eq!(o.history_depths(),depths);
}
#[test]
fn malformed_palette_intents_are_closed_duplicate_first_bounded_and_atomic() {
    let mut o=owner(fixture());let before=saved(&o);
    for a in [json!({"kind":"replace","palettes":[]}),json!({"kind":"create","id":"p","name":"x","colors":[]}),
        json!({"kind":"rename","ordinal":1,"name":"x".repeat(1025)}),json!({"kind":"appendColor","ordinal":1,"color":"\0"}),
        json!({"kind":"moveColor","ordinal":1,"from":0,"to":1}),json!({"kind":"select","ordinal":1.5}),
        json!({"kind":"select","ordinal":9_007_199_254_740_992_u64}),json!({"kind":"create","id":"p","name":"x","color":null})] {
        assert_eq!(o.prepare_palette_delta_json(&payload(a)).unwrap_err(),Error::InvalidRequest);assert_eq!(saved(&o),before);
    }
    for raw in [r#"[]"#,r#"{"command":"object.project.palette.edit","action":[]}"#,
        r#"{"command":"object.project.palette.edit","command":"object.project.palette.edit","action":{"kind":"select","ordinal":1}}"#,
        r#"{"command":"object.project.palette.edit","action":{"kind":"appendColor","ordinal":1,"color":"#AbC","color":"#def"}}"#] {
        assert_eq!(o.prepare_palette_delta_json(raw.as_bytes()).unwrap_err(),Error::InvalidRequest);assert_eq!(saved(&o),before);
    }
    assert_eq!(o.prepare_palette_delta_json(&payload(json!({"kind":"rename","ordinal":1,"name":"\u{1}".repeat(1024)}))).unwrap_err(),Error::InvalidRequest);
    assert_eq!(o.history_depths(),(0,0));
}
#[test]
fn palette_provider_is_unavailable_without_project_and_invalid_targets_never_materialize_defaults() {
    let document:ObjectDocument=serde_json::from_value(json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,"layers":[{"layerUid":"L"}],"objects":[]})).unwrap();
    let bare=NativeObjectHistory::new("bare-palette",document).unwrap();assert_eq!(bare.prepare_palette_delta_json(&payload(json!({"kind":"create","id":"p","name":"P"}))).unwrap_err(),Error::Unavailable);
    let mut o=owner(json!({}));let before=saved(&o);assert_eq!(edit(&mut o,json!({"kind":"rename","ordinal":1,"name":"P"})).unwrap_err(),Error::NotFound);assert_eq!(saved(&o),before);
}

#[test]
fn tiny_duplicate_intent_cannot_publish_an_unsaveable_palette_document() {
    let colors=vec!["x".repeat(1024);17_000];
    let mut o=owner(json!({"palettes":[{"id":"large","name":"Large","colors":colors}]}));
    let before=saved(&o);assert!(before.len()<32*1024*1024);
    let delta=payload(json!({"kind":"duplicate","ordinal":1,"id":"copy","name":"Copy"}));assert!(delta.len()<200);
    assert!(o.prepare_palette_delta_json(&delta).is_err());assert_eq!(saved(&o),before);
    assert_eq!(o.content_revision(),0);assert_eq!(o.history_depths(),(0,0));
}

//! Detached whole-table group references must resolve before publication.
use super::object_selection_edit_clipboard::{copy, paste, target, two_tables};
use super::object_selection_edit_tests::{apply, delete, frozen, owner_of, value};
use crate::object_document::object_group_kernels::object_clipboard_plan::ClipPayload;
use crate::object_document::object_group_kernels::object_selection_edit_frames::EditMode;
use serde_json::{json, Value};

#[test]
fn malformed_group_relationships_preserve_bytes_revision_and_existing_redo() {
    let mut owner=owner_of(&two_tables(2));
    let clip=serde_json::to_value(copy(&owner,"L0",0,&["a","b"]).unwrap().unwrap()).unwrap();
    apply(&mut owner,"delete",&delete("L1",0,"motion",&["x"])).unwrap();
    owner.undo(1).unwrap();
    let before=frozen(&owner);
    assert_eq!(owner.history_depths(),(0,1));
    let mut cases=Vec::new();
    let mut bad=clip.clone();bad["groups"]["contextId"]=json!("foreign-context");cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["members"][0]["groupId"]=json!("missing-group");cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["members"][0]["strokeId"]=json!("unlisted-leaf");cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["entries"][0]["parent"]=json!("missing-parent");cases.push(bad);
    let mut bad=clip.clone();let duplicate=bad["groups"]["entries"][0].clone();bad["groups"]["entries"].as_array_mut().unwrap().push(duplicate);cases.push(bad);
    let mut bad=clip.clone();let duplicate=bad["groups"]["members"][0].clone();bad["groups"]["members"].as_array_mut().unwrap().push(duplicate);cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["entries"][0]["gid"]=json!("");cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["entries"][0]["parent"]=json!("g2");
    bad["groups"]["entries"].as_array_mut().unwrap().push(json!({"gid":"g2","order":["a","b"]}));cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["entries"][0]["parent"]=json!("g2");
    bad["groups"]["entries"].as_array_mut().unwrap().extend([
        json!({"gid":"g2","order":["g1","a"]}),json!({"gid":"g3","order":["g1","b"]})]);cases.push(bad);
    let mut bad=clip.clone();bad["groups"]["entries"][0]["parent"]=json!("g2");
    bad["groups"]["entries"][0]["order"]=json!(["g2","a","b"]);
    bad["groups"]["entries"].as_array_mut().unwrap().push(json!({"gid":"g2","parent":"g1","order":["g1","a"]}));cases.push(bad);
    let mut bad=clip.clone();bad["records"][1]["target"]["frameScope"]["frame"]=json!(3);cases.push(bad);
    for (index,bad) in cases.into_iter().enumerate() {
        let bad:ClipPayload=serde_json::from_value(bad).unwrap();
        assert!(paste(&mut owner,&bad,&target("L1",0,EditMode::Motion)).is_err(),"case {index}");
        assert_eq!(frozen(&owner),before,"case {index} must preserve bytes, revision and both stacks");
    }
}

#[test]
fn legitimate_unselected_leaves_remain_portable_without_retained_owner_provenance() {
    let source=owner_of(&two_tables(2));
    let clip=copy(&source,"L0",0,&["a"]).unwrap().unwrap();
    let wire=serde_json::to_value(&clip).unwrap();
    assert_eq!(wire["groups"]["entries"][0]["order"],json!(["a","b"]));
    assert!(wire.get("source").is_none());
    let detached:ClipPayload=serde_json::from_value(wire).unwrap();
    let mut destination=owner_of(&two_tables(2));
    apply(&mut destination,"delete",&delete("L0",0,"motion",&["a","b"])).unwrap();
    assert!(paste(&mut destination,&detached,&target("L1",0,EditMode::Motion)).unwrap());
    let actual=value(&destination);
    assert_eq!(actual["layers"][1]["groups"]["entries"].as_array().unwrap().len(),1);
    assert_eq!(actual["layers"][1]["groups"]["members"],json!([
        {"strokeId":"x","groupId":"g1"},{"strokeId":"y","groupId":"g1"}]));
    assert_eq!(actual["objects"].as_array().unwrap().len(),3);
    let pasted=&actual["objects"][2];
    assert_ne!(pasted["target"]["strokeId"],Value::String("a".into()));
    assert_eq!(pasted["geometry"],serde_json::to_value(&detached.records[0]).unwrap()["geometry"]);
}

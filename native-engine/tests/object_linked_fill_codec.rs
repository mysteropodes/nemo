//! Own-geometry linked-fill codec controls; ordinary field absence and explicit false remain distinct.
use super::object_selection_edit_tests::{document,shape,timed,layer};
use crate::object_codec::{decode_storage_project,encode_storage_project};
use serde_json::{json,Value};
pub(super) fn linked_document(version:u32)->Value {
    let body=json!({"stroke":null,"hasRealStroke":false,"strokeWidth":6,"strokeCap":"round","strokeJoin":"round","opacity":1});
    let mut backdrop=shape("L","backdrop",0,-5);
    backdrop["family"]=json!("editable-vector-path");backdrop["vector"]=body.clone();
    backdrop["vectorExt"]=json!({"linkedFillId":"link-original","isLinkedFillCompanion":true});
    let mut ribbon=shape("L","ribbon",0,0);
    ribbon["family"]=json!("editable-vector-path");ribbon["vector"]=body;
    ribbon["vectorExt"]=json!({"linkedFillId":"link-original","isLinkedFillCompanion":false,
        "pressure":{"centerSegments":[
            {"point":{"x":0,"y":1},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":0.5},
            {"point":{"x":10,"y":1},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3}],
            "widthProfile":[{"t":0,"width":0.5},{"t":1,"width":3}]}});
    document(version,vec![timed("L","Pressure",&[0],&[]),layer("B","Unrelated")],
        vec![backdrop,ribbon,shape("L","other",0,40),shape("B","x",0,80)])
}
#[test]
fn separate_paths_roundtrip_link_presence_roles_and_geometry_without_single_path_substitution() {
    for version in [1,2] {
        let source=linked_document(version);
        let decoded=decode_storage_project(source.to_string().as_bytes()).unwrap();
        let encoded:Value=serde_json::from_slice(&encode_storage_project(&decoded).unwrap()).unwrap();
        assert_eq!(encoded,source);
        assert_ne!(encoded["objects"][0]["geometry"],encoded["objects"][1]["geometry"]);
        assert_eq!(encoded["objects"][0]["vectorExt"]["isLinkedFillCompanion"],true);
        assert_eq!(encoded["objects"][1]["vectorExt"]["isLinkedFillCompanion"],false);
        let mut absent=source.clone();absent["objects"][1]["vectorExt"].as_object_mut().unwrap().remove("isLinkedFillCompanion");
        let encoded:Value=serde_json::from_slice(&encode_storage_project(&decode_storage_project(absent.to_string().as_bytes()).unwrap()).unwrap()).unwrap();
        assert!(encoded["objects"][1]["vectorExt"].get("isLinkedFillCompanion").is_none());
        assert_eq!(encoded,absent);
    }
}
#[test]
fn malformed_missing_duplicate_cross_cell_mixed_kind_pairs_refuse() {
    let source=linked_document(1);
    for path in ["/objects/0/vectorExt/linkedFillId","/objects/0/vectorExt/isLinkedFillCompanion",
        "/objects/1/vectorExt/linkedFillId","/objects/1/vectorExt/isLinkedFillCompanion"] {
        let mut bad=source.clone();*bad.pointer_mut(path).unwrap()=Value::Null;
        assert!(decode_storage_project(bad.to_string().as_bytes()).is_err(),"{path}");
    }
    let mut cases=vec![];
    let mut bad=source.clone();bad["objects"].as_array_mut().unwrap().remove(0);cases.push(bad);
    let mut bad=source.clone();let mut duplicate=bad["objects"][0].clone();duplicate["target"]["strokeId"]=json!("duplicate-companion");bad["objects"].as_array_mut().unwrap().push(duplicate);cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["target"]["frameScope"]["frame"]=json!(1);cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["target"]["layerUid"]=json!("B");cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["family"]=json!("closed-solid-fill-cubic-path");cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["vectorExt"]["isLinkedFillCompanion"]=json!(false);cases.push(bad);
    let mut bad=source.clone();bad["objects"][1]["vectorExt"].as_object_mut().unwrap().remove("pressure");cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["fill"]=Value::Null;cases.push(bad);
    let mut bad=source.clone();bad["objects"][0]["vectorExt"]["linkedFillId"]=json!("");cases.push(bad);
    for (i,bad) in cases.into_iter().enumerate(){assert!(decode_storage_project(bad.to_string().as_bytes()).is_err(),"pair flaw {i}");}
}
#[test]
fn unrelated_vector_explicit_false_is_not_a_link_or_a_manufactured_companion() {
    let mut source=linked_document(1);source["objects"].as_array_mut().unwrap().drain(0..2);
    source["objects"][0]["family"]=json!("editable-vector-path");
    source["objects"][0]["vector"]=json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1});
    source["objects"][0]["vectorExt"]=json!({"isLinkedFillCompanion":false});
    let actual:Value=serde_json::from_slice(&encode_storage_project(&decode_storage_project(source.to_string().as_bytes()).unwrap()).unwrap()).unwrap();
    assert_eq!(actual,source);
}

#[test]
fn authored_order_is_preserved_even_when_it_routes_companion_above_primary() {
    let mut source=linked_document(2);
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]=json!({"static":[-17]});
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][1]["order"]=json!({"static":[23]});
    let actual:Value=serde_json::from_slice(&encode_storage_project(&decode_storage_project(source.to_string().as_bytes()).unwrap()).unwrap()).unwrap();
    assert_eq!(actual,source);
}

#[test]
fn animated_order_and_component_local_pairs_roundtrip_but_orphan_component_refuses() {
    let mut source=linked_document(2);
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]=json!({"track":{"keys":[{"frame":0,"v":[-7]},{"frame":5,"v":[7]}]}});
    let mut component_objects=source["objects"].as_array().unwrap()[..2].to_vec();
    for record in &mut component_objects {record["target"]["contextId"]=json!("comp-pair");}
    source["componentTables"]=json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-pair","totalFrames":6,"layers":[{"layerUid":"L"}],"objects":component_objects}],"instances":[]}});
    let decoded=decode_storage_project(source.to_string().as_bytes()).unwrap();
    let encoded:Value=serde_json::from_slice(&encode_storage_project(&decoded).unwrap()).unwrap();assert_eq!(encoded,source);
    source["componentTables"]["components"]["components"][0]["objects"].as_array_mut().unwrap().remove(0);
    assert!(decode_storage_project(source.to_string().as_bytes()).is_err());
}

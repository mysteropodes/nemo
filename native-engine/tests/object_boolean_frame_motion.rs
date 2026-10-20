//! Actual production Flatten: native tracks -> geometry -> one native history entry.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn document(property: Value) -> Value {
    let records: Vec<Value> = [("a", 0.), ("b", 20.)].into_iter().map(|(id,x)| {
        let segments: Vec<_> = [[x,0.],[x+10.,0.],[x+10.,10.],[x,10.]].into_iter().map(|p|
            json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})).collect();
        json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","layerUid":"L0","strokeId":id,"frameScope":{"kind":"authored","frame":1}},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}})
    }).collect();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[{"layerUid":"L0","groups":{"contextId":"scene-root","entries":[{"gid":"g","order":["a","b"],"combineMode":"unite"}],
            "members":[{"strokeId":"a","groupId":"g"},{"strokeId":"b","groupId":"g"}]}}],
        "objects":records,"propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[property]}]})
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new("flatten-motion",decode_storage_project(value.to_string().as_bytes()).unwrap()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(o.acquire_snapshot(o.content_revision()).unwrap().document()).unwrap()
}
fn payload() -> Vec<u8> {
    json!({"command":"object.boolean.flatten","contextId":"scene-root","layerUid":"L0","frame":1,"groupId":"g"}).to_string().into_bytes()
}
#[cfg(feature="compositor")]
fn actual_frame_bounds(o:&NativeObjectHistory,frame:u32) -> Vec<[f64;4]> {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame,ObjectFrameSelector};
    use crate::object_render_scene::prepare_object_render_scene;
    use vello::kurbo::Shape;
    let s=o.acquire_snapshot(o.content_revision()).unwrap();
    let selector=ObjectFrameSelector { instance_id:s.instance_id(),document_id:s.document_id(),content_revision:s.content_revision(),
        document_snapshot_id:s.snapshot_id(),context_id:"scene-root",scope_kind:FrameScopeKind::Authored,frame };
    let packet=prepare_object_frame(&s,&selector).unwrap();
    let scene=prepare_object_render_scene(&packet,&selector).unwrap();
    scene.entries().map(|entry|{let b=entry.path().bounding_box();[b.x0,b.y0,b.x1,b.y1]}).collect()
}

#[test]
fn actual_keyed_member_motion_is_baked_then_undo_and_reopen_preserve_it() {
    let mut o = owner(&document(json!({"property":"position","track":{"keys":[
        {"frame":0,"v":[0,0]}, {"frame":2,"v":[40,0]}]}})));
    let before = bytes(&o);
    let candidate = o.prepare_boolean_flatten_json(&payload()).unwrap();
    assert_eq!(bytes(&o), before, "preparation cannot publish");
    o.commit_boolean_edit(candidate).unwrap();
    assert_eq!(o.history_depths(), (1,0));
    let after = bytes(&o);
    #[cfg(feature="compositor")]
    assert_eq!(actual_frame_bounds(&o,1),vec![[20.,0.,30.,10.]]);
    let value: Value = serde_json::from_slice(&after).unwrap();
    assert_eq!(value["objects"].as_array().unwrap().len(), 1, "sampled +20 translation overlaps the second square exactly");
    for segment in value["objects"][0]["geometry"]["segments"].as_array().unwrap() {
        assert!(segment["point"]["x"].as_f64().unwrap() >= 20.);
        assert!(segment["point"]["x"].as_f64().unwrap() <= 30.);
    }
    let reopened = owner(&value);
    assert_eq!(bytes(&reopened), after);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), before);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), after);
}

#[test]
fn unsupported_or_expression_motion_leaves_source_revision_and_history_untouched() {
    for property in [
        json!({"property":"position","static":[20,0],"expression":{"enabled":true,"code":"[20,0]"}}),
        json!({"property":"rotationX","static":[30]})
    ] {
        let o = owner(&document(property));
        let before = bytes(&o);
        assert!(matches!(o.prepare_boolean_flatten_json(&payload()), Err(DispatchErrorCode::Unavailable)));
        assert_eq!(bytes(&o), before);
        assert_eq!(o.content_revision(), 0);
        assert_eq!(o.history_depths(), (0,0));
    }
}

#[test]
fn a_real_intervening_publication_rejects_the_sealed_motion_candidate() {
    let mut o = owner(&document(json!({"property":"position","static":[20,0]})));
    let stale = o.prepare_boolean_flatten_json(&payload()).unwrap();
    let current = o.prepare_boolean_flatten_json(&payload()).unwrap();
    o.commit_boolean_edit(current).unwrap();
    let after = bytes(&o);
    assert_eq!(o.commit_boolean_edit(stale), Err(DispatchErrorCode::StaleRevision));
    assert_eq!(bytes(&o), after);
    assert_eq!(o.history_depths(), (1,0));
}

#[test]
fn the_animated_style_identity_is_not_applied_twice_and_other_frames_keep_their_motion() {
    let mut source=document(json!({"property":"position","static":[-20,0]}));
    source["propertyHolders"][0]["target"]["strokeId"]=json!("b");
    let mut other=source["objects"][1].clone();
    other["target"]["frameScope"]["frame"]=json!(0);
    source["objects"].as_array_mut().unwrap().push(other.clone());
    for record in source["objects"].as_array_mut().unwrap() {
        record["family"]=json!("editable-vector-path");
        record["vector"]=json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1});
    }
    let mut o=owner(&source); let before=bytes(&o);
    #[cfg(feature="compositor")]
    let other_before=actual_frame_bounds(&o,0);
    let candidate=o.prepare_boolean_flatten_json(&payload()).unwrap(); o.commit_boolean_edit(candidate).unwrap();
    let after:Value=serde_json::from_slice(&bytes(&o)).unwrap();
    let objects=after["objects"].as_array().unwrap();
    let old=objects.iter().find(|r|r["target"]["frameScope"]["frame"]==0).unwrap();
    assert_eq!(old["geometry"],other["geometry"]);
    assert_eq!(after["propertyHolders"],source["propertyHolders"]);
    let baked=objects.iter().find(|r|r["target"]["frameScope"]["frame"]==1).unwrap();
    assert_ne!(baked["target"]["strokeId"],json!("b"),"the retained b holder must not move an already-baked result again");
    for segment in baked["geometry"]["segments"].as_array().unwrap() {
        let x=segment["point"]["x"].as_f64().unwrap(); assert!((0.0..=10.0).contains(&x));
    }
    let after_bytes=bytes(&o);
    #[cfg(feature="compositor")]
    {
        assert_eq!(actual_frame_bounds(&o,1),vec![[0.,0.,10.,10.]]);
        assert_eq!(actual_frame_bounds(&o,0),other_before);
        let reopened=owner(&after);
        assert_eq!(actual_frame_bounds(&reopened,1),vec![[0.,0.,10.,10.]]);
        assert_eq!(actual_frame_bounds(&reopened,0),other_before);
    }
    o.undo(o.content_revision()).unwrap(); assert_eq!(bytes(&o),before);
    o.redo(o.content_revision()).unwrap(); assert_eq!(bytes(&o),after_bytes);
    #[cfg(feature="compositor")]
    { assert_eq!(actual_frame_bounds(&o,1),vec![[0.,0.,10.,10.]]); assert_eq!(actual_frame_bounds(&o,0),other_before); }
}

#[test]
fn curved_members_use_the_real_t10_geometry_before_the_motion_fold() {
    let mut source=document(json!({"property":"position","static":[20,0]}));
    source["objects"][0]["geometry"]["segments"][0]["handleOut"]["y"]=json!(-20);
    source["objects"][0]["geometry"]["segments"][1]["handleIn"]["y"]=json!(-20);
    let mut o=owner(&source); let before=bytes(&o);
    let candidate=o.prepare_boolean_flatten_json(&payload()).unwrap(); o.commit_boolean_edit(candidate).unwrap();
    let after:Value=serde_json::from_slice(&bytes(&o)).unwrap();
    assert_eq!(after["objects"].as_array().unwrap().len(),1);
    assert!(after["objects"][0]["geometry"]["segments"].as_array().unwrap().iter()
        .any(|segment|segment["point"]["y"].as_f64().unwrap() < -10.),"curved bulge must survive; straight-anchor substitution fails");
    assert_eq!(o.history_depths(),(1,0));
    o.undo(o.content_revision()).unwrap(); assert_eq!(bytes(&o),before);
}

#[test]
fn nonneutral_opacity_refuses_without_losing_other_frames_or_redo() {
    for other_frame in [false,true] {
        let mut source=document(json!({"property":"opacity","static":[25]}));
        source["propertyHolders"][0]["target"]["strokeId"]=json!("b");
        if other_frame {
            let mut other=source["objects"][1].clone(); other["target"]["frameScope"]["frame"]=json!(0);
            source["objects"].as_array_mut().unwrap().push(other);
        }
        let mut o=owner(&source);
        let rename=json!({"command":"object.layer.rename","layerUid":"L0","name":"temporary"}).to_string();
        let edit=o.prepare_layer_rename_json(rename.as_bytes()).unwrap(); o.commit_layer_lifecycle(edit).unwrap();
        o.undo(o.content_revision()).unwrap();
        let before=bytes(&o); let revision=o.content_revision(); assert_eq!(o.history_depths(),(0,1));
        assert!(matches!(o.prepare_boolean_flatten_json(&payload()),Err(DispatchErrorCode::Unavailable)));
        assert_eq!(bytes(&o),before); assert_eq!(o.content_revision(),revision); assert_eq!(o.history_depths(),(0,1));
    }
}

#[test]
fn asymmetric_cubic_motion_uses_exact_pivot_and_output_space_curve_tolerance() {
    use crate::object_document::object_group_kernels::object_fill_trace_ops::native::{bounds,ring};
    let mut source=document(json!({"property":"position","static":[5,-7]}));
    source["propertyHolders"][0]["properties"].as_array_mut().unwrap().extend([
        json!({"property":"anchor","static":[2,3]}),json!({"property":"rotation","static":[90]}),
        json!({"property":"scale","static":[250,10000]})]);
    source["objects"][0]["geometry"]["segments"][0]["handleOut"]["y"]=json!(-20);
    source["objects"][0]["geometry"]["segments"][1]["handleIn"]["y"]=json!(-10);
    for (segment,p) in source["objects"][1]["geometry"]["segments"].as_array_mut().unwrap().iter_mut()
        .zip([[0,-10],[1,-10],[1,-9],[0,-9]]) { segment["point"]=json!({"x":p[0],"y":p[1]}); }
    let mut o=owner(&source);
    let edit=o.prepare_boolean_flatten_json(&payload()).unwrap(); o.commit_boolean_edit(edit).unwrap();
    let s=o.acquire_snapshot(o.content_revision()).unwrap(); assert_eq!(s.document().objects().len(),1);
    let b=bounds(&ring(&s.document().objects()[0]));
    // The cubic y extremum is -20/sqrt(3); true bounds centre + anchor is
    // [7,8-10/sqrt(3)]. Scale[2.5,100], then90deg, then[5,-7].
    let py=8.-10./3f64.sqrt();
    let expected=[12.+100.*py-1000.,py-24.5,12.+100.*py+2000./3f64.sqrt(),py+0.5];
    for (actual,expected) in [b.x,b.y,b.x+b.width,b.y+b.height].into_iter().zip(expected) {
        assert!((actual-expected).abs()<=0.75,"{actual} vs {expected}: retained output-space tolerance exceeded");
    }
}

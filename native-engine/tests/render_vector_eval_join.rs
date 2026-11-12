//! The join as a consumer sees it: immutability across frames, detached output, the packet entry the scene producer calls,
//! the holder-table predicate and the typed refusals. Documents are oracle rows (real codec, real snapshot), edited as JSON.
use super::super::{
    evaluate_record, evaluate_vector, holders_fully_consumed, params, VectorEvalError,
};
use super::{bits, check, row, rows, snapshot, Row};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamGeomError;
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::PathFxError;
use crate::object_codec::{decode_project, encode_project};
use crate::object_document::{FrameScopeKind, Presence};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn value_of(row: &Row) -> Value {
    serde_json::from_str(&row.doc).expect("oracle document is json")
}

fn snap(value: &Value) -> ObjectSnapshot {
    snapshot(&value.to_string())
}

fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

fn consumed(value: &Value) -> bool {
    let document = decode_project(value.to_string().as_bytes()).expect("variant is admitted");
    holders_fully_consumed(&document)
}

#[test]
fn repeated_frames_leave_the_snapshot_and_the_first_result_untouched() {
    let (_, all) = rows();
    let source = row(&all, "rect-roughen-stack-then-rebuild@12");
    let snapshot = snap(&value_of(source));
    let document_before = snapshot.document().clone();
    let bytes_before = encode_project(snapshot.document()).expect("encodes");
    let first = evaluate_vector(&snapshot, 0, 0).expect("frame 0");
    let middle = evaluate_vector(&snapshot, 0, 10).expect("frame 10");
    let again = evaluate_vector(&snapshot, 0, 0).expect("frame 0 again");
    assert_eq!(
        bits(&first),
        bits(&again),
        "frame 0 is byte-identical after frame 10"
    );
    assert_ne!(
        bits(&first),
        bits(&middle),
        "roughen evolves, the animated corner moves"
    );
    assert_eq!(snapshot.document(), &document_before);
    assert_eq!(
        encode_project(snapshot.document()).expect("encodes"),
        bytes_before
    );
    // the stored rest geometry is still the authored rounded rectangle (anchor 0 at x 10)
    let rest = &snapshot.document().objects()[0].geometry.segments[0];
    assert_eq!(rest.point.x.as_f64(), Some(10.0));
    assert_eq!(rest.point.y.as_f64(), Some(0.0));
}

#[test]
fn detached_output_cannot_mutate_rest_geometry() {
    let (_, all) = rows();
    let source = row(&all, "rect-zigzag-then-rebuild-uses-post-effect-bounds@5");
    let snapshot = snap(&value_of(source));
    let mut taken = evaluate_vector(&snapshot, 0, 5).expect("evaluates");
    check(&taken, source).expect("matches the oracle");
    taken.segments.clear();
    taken.closed = false;
    let fresh = evaluate_vector(&snapshot, 0, 5).expect("evaluates again");
    check(&fresh, source).expect("a second evaluation is unaffected by the caller's edits");
    assert_eq!(fresh.segments.len(), 8);
    assert_eq!(
        snapshot.document().objects()[0].geometry.segments.len(),
        8,
        "rest geometry keeps the authored anchors"
    );
}

#[test]
fn the_packet_entry_gives_every_consumer_the_snapshot_entry_value() {
    let (_, all) = rows();
    let mut checked = 0;
    for source in all.iter().filter(|r| !r.doc.contains("propertyHolders")) {
        // v1 selects the records authored AT the selector frame, so the record is authored at the evaluated frame
        let mut value = value_of(source);
        value["objects"][0]["target"]["frameScope"]["frame"] = json!(source.frame);
        let snapshot = snap(&value);
        let packet = prepare_object_frame(&snapshot, &selector(&snapshot, source.frame))
            .unwrap_or_else(|e| panic!("{}: {e:?}", source.id));
        let record = packet.records().next().expect("one record");
        let direct = evaluate_vector(&snapshot, 0, source.frame).expect("snapshot entry");
        let via_packet = evaluate_record(&packet, record).expect("packet entry");
        assert_eq!(via_packet, direct, "{}", source.id);
        // scene, preview and export all retain a clone of this packet
        let retained = packet.clone();
        let again = evaluate_record(&retained, retained.records().next().expect("record"));
        assert_eq!(again.as_ref(), Ok(&direct), "{}", source.id);
        check(&via_packet, source).expect("matches the oracle");
        checked += 1;
    }
    assert_eq!(checked, 17);
}

#[test]
fn the_holder_predicate_admits_only_what_the_join_consumes() {
    let (_, all) = rows();
    let rect = value_of(row(&all, "rect-corner-tl-animated@5"));
    assert!(consumed(&rect));
    assert!(consumed(&value_of(row(&all, "star-corner-animated@5"))));
    assert!(
        consumed(&value_of(row(&all, "plain-closed-zigzag@0"))),
        "no table"
    );
    let mut empty = rect.clone();
    empty["propertyHolders"] = json!([]);
    assert!(consumed(&empty), "an empty table");

    let mut other = rect.clone();
    other["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "position", "static": [1, 2]}));
    assert!(
        consumed(&other),
        "element position now reaches the actual sampled Motion consumer"
    );
    other["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"opacity","static":[25]}));
    assert!(
        !consumed(&other),
        "unconsumed element opacity remains refused"
    );

    let mut foreign = rect.clone();
    foreign["propertyHolders"][0]["properties"][0]["property"] = json!("arcSweep");
    assert!(!consumed(&foreign), "a key of another kind is not consumed");

    let mut live = rect.clone();
    live["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled": true, "code": "1"});
    assert!(
        !consumed(&live),
        "an enabled expression needs the evaluator"
    );
    live["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
    assert!(consumed(&live), "a disabled expression is stored data only");

    let mut layer = rect.clone();
    layer["propertyHolders"]
        .as_array_mut()
        .expect("holders")
        .push(
            json!({"target": {"contextId": "scene-root", "layerUid": "L1"},
               "properties": [{"property": "rotation", "static": [50]}]}),
        );
    assert!(
        !consumed(&layer),
        "unsupported layer rotation is not consumed"
    );

    let mut controls = rect.clone();
    controls["propertyHolders"][0]["controls"] =
        json!([{"key": "xc_point", "name": "Point", "type": "point", "default": [12, 34]}]);
    assert!(!consumed(&controls), "expression controls are not consumed");

    let mut no_shape = rect.clone();
    no_shape["objects"][0]
        .as_object_mut()
        .expect("record")
        .remove("vectorExt");
    assert!(
        !consumed(&no_shape),
        "a vector without paramShape has no shape to rebuild"
    );

    let mut legacy = rect.clone();
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vectorExt");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    assert!(
        !consumed(&legacy),
        "the legacy family has no parametric rebuild"
    );
}

#[test]
fn refusals_are_typed_and_publish_nothing() {
    let (_, all) = rows();
    let rect = value_of(row(&all, "rect-corner-tl-animated@5"));
    let base = snap(&rect);
    let baseline = evaluate_vector(&base, 0, 5).expect("base evaluates");

    assert_eq!(
        evaluate_vector(&base, 7, 5),
        Err(VectorEvalError::MissingRecord)
    );

    let mut legacy = value_of(row(&all, "plain-closed-zigzag@0"));
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));
    assert_eq!(
        evaluate_vector(&snap(&legacy), 0, 0),
        Err(VectorEvalError::NotVectorFamily)
    );

    // an enabled expression on a KEYED parameter needs the expression evaluator; the same expression disabled, or on an
    // unkeyed parameter (the baseline never calls valueAtFrame for it), changes nothing
    let mut live = rect.clone();
    live["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled": true, "code": "1"});
    assert_eq!(
        evaluate_vector(&snap(&live), 0, 5),
        Err(VectorEvalError::ExpressionUnavailable {
            property: "cornerTL"
        })
    );
    live["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
    assert_eq!(evaluate_vector(&snap(&live), 0, 5).as_ref(), Ok(&baseline));
    let mut unkeyed = rect.clone();
    unkeyed["propertyHolders"][0]["properties"]
        .as_array_mut()
        .expect("properties")
        .push(json!({"property": "cornerBL", "expression": {"enabled": true, "code": "1"}}));
    assert_eq!(
        evaluate_vector(&snap(&unkeyed), 0, 5).as_ref(),
        Ok(&baseline)
    );

    // an unknown kind cannot be admitted, so it is tried on a detached copy of the stored record
    let mut stored = base.document().objects()[0].clone();
    if let Presence::Present(ext) = &mut stored.vector_ext {
        if let Presence::Present(shape) = &mut ext.param_shape {
            shape.kind = "hexagon".to_string();
        }
    }
    assert!(matches!(
        params::shape_of(&stored),
        Err(VectorEvalError::UnknownShapeKind)
    ));

    // an unbounded sample count is refused by the kernel and surfaces as the join's error
    let mut budget = value_of(row(&all, "plain-closed-zigzag@0"));
    budget["layers"][0]["pathFx"][0]["p2"] = json!(1.0e9);
    assert_eq!(
        evaluate_vector(&snap(&budget), 0, 0),
        Err(VectorEvalError::PathFx(PathFxError::SampleBudget))
    );
    let mut points = value_of(row(&all, "star-inner-animated@10"));
    points["objects"][0]["vectorExt"]["paramShape"]["pointCount"] = json!(1.0e9);
    assert_eq!(
        evaluate_vector(&snap(&points), 0, 5),
        Err(VectorEvalError::Rebuild(ParamGeomError::StarPointBudget))
    );
}


fn pressure_motion_fixture(row:&Value)->Value {
    let anchor=|s:&Value|json!({"point":{"x":s["point"][0],"y":s["point"][1]},"handleIn":{"x":s["handleIn"][0],"y":s["handleIn"][1]},"handleOut":{"x":s["handleOut"][0],"y":s["handleOut"][1]}});
    let center=row["center"].as_array().unwrap().iter().map(|s|{let mut a=anchor(s);a["width"]=s["width"].clone();a}).collect::<Vec<_>>();
    let mut pressure=json!({"centerSegments":center,"strokeProfile":"taper-both","profileBase":0});
    if !row["profile"].is_null(){pressure["widthProfile"]=row["profile"].clone();}
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L1","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":[{"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":row["stored"].as_array().unwrap().iter().map(anchor).collect::<Vec<_>>()},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},"vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1},"vectorExt":{"pressure":pressure}}],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"v1"},"properties":row["raw"].as_object().unwrap().iter().map(|(p,v)|json!({"property":p,"static":v})).collect::<Vec<_>>()}]})
}
fn pressure_motion_snapshot(doc:&Value)->ObjectSnapshot {
    let mut doc=doc.clone();
    doc["compositionOrder"]=json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
        "layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],"scopes":[{"frameScope":{"kind":"authored","frame":0},
        "elements":doc["objects"].as_array().unwrap().iter().enumerate().map(|(i,r)|json!({"layerUid":"L1","strokeId":r["target"]["strokeId"],"naturalPosition":i,"order":{}})).collect::<Vec<_>>()}]}]});
    ObjectSnapshot::new("pressure-motion",crate::object_codec::decode_storage_project(doc.to_string().as_bytes()).unwrap()).unwrap()
}
fn pressure_motion_rows()->Value {
    serde_json::from_str(include_str!("fixtures/subselect-baseline/motion.json")).unwrap()
}
fn check_pressure_motion(snapshot:&ObjectSnapshot,row:&Value,frame:u32) {
    if row["baselineNonFinite"]==true {
        assert_eq!(evaluate_vector(snapshot,0,frame),Err(VectorEvalError::PressureWidthBaselineDefect));
        return;
    }
    if ["collapsed/vertices-zero-and-size","collapsed/size","collapsed/trim","collapsed/trim-wins","collapsed/full-trim-wins","collapsed-matching/trim","collapsed-matching/trim-wins","collapsed-matching/full-trim-wins"].contains(&row["name"].as_str().unwrap()) {
        assert_eq!(row["expected"].as_array().unwrap().len(),match row["name"].as_str().unwrap() {"collapsed/full-trim-wins"=>83,"collapsed-matching/full-trim-wins"=>81,"collapsed-matching/trim"|"collapsed-matching/trim-wins"=>50,"collapsed/trim"|"collapsed/trim-wins"=>53,_=>4},"retain the frozen expected contour");
        assert_eq!(evaluate_vector(snapshot,0,frame),Err(VectorEvalError::PressureCrossingUnavailable));
        return;
    }
    let got=evaluate_vector(snapshot,0,frame).unwrap_or_else(|e|panic!("{}: {e:?}",row["name"]));
    let expected=row["expected"].as_array().unwrap();
    assert_eq!(got.segments.len(),expected.len(),"{}",row["name"]);
    assert_eq!(got.closed,!expected.is_empty(),"{}",row["name"]);
    for (a,b) in got.segments.iter().zip(expected) {
        for (actual,key) in [(&a.point,"point"),(&a.handle_in,"handleIn"),(&a.handle_out,"handleOut")] {
            for j in 0..2 {assert!((actual[j]-b[key][j].as_f64().unwrap()).abs()<1e-8,"{} {key} {j}: {:?} != {}",row["name"],actual,b[key]);}
        }
    }
}
#[test]
fn pressure_motion_frozen_centerline_width_size_and_trim_precedence() {
    let oracle=pressure_motion_rows();
    assert_eq!(oracle["baseline"],"1065e9d8d9c48e66d9b44bb4de446e8f4e887040");
    for row in oracle["rows"].as_array().unwrap() {
        let doc=pressure_motion_fixture(row);let snapshot=pressure_motion_snapshot(&doc);
        let before=crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
        for frame in [10,0,10] {check_pressure_motion(&snapshot,row,frame);}
        assert_eq!(crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),before);
        assert!(holders_fully_consumed(snapshot.document()),"{}",row["name"]);
        assert_eq!(crate::object_document::PropertyHolder::vertex_count(snapshot.document(),snapshot.document().property_holders().unwrap()[0].target()),Some(row["center"].as_array().unwrap().len()),"Motion rows address centerline nodes, never outline edges");
    }
}
#[test]
fn pressure_motion_keyed_frames_and_idle_source_remain_independent() {
    let rows=pressure_motion_rows();let rows=rows["rows"].as_array().unwrap();
    let row=rows.iter().find(|r|r["name"]=="curve/vertices-and-size").unwrap();
    let zero=rows.iter().find(|r|r["name"]=="curve/vertices-zero-and-size").unwrap();
    let mut doc=pressure_motion_fixture(row);
    for p in doc["propertyHolders"][0]["properties"].as_array_mut().unwrap() {
        if p["property"].as_str().unwrap().starts_with("vtx") {let value=p["static"].take();p.as_object_mut().unwrap().remove("static");p["track"]=json!({"keys":[{"frame":0,"v":[0,0],"curvePoints":[]},{"frame":10,"v":value,"curvePoints":[]}]});}
    }
    let s=pressure_motion_snapshot(&doc);for (frame,row) in [(0,zero),(10,row),(0,zero)] {check_pressure_motion(&s,row,frame);}
    doc["propertyHolders"]=json!([]);let idle=pressure_motion_snapshot(&doc);let got=evaluate_vector(&idle,0,0).unwrap();
    for (a,b) in got.segments.iter().zip(row["stored"].as_array().unwrap()) {for j in 0..2 {assert_eq!(a.point[j],b["point"][j].as_f64().unwrap());}}
    doc["propertyHolders"]=json!([{"target":{"contextId":"scene-root","layerUid":"L1","strokeId":"v1"},"properties":[{"property":"brushSize","track":{"keys":[]}}]}]);
    assert_eq!(evaluate_vector(&pressure_motion_snapshot(&doc),0,0).unwrap(),got,"empty track is not authored size");
}

#[cfg(feature="compositor")]
#[test]
fn pressure_motion_scene_export_inputs_hit_contours_and_empty_neighbors_agree() {
    use crate::object_frame_packet::prepare_object_exposure_frame;
    use crate::object_render_scene::{prepare_object_render_scene,prepare_object_paint_plan,prepare_object_viewport_composition};
    use crate::evaluation::object_vector_kernels::render_vector_eval::{painted_record,duplicator_geometry};
    for row in pressure_motion_rows()["rows"].as_array().unwrap().iter().filter(|r|r["name"].as_str().unwrap().starts_with("curve/")) {
        let doc=pressure_motion_fixture(row);let s=pressure_motion_snapshot(&doc);let pins=selector(&s,10);
        let packet=prepare_object_exposure_frame(&s,&pins).unwrap();
        let local=painted_record(&s,&s.document().objects()[0],&s.document().layers()[0],10).unwrap();
        let got=evaluate_vector(&s,0,10).unwrap();
        assert_eq!(local.geometry.segments.len(),got.segments.len());
        assert_eq!(duplicator_geometry(&packet,&s.document().objects()[0],0).unwrap().0,got.segments);
        let scene=prepare_object_render_scene(&packet,&pins).unwrap_or_else(|e|panic!("{}: {e:?}",row["name"]));
        let paint=prepare_object_paint_plan(&scene,&pins).unwrap();
        let command=paint.commands().next().unwrap();
        if got.segments.is_empty(){assert!(command.path().elements().is_empty(),"export input must be an empty draw, never its culling envelope");}
        else {assert!(!command.path().elements().is_empty());}
        let witness=prepare_object_viewport_composition(&paint,&pins,None).unwrap();
        witness.subselect_read_projection("L1",None,true).unwrap();
        // A sibling's ordinary paint never becomes unavailable because this
        // ribbon has a collapsed/off-path Trim, nor does Trim write a revision.
        let mut sibling=doc["objects"][0].clone();sibling["target"]["strokeId"]=json!("neighbor");sibling["vectorExt"]=Value::Null;
        let mut neighbor=doc.clone();sibling.as_object_mut().unwrap().remove("vectorExt");neighbor["objects"].as_array_mut().unwrap().push(sibling);
        let s=pressure_motion_snapshot(&neighbor);let pins=selector(&s,10);let packet=prepare_object_exposure_frame(&s,&pins).unwrap();
        assert!(prepare_object_render_scene(&packet,&pins).is_ok(),"{} neighbor",row["name"]);
    }
}

#[cfg(feature="compositor")]
#[test]
fn pressure_motion_center_pages_keep_source_identity_and_refuse_stale_or_unpaintable_reads() {
    let oracle=pressure_motion_rows();let row=oracle["rows"].as_array().unwrap().iter().find(|r|r["name"]=="curve/vertices-and-size").unwrap();
    let s=pressure_motion_snapshot(&pressure_motion_fixture(row));let record=&s.document().objects()[0];
    let mut q=json!({"apiVersion":2,"requestId":"ribbon-center","instanceId":s.instance_id(),"documentId":s.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),"stableTarget":record.target(),"subselect":{"cursor":0,"limit":16,"hitTest":true,"center":true,"atFrame":10}}});
    let read=|q:&Value|serde_json::to_value(s.dispatch(serde_json::from_value(q.clone()).unwrap()).unwrap()).unwrap();
    let response=read(&q);assert_eq!(response["ok"],true,"{response}");let page=&response["result"]["subselect"];
    assert_eq!(page["atFrame"],10);assert_eq!(page["total"],3);assert_eq!(page["widths"],json!([2,9,3]));
    for (i,expected) in [[12.,42.],[64.,44.],[110.,40.]].iter().enumerate() {assert_eq!(page["segments"][i]["point"],json!(expected));}
    assert_eq!(page["segments"][0]["handleOut"],json!([20.,-20.]));
    q["payload"]["subselect"].as_object_mut().unwrap().remove("atFrame");let source=read(&q);
    assert_eq!(source["result"]["subselect"]["segments"][0]["point"],json!([20.,30.]));
    assert!(source["result"]["subselect"].get("atFrame").is_none());
    q["payload"]["documentSnapshotId"]=json!("stale");assert_eq!(read(&q)["error"]["code"],"stale_revision");
    use crate::evaluation::object_vector_kernels::render_vector_eval::{painted_record,evaluate_record_rest_with_pose};
    let mut textured=record.clone();textured.vector_ext.value_mut().unwrap().brush_texture=Presence::Present(serde_json::from_value(json!({"brushGroupId":"texture"})).unwrap());
    assert_eq!(painted_record(&s,&textured,&s.document().layers()[0],10),Err(VectorEvalError::BrushVertexUnavailable));
    let pins=selector(&s,10);let packet=crate::object_frame_packet::prepare_object_exposure_frame(&s,&pins).unwrap();
    let pose=crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedStroke {stroke_id:"v1".into(),closed:true,
        segments:record.geometry.segments.iter().map(|s|crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedSegment {point:[s.point.x.as_f64().unwrap(),s.point.y.as_f64().unwrap()],handle_in:[s.handle_in.x.as_f64().unwrap(),s.handle_in.y.as_f64().unwrap()],handle_out:[s.handle_out.x.as_f64().unwrap(),s.handle_out.y.as_f64().unwrap()]}).collect()};
    assert_eq!(evaluate_record_rest_with_pose(&packet,packet.records().next().unwrap(),Some(&pose)),Err(VectorEvalError::RigCompositionUnavailable));
}

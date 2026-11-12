//! #2372 SEL-MASK: the hit-test listing of a layer that holds masks lists its content and each mask (role `mask`, with the mode and the
//! layer-shared feather the Mask panel shows); the editable read still refuses such a layer. Evaluated contour paging covers
//! ordinary painted vectors; masks with path effects remain omitted from their unsupported mask-geometry reader.
use super::*;
use crate::object_render_scene::prepare_object_viewport_composition;

fn listing(doc: &Value) -> Result<Vec<Value>, &'static str> {
    full(doc).map(|result| result["items"].as_array().cloned().unwrap_or_default())
}
fn full(doc: &Value) -> Result<Value, &'static str> {
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let plan = prepare_object_paint_plan(&scene(&s), &pins(&s, 7)).unwrap();
    let witness = prepare_object_viewport_composition(&plan, &pins(&s, 7), None).unwrap();
    let projection = witness
        .subselect_read_projection("L0", None, true)?
        .with_presentation("native-work-1".into(), 1)
        .unwrap();
    let request = json!({"apiVersion":2,"requestId":"hit","instanceId":s.instance_id(),"documentId":s.document_id(),
        "operation":"query.document.object","payload":{"atRevision":s.content_revision(),
        "documentSnapshotId":s.snapshot_id(),"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":7,
        "workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true}}});
    let response = s
        .dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection))
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    Ok(response["result"].clone())
}
fn rows(items: &[Value]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|i| {
            (
                i["stableTarget"]["strokeId"].as_str().unwrap().to_owned(),
                i["role"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// The mask document with its record `a` made ordinary filled content: the layer holds one stroke and two masks.
fn masked_content() -> Value {
    let mut doc = document();
    let content = doc["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["target"]["layerUid"] == "L0" && r["target"]["strokeId"] == "a")
        .unwrap();
    content.as_object_mut().unwrap().remove("vectorExt");
    content.as_object_mut().unwrap().remove("effects");
    content["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    doc
}

#[test]
fn a_masked_layer_lists_its_content_and_each_mask_with_mode_and_shared_feather() {
    let items = listing(&masked_content()).unwrap();
    let rows = rows(&items);
    assert_eq!(
        rows.iter().filter(|(_, role)| role == "mask").count(),
        2,
        "{rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|(id, role)| id == "a" && role != "mask")
            .count(),
        1,
        "the masked content is listed as before: {rows:?}"
    );
    let masks: Vec<_> = items.iter().filter(|i| i["role"] == "mask").collect();
    let modes: Vec<_> = masks
        .iter()
        .map(|m| m["style"]["maskMode"].as_str().unwrap())
        .collect();
    assert_eq!(modes, ["intersect", "subtract"]);
    for mask in &masks {
        assert_eq!(
            mask["style"]["maskFeather"], 4.0,
            "the feather is the layer's shared (largest positive) value"
        );
        assert_eq!(
            mask["matrix"],
            json!([1.0, 0.0, 0.0, 1.0, 7.0, -3.0]),
            "a mask is posed by its layer's Position"
        );
        assert!(
            mask["count"].as_u64().is_some_and(|n| n >= 2) && mask["closed"].is_boolean(),
            "{mask}"
        );
        assert!(mask["bounds"].as_array().is_some_and(|b| b.len() == 4));
    }
}

#[test]
fn a_layer_of_masks_alone_lists_them_and_reads_an_unknown_mode_as_add() {
    let items = listing(&document()).unwrap();
    let modes: Vec<_> = items
        .iter()
        .map(|m| {
            (
                m["role"].as_str().unwrap(),
                m["style"]["maskMode"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        modes,
        [("mask", "intersect"), ("mask", "subtract"), ("mask", "add")]
    );
}

#[test]
fn the_editable_read_still_refuses_a_masked_layer() {
    let o = owner(&document());
    let s = o.acquire_snapshot(0).unwrap();
    let plan = prepare_object_paint_plan(&scene(&s), &pins(&s, 7)).unwrap();
    let witness = prepare_object_viewport_composition(&plan, &pins(&s, 7), None).unwrap();
    assert!(witness
        .subselect_read_projection("L0", None, false)
        .is_err());
}

#[test]
fn a_layer_with_path_effects_lists_no_vector_item_but_is_not_refused() {
    let mut doc = document();
    let layer = doc["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|l| l["layerUid"] == "L0")
        .unwrap();
    layer["pathFx"] = json!([{"type":"zigzag","enabled":true,"p1":10.0,"p2":2.0,"p3":0.0}]);
    let items = listing(&doc);
    // The listing is answered (never the whole-layer refusal); the effect layer's records are not offered for selection.
    let rows = rows(&items.expect("a path-effect layer is not refused as a whole"));
    assert!(
        rows.is_empty(),
        "every record of the effect layer is left out item by item: {rows:?}"
    );
}

#[test]
fn the_hit_listing_names_the_pose_the_layers_motion_applies_to_a_pointer_on_the_canvas() {
    // The fixture moves L0 by Position [7,-3]; a pointer at world (W) reaches a record at W - (7,-3).
    let result = full(&masked_content()).unwrap();
    assert_eq!(result["layerMatrix"], json!([1.0, 0.0, 0.0, 1.0, 7.0, -3.0]), "{result}");
    let item = result["items"].as_array().unwrap().iter().find(|i| i["role"] != "mask").unwrap();
    assert_eq!(item["matrix"], result["layerMatrix"], "an ordinary record is posed by exactly the layer's pose");
    // A rotation about the content's own centre is part of the same pose.
    let mut turned = paint_test_document(&[7]);
    turned["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","static":[7,-3]}, {"property":"rotation","static":[90]}]}]);
    let pose = full(&turned).unwrap()["layerMatrix"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
    assert!(pose[0].abs() < 1e-9 && (pose[1] - 1.0).abs() < 1e-9 && (pose[2] + 1.0).abs() < 1e-9 && pose[3].abs() < 1e-9, "{pose:?}");
    // No Motion at all is the identity, and the empty layer too (nothing to pivot on).
    let mut still = masked_content();
    still.as_object_mut().unwrap().remove("propertyHolders");
    assert_eq!(full(&still).unwrap()["layerMatrix"], json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
}

/// Frozen point-offset semantics remain covered independently of the old
/// offset-metadata transport: the client now pages evaluated paint at its frame.
#[test]
fn a_record_with_vertex_offsets_lists_painted_bounds_and_correlated_evaluated_geometry() {
    let plain = listing(&masked_content()).unwrap();
    let mut doc=masked_content();
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
        "properties":[{"property":"vtx0","static":[40,30]}]}));
    let items=listing(&doc).unwrap();
    let posed=items.iter().find(|i|i["stableTarget"]["strokeId"]=="a"&&i["role"]!="mask").unwrap();
    assert_eq!(posed["count"],3);
    // Source [0,0] + offset [40,30], with relative handleOut [3,4]:
    // the conservative painted control bounds end at [43,34].
    assert_eq!(posed["bounds"],json!([0.,0.,43.,34.]));
    assert_eq!(posed["matrix"],json!([1.,0.,0.,1.,7.,-3.]));
    // Legacy callers still combine this with an unframed source geometry
    // page; the frame-aware caller consumes evaluated geometry and ignores it.
    assert_eq!(posed["vertexOffsets"],json!([[40.,30.],[0.,0.],[0.,0.]]));
    assert_eq!(items.iter().filter(|i|i["role"]=="mask").collect::<Vec<_>>(),plain.iter().filter(|i|i["role"]=="mask").collect::<Vec<_>>(),"mask siblings remain unchanged");
    let o=owner(&doc);let snapshot=o.acquire_snapshot(0).unwrap();
    let mut request=json!({"apiVersion":2,"requestId":"evaluated-mask-neighbor","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),"stableTarget":posed["stableTarget"],"subselect":{"cursor":0,"limit":16,"hitTest":true,"atFrame":7}}});
    let read=|q:&Value|serde_json::to_value(snapshot.dispatch(serde_json::from_value(q.clone()).unwrap()).unwrap()).unwrap();
    let response=read(&request);assert_eq!(response["ok"],true,"{response}");let page=&response["result"]["subselect"];
    assert_eq!(page["atFrame"],7);assert_eq!(page["stableTarget"],posed["stableTarget"]);assert_eq!(page["total"],posed["count"]);
    assert_eq!(page["segments"][0],json!({"point":[40.,30.],"handleIn":[0.,0.],"handleOut":[3.,4.]}));
    assert_eq!(page["segments"][1],json!({"point":[10.,0.],"handleIn":[-2.,1.],"handleOut":[0.,0.]}));
    assert_eq!(page["segments"][2]["point"],json!([0.,10.]));
    let mut legacy=request.clone();legacy["payload"]["subselect"].as_object_mut().unwrap().remove("atFrame");
    let old=read(&legacy);assert_eq!(old["ok"],true);
    for (i,source) in old["result"]["subselect"]["segments"].as_array().unwrap().iter().enumerate() {
        for j in 0..2 {assert_eq!(source["point"][j].as_f64().unwrap()+posed["vertexOffsets"][i][j].as_f64().unwrap(),page["segments"][i]["point"][j].as_f64().unwrap());}
        assert_eq!(source["handleIn"],page["segments"][i]["handleIn"]);assert_eq!(source["handleOut"],page["segments"][i]["handleOut"]);
    }
    request["payload"]["documentSnapshotId"]=json!("stale");assert_eq!(read(&request)["error"]["code"],"stale_revision");
    let plan=prepare_object_paint_plan(&scene(&snapshot),&pins(&snapshot,7)).unwrap();
    let painted=plan.commands().find(|c|c.source().record().target().stroke_id()=="a"&&c.source().record().target().layer_uid()=="L0").unwrap();
    assert_eq!(painted.path().elements()[0],PathEl::MoveTo(Point::new(47.,27.)));
    assert_eq!(painted.path().elements()[1],PathEl::CurveTo(Point::new(50.,31.),Point::new(15.,-2.),Point::new(17.,-3.)));
}

#[test]
fn the_listing_reads_back_the_stored_select_pivot_of_a_vector_record() {
    let mut doc = masked_content();
    let content = doc["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["target"]["layerUid"] == "L0" && r["target"]["strokeId"] == "a")
        .unwrap();
    content["vectorExt"] = json!({"xformAnchorKey":"br","xformAnchorCustom":[12.5, -3]});
    let items = listing(&doc).unwrap();
    let item = items.iter().find(|i| i["stableTarget"]["strokeId"] == "a").unwrap();
    assert_eq!(item["xformAnchorKey"], "br");
    assert_eq!(item["xformAnchorCustom"], json!([12.5, -3.0]));
    let plain = listing(&masked_content()).unwrap();
    let item = plain.iter().find(|i| i["stableTarget"]["strokeId"] == "a").unwrap();
    assert!(item.get("xformAnchorKey").is_none() && item.get("xformAnchorCustom").is_none(), "{item}");
}

#[test]
fn path_effect_contours_use_framed_geometry_without_legacy_source_offsets() {
    let mut doc=masked_content();
    doc["layers"].as_array_mut().unwrap().iter_mut().find(|l|l["layerUid"]=="L0").unwrap()["pathFx"]=json!([{"type":"zigzag","enabled":true,"p1":10.,"p2":2.,"p3":0.}]);
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[{"property":"vtx0","static":[4,3]}]}));
    let items=listing(&doc).unwrap();
    let painted=items.iter().find(|r|r["stableTarget"]["strokeId"]=="a"&&r["role"]!="mask").unwrap();
    assert_eq!(painted["count"],12);
    assert!(painted.get("vertexOffsets").is_none(),"effect topology cannot be represented by offsets on three source anchors");
    let o=owner(&doc);let s=o.acquire_snapshot(0).unwrap();
    let q=json!({"apiVersion":2,"requestId":"fx-hit","instanceId":s.instance_id(),"documentId":s.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),"stableTarget":painted["stableTarget"],"subselect":{"cursor":0,"limit":16,"hitTest":true,"atFrame":7}}});
    let response=serde_json::to_value(s.dispatch(serde_json::from_value(q).unwrap()).unwrap()).unwrap();
    assert_eq!(response["ok"],true,"{response}");let page=&response["result"]["subselect"];
    assert_eq!(page["atFrame"],7);assert_eq!(page["total"],painted["count"]);// Frozen path-fx.js@1065e9d8 starts this closed zigzag at [-8,6]
    // (normal to the [3,4] tangent), then the vertex offset adds [4,3].
    assert_eq!(page["segments"][0]["point"],json!([-4.,9.]));
}

#[test]
fn a_masks_vertex_motion_uses_the_same_evaluated_hit_bounds_and_legacy_source_offsets() {
    let mut doc=masked_content();
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"b"},"properties":[{"property":"vtx0","static":[5,-2]}]}));
    let items=listing(&doc).unwrap();let mask=items.iter().find(|i|i["role"]=="mask"&&i["stableTarget"]["strokeId"]=="b").unwrap();
    assert_eq!(mask["bounds"],json!([0.,-2.,10.,10.]));assert_eq!(mask["count"],3);
    assert_eq!(mask["vertexOffsets"],json!([[5.,-2.],[0.,0.],[0.,0.]]));
    let o=owner(&doc);let s=o.acquire_snapshot(0).unwrap();
    let request=json!({"apiVersion":2,"requestId":"mask-vertex-frame","instanceId":s.instance_id(),"documentId":s.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),"stableTarget":mask["stableTarget"],"subselect":{"cursor":0,"limit":16,"hitTest":true,"atFrame":7}}});
    let response=serde_json::to_value(s.dispatch(serde_json::from_value(request).unwrap()).unwrap()).unwrap();
    assert_eq!(response["ok"],true,"{response}");assert_eq!(response["result"]["subselect"]["atFrame"],7);
    assert_eq!(response["result"]["subselect"]["segments"][0]["point"],json!([5.,-2.]));
}

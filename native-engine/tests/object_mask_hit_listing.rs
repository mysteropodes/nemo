//! #2372 SEL-MASK: the hit-test listing of a layer that holds masks lists its content and each mask (role `mask`, with the mode and the
//! layer-shared feather the Mask panel shows); the editable read still refuses such a layer, and a layer with path effects leaves its
//! vector records out item by item instead of refusing the layer.
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

/// Baseline `applyPathVertexOffsets`: a record whose element holder carries `vtxN` offsets is listed as PAINTED: its bounds cover the
/// moved points, and `vertexOffsets` hands the reader the per-anchor offsets to add to the stored anchor points.
#[test]
fn a_record_with_vertex_offsets_lists_its_painted_bounds_and_the_offsets() {
    let plain = listing(&masked_content()).unwrap();
    let stored = plain.iter().find(|i| i["stableTarget"]["strokeId"] == "a" && i["role"] != "mask").unwrap().clone();
    assert!(stored.get("vertexOffsets").is_none(), "{stored}");
    let mut doc = masked_content();
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
        "properties":[{"property":"vtx0","static":[40,30]}]}));
    let items = listing(&doc).unwrap();
    let posed = items.iter().find(|i| i["stableTarget"]["strokeId"] == "a" && i["role"] != "mask").unwrap();
    let count = posed["count"].as_u64().unwrap() as usize;
    let offsets = posed["vertexOffsets"].as_array().unwrap_or_else(|| panic!("offsets are listed: {posed}"));
    assert_eq!(offsets.len(), count);
    assert_eq!(offsets[0], json!([40.0, 30.0]));
    assert!(offsets[1..].iter().all(|o| *o == json!([0.0, 0.0])));
    // The first anchor moved by (40, 30): the box can only have grown towards it.
    let (before, after) = (stored["bounds"].as_array().unwrap(), posed["bounds"].as_array().unwrap());
    assert!(after[2].as_f64().unwrap() >= before[2].as_f64().unwrap() && after[3].as_f64().unwrap() >= before[3].as_f64().unwrap());
    assert_ne!(before, after, "the bounds follow the moved vertex");
    // A sibling without vertex Motion is listed exactly as before.
    assert!(items.iter().filter(|i| i["role"] == "mask").all(|m| m.get("vertexOffsets").is_none()));
}

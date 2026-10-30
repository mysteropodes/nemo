//! #2372: the hit-test listing of a presented layer carries its raster image and text block, at their paint-order position.
use super::*;

fn listing(drawing: &Value) -> Value {
    let owner = owner(drawing);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 0);
    let pins = selector(&snapshot, 0);
    let witness = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let projection = witness
        .subselect_read_projection("L0", None, true)
        .unwrap()
        .with_presentation("native-work-1".into(), 1)
        .unwrap();
    let request = json!({"apiVersion":2,"requestId":"hit","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,
        "workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true}}});
    let response = snapshot
        .dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection))
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
#[test]
fn the_editable_listing_of_a_layer_with_an_image_or_text_stays_refused() {
    let owner = owner(&drawing());
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 0);
    let witness = prepare_object_viewport_composition(&p, &selector(&snapshot, 0), None).unwrap();
    assert!(witness
        .subselect_read_projection("L0", None, false)
        .is_err());
}
fn pair(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}

#[test]
fn the_listing_places_the_text_block_and_the_raster_after_the_vector_with_their_bounds() {
    let result = listing(&drawing());
    assert_eq!(result["hitTest"], true);
    let items = result["items"].as_array().unwrap();
    let roles: Vec<_> = items.iter().map(|i| i["role"].as_str().unwrap()).collect();
    assert_eq!(
        roles,
        ["plain", "text", "raster"],
        "paint order: vector, text, image"
    );
    let strokes: Vec<_> = items
        .iter()
        .map(|i| i["stableTarget"]["strokeId"].as_str().unwrap())
        .collect();
    assert_eq!(
        strokes,
        ["vector", "text", "image"],
        "each carries its own stable target"
    );
    assert_eq!(
        pair(&items[0]["matrix"]),
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        "an unposed vector keeps the identity pose"
    );
    let image = &items[2];
    assert_eq!(
        pair(&image["bounds"]),
        [39.0, 39.0, 41.0, 41.0],
        "centre (40,40), 2x2"
    );
    assert_eq!(pair(&image["matrix"]), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(
        (
            image["closed"].as_bool(),
            image["count"].as_u64(),
            image["filled"].as_bool()
        ),
        (Some(true), Some(0), Some(true))
    );
    let text = &items[1];
    let b = pair(&text["bounds"]);
    assert!(
        b[0] >= 9.0
            && b[0] < 20.0
            && b[1] >= 19.0
            && b[2] > b[0]
            && b[3] > b[1]
            && b[2] < 60.0
            && b[3] < 80.0,
        "the glyph bounds sit near the anchor (10,20) at 24px: {b:?}"
    );
    assert_eq!(
        pair(&text["matrix"]),
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        "text contours are already in world space"
    );
}

#[test]
fn a_rotated_raster_keeps_its_unrotated_rect_and_the_map_about_its_centre() {
    let mut doc = drawing();
    doc["items"]["raster"][0]["item"]["rotation"] = json!(90);
    let result = listing(&doc);
    let image = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["role"] == "raster")
        .unwrap();
    assert_eq!(pair(&image["bounds"]), [39.0, 39.0, 41.0, 41.0]);
    let m = pair(&image["matrix"]);
    // A quarter turn about (40,40): the top-left corner (39,39) lands on the top-right (41,39).
    let at = |x: f64, y: f64| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
    let p = at(39.0, 39.0);
    assert!(
        (p[0] - 41.0).abs() < 1e-9 && (p[1] - 39.0).abs() < 1e-9,
        "{p:?}"
    );
    let c = at(40.0, 40.0);
    assert!(
        (c[0] - 40.0).abs() < 1e-9 && (c[1] - 40.0).abs() < 1e-9,
        "the centre is fixed: {c:?}"
    );
}

#[test]
fn a_texture_anchor_names_its_brush_and_group_for_select() {
    let mut doc = drawing();
    let texture = json!({"brushGroupId":"g","brushTexturePreset":"chalk-round","preTextureOpacity":1});
    doc["objects"][0]["family"] = json!("editable-vector-path");
    doc["objects"][0]["geometry"]["coordinateSpace"] = json!("document-world");
    doc["objects"][0]["geometry"]["handleSpace"] = json!("relative");
    doc["objects"][0]["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"butt","strokeJoin":"miter",
        "miterLimit":10,"dashOffset":0,"opacity":1});
    doc["objects"][0]["vectorExt"] = json!({"brushTexture":texture});
    let items = listing(&doc)["items"].as_array().unwrap().clone();
    let by = |role: &str| items.iter().find(|i| i["role"] == role).unwrap().clone();
    assert_eq!((by("brush")["brush"].clone(), by("brush")["group"].clone()), (json!("anchor"), json!("g")));
    assert!(by("raster").get("brush").is_none(), "an ordinary image names no brush");
    assert!(by("text").get("brush").is_none(), "an ordinary item names no brush");
}

#[test]
fn a_posed_layer_lists_its_image_and_text_in_their_stored_frame_with_the_pose_as_the_map() {
    // #2372: record space is the stored item's own frame, so a Select matrix conjugated with the listing's map is what the
    // item's batch transform applies to the stored centre / anchor.
    let mut doc = drawing();
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","static":[7,9]}]}]);
    let items = listing(&doc)["items"].as_array().unwrap().clone();
    let by = |role: &str| items.iter().find(|i| i["role"] == role).unwrap().clone();
    let (image, text) = (by("raster"), by("text"));
    assert_eq!(pair(&image["bounds"]), [39.0, 39.0, 41.0, 41.0], "the stored centre (40,40) and size, not the posed rect");
    assert_eq!(pair(&image["matrix"]), [1.0, 0.0, 0.0, 1.0, 7.0, 9.0], "the pose carries the stored frame to the painted rect");
    assert_eq!(pair(&text["matrix"]), [1.0, 0.0, 0.0, 1.0, 7.0, 9.0]);
    let b = pair(&text["bounds"]);
    assert!(b[0] >= 9.0 - 1e-9 && b[0] < 20.0 && b[1] >= 19.0 - 1e-9, "the glyph bounds are back in the stored frame near the anchor (10,20): {b:?}");
}

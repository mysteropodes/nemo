//! #2372: the hit-test listing of a presented layer carries its raster image and text block, at their paint-order position.
use super::*;

fn listing(drawing: &Value) -> Value {
    page(drawing, 0)
}
fn page(drawing: &Value, cursor: u64) -> Value {
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
        "workId":"native-work-1","viewGeneration":1,"cursor":cursor,"limit":8,"hitTest":true}}});
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
fn a_stroked_plain_record_lists_its_authored_width_and_an_unstroked_or_image_item_does_not() {
    // #2372: Select pads its reach and hit test with half this width, as the baseline hit the real painted stroke.
    let stroked = |real: bool| {
        let mut doc = drawing();
        doc["objects"][0]["family"] = json!("editable-vector-path");
        doc["objects"][0]["vector"] = json!({"stroke":if real { json!({"kind":"solid","r":0.0,"g":0.0,"b":0.0,"a":1.0}) } else { Value::Null },
            "hasRealStroke":real,"strokeWidth":40,"strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
        listing(&doc)["items"].as_array().unwrap().clone()
    };
    let items = stroked(true);
    assert_eq!(items[0]["role"], "plain");
    assert_eq!(items[0]["strokeWidth"].as_f64(), Some(40.0), "the authored width, in record space");
    assert!(items[1].get("strokeWidth").is_none() && items[2].get("strokeWidth").is_none(), "a text block and an image carry none");
    assert_eq!(stroked(false)[0]["strokeWidth"].as_f64(), Some(0.0), "no real stroke paints no width");
}

#[test]
fn a_record_lists_its_stored_box_angle_and_an_unturned_record_or_an_image_lists_none() {
    // #2372: Select draws its oriented box from this after a frame change, an undo or a reload (baseline serP boxAngle).
    let listed = |ext: Value| {
        let mut doc = drawing();
        doc["objects"][0]["family"] = json!("editable-vector-path");
        doc["objects"][0]["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"butt","strokeJoin":"miter",
            "miterLimit":10,"dashOffset":0,"opacity":1});
        if !ext.is_null() {
            doc["objects"][0]["vectorExt"] = ext;
        }
        listing(&doc)["items"].as_array().unwrap().clone()
    };
    let items = listed(json!({"boxAngle":37.5}));
    assert_eq!(items[0]["role"], "plain");
    assert_eq!(items[0]["boxAngle"].as_f64(), Some(37.5), "the stored rotation of the record's frame, in degrees");
    assert!(items[1].get("boxAngle").is_none() && items[2].get("boxAngle").is_none(), "a text block and an image carry none");
    assert!(listed(Value::Null)[0].get("boxAngle").is_none(), "an unturned record lists none");
    assert!(listed(json!({"boxAngle":0}))[0].get("boxAngle").is_none(), "an explicit 0 is a falsy angle, as serP drops it");
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

fn textured_layer(strokes: usize, copies: usize, anchorless: Option<usize>) -> Value {
    let mut doc = drawing();
    doc.as_object_mut().unwrap().remove("items");
    let template = doc["objects"][0].clone();
    let line = |x0: f64, x1: f64, y: f64| json!({"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":[{"point":{"x":x0,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":x1,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]});
    let (mut objects, mut elements) = (Vec::new(), Vec::new());
    let mut add = |id: String, geometry: Value, texture: Value| {
        let mut r = template.clone();
        r["target"] = support::target("L0", &id);
        r["family"] = json!("editable-vector-path");
        r["geometry"] = geometry;
        r["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"butt","strokeJoin":"miter",
            "miterLimit":10,"dashOffset":0,"opacity":1});
        r["vectorExt"] = json!({"brushTexture":texture});
        elements.push(json!({"layerUid":"L0","strokeId":id,"naturalPosition":objects.len(),"order":{}}));
        objects.push(r);
    };
    for i in 0..strokes {
        let x = i as f64 * 100.0;
        if anchorless != Some(i) {
            add(format!("a{i}"), line(x, x + 50.0, 10.0), json!({"brushGroupId":format!("g{i}"),"brushTexturePreset":"chalk-round","preTextureOpacity":1}));
        }
        for k in 0..copies {
            let y = 40.0 + k as f64;
            add(format!("d{i}_{k}"), line(x + k as f64 * 0.25, x + k as f64 * 0.25 + 2.0, y),
                json!({"brushGroupId":format!("g{i}"),"isBrushTextureCopy":true}));
        }
    }
    doc["objects"] = json!(objects);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!(elements);
    doc
}
fn listed(doc: &Value) -> Vec<Value> {
    let (mut items, mut cursor) = (Vec::new(), 0);
    loop {
        let r = page(doc, cursor);
        items.extend(r["items"].as_array().unwrap().iter().cloned());
        match r["nextCursor"].as_u64() {
            Some(next) => cursor = next,
            None => return items,
        }
    }
}

#[test]
fn ten_textured_strokes_list_as_ten_anchors_whose_bounds_cover_their_copies() {
    // 10 strokes x 170 stamped copies is 1710 records; before, the listing refused past 1024 and Select read nothing.
    let items = listed(&textured_layer(10, 170, None));
    assert_eq!(items.len(), 10, "one anchor per textured stroke, no copy listed");
    for (i, item) in items.iter().enumerate() {
        assert_eq!((item["brush"].as_str(), item["group"].as_str().map(str::to_owned)), (Some("anchor"), Some(format!("g{i}"))));
        let (b, x) = (pair(&item["bounds"]), i as f64 * 100.0);
        assert_eq!((b[0], b[2]), (x, x + 50.0), "the anchor line spans x..x+50 wider than the copies' x extent");
        assert_eq!((b[1], b[3]), (10.0, 40.0 + 169.0), "the box is grown down over the copies' y extent (40..209)");
    }
}

#[test]
fn a_copy_without_a_listed_anchor_stays_listed_as_itself() {
    let items = listed(&textured_layer(2, 3, Some(1)));
    let kinds: Vec<_> = items.iter().map(|i| i["brush"].as_str().unwrap().to_owned()).collect();
    assert_eq!(kinds, ["anchor", "copy", "copy", "copy"], "stroke 0 lists its anchor only, stroke 1 keeps its anchorless copies");
}

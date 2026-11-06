//! #2372: the hit-test listing of a presented layer carries its raster image and text block, at their paint-order position.
use super::*;

fn listing(drawing: &Value) -> Value {
    page(drawing, 0)
}
/// The first page of the hit-test listing of `drawing`'s layer (shared with the rotation tests).
pub(super) fn listing_of(drawing: &Value) -> Value {
    listing(drawing)
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
fn a_record_image_and_text_list_the_style_a_panel_shows() {
    // #2372 SEL-STYLE: the panel shows the selection's style from the listing, not from the transient presented path.
    let mut doc = drawing();
    doc["objects"][0]["family"] = json!("editable-vector-path");
    doc["objects"][0]["fill"] = json!({"kind":"solid","r":1.0,"g":0.0,"b":0.0,"a":0.5019607843137255});
    doc["objects"][0]["vector"] = json!({"stroke":{"kind":"solid","r":0.0,"g":1.0,"b":0.0,"a":1.0},"hasRealStroke":true,"strokeWidth":6,
        "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
    let items = listing(&doc)["items"].as_array().unwrap().clone();
    assert_eq!(items[0]["style"], json!({"fill":"#ff000080","stroke":"#00ff00","width":6.0,"opacity":0.5019607843137255,
        "cap":"butt","join":"miter","miter":10.0,"dashOffset":0.0,"strokeStyle":"solid"}));
    assert_eq!(items[1]["role"], "text");
    assert!(items[1]["style"]["fill"].as_str().is_some_and(|c| c.starts_with('#')), "a text block lists its colour: {}", items[1]);
    assert_eq!(items[2]["style"], json!({"opacity":1.0}), "an image lists its opacity");
    // Unfilled: the item opacity stands in; unstroked: no stroke member.
    doc["objects"][0]["fill"] = Value::Null;
    doc["objects"][0]["vector"]["hasRealStroke"] = json!(false);
    doc["objects"][0]["vector"]["stroke"] = Value::Null;
    doc["objects"][0]["vector"]["opacity"] = json!(0.25);
    assert_eq!(listing(&doc)["items"][0]["style"], json!({"width":6.0,"opacity":0.25,"cap":"butt","join":"miter","miter":10.0,"dashOffset":0.0,"strokeStyle":"solid"}));
    // SEL-STYLE2: the paint order is listed for the panel too.
    doc["objects"][0]["vector"]["paintOrder"] = json!("strokeFirst");
    let style = listing(&doc)["items"][0]["style"].clone();
    assert_eq!(style["paintOrder"], "strokeFirst");
}

#[test]
fn a_record_lists_the_stroke_style_its_dash_pattern_reads_back_as_and_an_anchor_its_preset() {
    // #2372 INSP-READBACK: the panel's Stroke Style and brush preset show the selection's own values (strokeStyle writes [on, off] * width).
    let styled = |dash: Value, width: f64, ext: Value| {
        let mut doc = drawing();
        doc["objects"][0]["family"] = json!("editable-vector-path");
        doc["objects"][0]["vector"] = json!({"stroke":{"kind":"solid","r":0.0,"g":1.0,"b":0.0,"a":1.0},"hasRealStroke":true,"strokeWidth":width,
            "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
        if !dash.is_null() {
            doc["objects"][0]["vector"]["dashArray"] = dash;
        }
        if !ext.is_null() {
            doc["objects"][0]["vectorExt"] = ext;
        }
        listing(&doc)["items"][0]["style"].clone()
    };
    assert_eq!(styled(Value::Null, 4.0, Value::Null)["strokeStyle"], "solid");
    assert_eq!(styled(json!([10.0, 7.2]), 4.0, Value::Null)["strokeStyle"], "dashed");
    assert_eq!(styled(json!([2.0, 4.8]), 4.0, Value::Null)["strokeStyle"], "dotted");
    assert!(styled(json!([3.0, 3.0]), 4.0, Value::Null).get("strokeStyle").is_none(), "a hand-made dash names no style");
    assert!(styled(Value::Null, 4.0, Value::Null).get("brushPreset").is_none(), "a plain stroke has no preset");
    let anchor = json!({"brushTexture":{"brushGroupId":"g","brushTexturePreset":"chalk-round","preTextureOpacity":1}});
    assert_eq!(styled(Value::Null, 4.0, anchor)["brushPreset"], "chalk-round");
    let dab = json!({"brushTexture":{"brushGroupId":"g","isBrushTextureCopy":true}});
    assert!(styled(Value::Null, 4.0, dab).get("brushPreset").is_none(), "a dab carries no preset of its own");
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
fn a_dynamic_shape_lists_its_parameters_without_the_generator_box_and_every_other_item_lists_none() {
    // #2372 insp-parametric: the Rounded corners / Ellipse arc / Star panels read their current values from the listing and send the body back.
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
    let rect = json!({"kind":"rect","tl":6,"tr":6,"br":0,"bl":2.5,"box":{"x1":0,"y1":0,"x2":10,"y2":10}});
    let items = listed(json!({"paramShape":rect}));
    assert_eq!(items[0]["paramShape"], json!({"kind":"rect","tl":6,"tr":6,"br":0,"bl":2.5}), "the kind and its members, never the box");
    assert!(items[1].get("paramShape").is_none() && items[2].get("paramShape").is_none(), "a text block and an image carry none");
    let star = json!({"kind":"star","pointCount":7,"innerRatio":0.4,"cornerRadius":1,"box":{"x1":0,"y1":0,"x2":10,"y2":10}});
    assert_eq!(listed(json!({"paramShape":star}))[0]["paramShape"], json!({"kind":"star","pointCount":7,"innerRatio":0.4,"cornerRadius":1}));
    let ellipse = json!({"kind":"ellipse","startAngle":15,"sweep":300,"innerRadius":0.2});
    assert_eq!(listed(json!({"paramShape":ellipse}))[0]["paramShape"], ellipse, "a shape without a box lists as stored");
    assert!(listed(Value::Null)[0].get("paramShape").is_none(), "an ordinary record lists none");
}

#[test]
fn a_grouped_record_lists_the_groups_it_sits_in_outermost_first_and_a_loose_record_lists_none() {
    // #2376: the Elements panel builds its group tree from the listing; the path is the member tag's group then each parent up.
    // (A layer with a group table paints vectors only: its images and text have no parent chain to sit in.)
    let mut doc = drawing();
    doc.as_object_mut().unwrap().remove("items");
    let order = |ids: &[&str]| -> Vec<Value> {
        ids.iter().enumerate().map(|(n, id)| json!({"layerUid":"L0","strokeId":id,"naturalPosition":n,"order":{}})).collect()
    };
    for id in ["v2", "v3", "v4"] {
        let mut more = doc["objects"][0].clone();
        more["target"] = support::target("L0", id);
        doc["objects"].as_array_mut().unwrap().push(more);
    }
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!(order(&["vector", "v2", "v3", "v4"]));
    let loose = listing(&doc)["items"].as_array().unwrap().clone();
    assert!(loose.iter().all(|i| i.get("groupPath").is_none()), "a layer without a group table lists none");
    doc["layers"][0]["groups"] = json!({"contextId":"scene-root",
        "entries":[{"gid":"outer","name":"Body","order":["inner","v3"]},{"gid":"inner","parent":"outer","order":["vector","v2"]}],
        "members":[{"strokeId":"vector","groupId":"inner"},{"strokeId":"v2","groupId":"inner"},{"strokeId":"v3","groupId":"outer"}]});
    let items = listing(&doc)["items"].as_array().unwrap().clone();
    let path = |id: &str| items.iter().find(|i| i["stableTarget"]["strokeId"] == id).unwrap().get("groupPath").cloned();
    assert_eq!(path("vector"), Some(json!([{"id":"outer","name":"Body"},{"id":"inner"}])), "a nested member: the parent first, an unnamed group has no name");
    assert_eq!(path("v2"), path("vector"), "a sibling shares the inner group");
    assert_eq!(path("v3"), Some(json!([{"id":"outer","name":"Body"}])), "a direct member of the outer group alone");
    assert_eq!(path("v4"), None, "a loose stroke of a grouped layer lists none");
    // The stored combine mode rides on the group's own entry; a legacy group without one lists none (the panel reads that as `none`).
    doc["layers"][0]["groups"]["entries"][0]["combineMode"] = json!("subtract");
    doc["layers"][0]["groups"]["entries"][1]["combineMode"] = json!("none");
    let items = listing(&doc)["items"].as_array().unwrap().clone();
    let path = |id: &str| items.iter().find(|i| i["stableTarget"]["strokeId"] == id).unwrap().get("groupPath").cloned();
    assert_eq!(path("vector"), Some(json!([{"id":"outer","name":"Body","mode":"subtract"},{"id":"inner","mode":"none"}])), "each group lists its own stored mode");
    assert_eq!(path("v3"), Some(json!([{"id":"outer","name":"Body","mode":"subtract"}])));
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

/// #2372 SEL-PAGE: eight hit-test items with 64-character UUID-style ids and full-precision matrices and bounds exceed the 4096-byte
/// response, so a page is shortened by bytes (its cursor advancing by the items actually sent) instead of refusing the whole view.
fn worst_case_items(count: usize, id_len: usize) -> Vec<crate::object_snapshot::SubselectReadItem> {
    use crate::object_codec::{FrameScope, FrameScopeKind, ObjectTarget};
    (0..count)
        .map(|n| {
            let t = 1.0 / 3.0 + n as f64 * 0.1234567890123457;
            crate::object_snapshot::SubselectReadItem {
                stable_target: ObjectTarget {
                    context_id: "scene-root".into(),
                    frame_scope: FrameScope {
                        kind: FrameScopeKind::Authored,
                        frame: 0,
                    },
                    layer_uid: "L0".into(),
                    stroke_id: format!("{n:02}{}", "a1b2c3d4-e5f6-4789-8abc-def012345678".repeat(120))[..id_len].into(),
                },
                closed: true,
                count: 12,
                paint_count: None,
                matrix: [t, -t / 7.0, t / 11.0, t * 1.7, t * 123.456789012345, -t * 98.7654321098765],
                bounds: Some([t * 1.1, t * 2.2, t * 333.3333333333333, t * 444.4444444444444]),
                role: Some("plain"),
                filled: Some(true),
                stroke_width: Some(t * 3.0),
                box_angle: Some(t * 17.0),
                owner: None,
                brush: None,
                group: None,
                style: None,
                param_shape: None,
                group_path: None,
            }
        })
        .collect()
}
fn worst_case_read(items: Vec<crate::object_snapshot::SubselectReadItem>, cursor: u64) -> Value {
    let owner = owner(&drawing());
    let snapshot = support::head(&owner);
    let projection = crate::object_snapshot::SubselectReadProjection::new(&snapshot, 0, "L0", items, None)
        .with_hit_test(true)
        .with_presentation("native-work-1".into(), 1)
        .unwrap();
    let request = json!({"apiVersion":2,"requestId":"hit","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,
        "workId":"native-work-1","viewGeneration":1,"cursor":cursor,"limit":8,"hitTest":true}}});
    let response = snapshot
        .dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection))
        .unwrap();
    let encoded = serde_json::to_vec(&response).unwrap();
    assert!(encoded.len() <= 4096, "{} bytes", encoded.len());
    serde_json::to_value(response).unwrap()
}
#[test]
fn a_worst_case_page_is_shortened_by_bytes_and_the_listing_still_pages_through_every_item() {
    let first = worst_case_read(worst_case_items(20, 64), 0);
    assert!(first["result"]["items"].as_array().unwrap().len() < 8, "eight such items do not fit one 4096-byte page");
    let (mut cursor, mut seen, mut pages, mut shortened) = (0u64, Vec::new(), 0, false);
    loop {
        let response = worst_case_read(worst_case_items(20, 64), cursor);
        assert_eq!(response["ok"], true, "{response}");
        let result = &response["result"];
        let page = result["items"].as_array().unwrap();
        assert!(!page.is_empty() && page.len() <= 8, "{result}");
        assert_eq!(result["total"], json!(20));
        shortened |= page.len() < 8 && result["nextCursor"] != Value::Null;
        seen.extend(page.iter().map(|i| i["stableTarget"]["strokeId"].as_str().unwrap().to_string()));
        pages += 1;
        match result["nextCursor"].as_u64() {
            Some(next) => {
                assert_eq!(next, cursor + page.len() as u64, "the cursor advances by the items actually sent");
                cursor = next;
            }
            None => break,
        }
    }
    assert!(shortened && pages >= 3, "the byte cap must have shortened pages ({pages} pages)");
    let expected: Vec<String> = worst_case_items(20, 64).iter().map(|i| i.stable_target.stroke_id.clone()).collect();
    assert_eq!(seen, expected, "every item is read once, in order");
}
#[test]
fn a_page_that_fits_is_not_shortened_and_one_item_alone_over_the_limit_stays_refused() {
    let small = worst_case_read(worst_case_items(3, 12), 0);
    assert_eq!((small["result"]["items"].as_array().unwrap().len(), small["result"]["nextCursor"].clone()), (3, Value::Null));
    let owner = owner(&drawing());
    let snapshot = support::head(&owner);
    let projection = crate::object_snapshot::SubselectReadProjection::new(&snapshot, 0, "L0", worst_case_items(2, 4000), None)
        .with_hit_test(true)
        .with_presentation("native-work-1".into(), 1)
        .unwrap();
    let request = json!({"apiVersion":2,"requestId":"hit","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"view":{"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,
        "workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true}}});
    let response = serde_json::to_value(
        snapshot
            .dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error"]["message"], "Subselect view exceeds the response byte limit");
}

#[test]
fn a_record_with_a_stored_gradient_lists_it_for_the_fill_row() {
    // #2372 native-fill-gradient: the gradient button and the stop editor show the selection's own stored gradient.
    let mut doc = drawing();
    doc["objects"][0]["family"] = json!("editable-vector-path");
    doc["objects"][0]["fill"] = json!({"kind":"solid","r":1.0,"g":0.0,"b":0.0,"a":1.0});
    doc["objects"][0]["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"butt","strokeJoin":"miter",
        "miterLimit":10,"dashOffset":0,"opacity":1});
    assert!(listing(&doc)["items"][0]["style"].get("fillGradient").is_none(), "a flat fill lists none");
    let gradient = json!({"kind":"radial","from":[1,2],"to":[30,2],"stops":[{"offset":0,"color":"#ff0000"},{"offset":0.75,"color":"#0000ff80"}]});
    doc["objects"][0]["vectorExt"] = json!({"fillGradient": gradient});
    assert_eq!(listing(&doc)["items"][0]["style"]["fillGradient"], gradient, "as stored: kind, record-space ends, stop order and alpha");
}

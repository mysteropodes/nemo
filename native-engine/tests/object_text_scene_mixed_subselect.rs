//! #2372: Select and Direct Select work on a layer that holds vectors together with text or images.
use super::*;
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod edit_support;

/// `drawing()` (a vector, a text block and an image on `L0`) with a plain editable vector record in place of its vector.
fn mixed() -> Value {
    let mut d = drawing();
    d["objects"][0] = edit_support::record_at("A", "L0", 0, "vector");
    d
}

/// One page of the subselect view (the hit-test listing, or the editable one) of the layer `L0` of `doc`.
fn view(doc: &Value, hit_test: bool) -> Result<Value, &'static str> {
    view_of(&owner(doc), hit_test, None)
}
/// The same page read from `owner`'s head, optionally while a vertex `preview` of one of its records is in flight.
fn view_of(owner: &NativeObjectHistory, hit_test: bool, preview: Option<&Value>) -> Result<Value, &'static str> {
    let snapshot = support::head(owner);
    let p = plan(&snapshot, 0);
    let witness = prepare_object_viewport_composition(&p, &selector(&snapshot, 0), None).unwrap();
    let projection = witness
        .subselect_read_projection("L0", preview, hit_test)?
        .with_presentation("native-work-1".into(), 1)
        .unwrap();
    let mut v = json!({"kind":"subselect","contextId":"scene-root","layerUid":"L0","frame":0,
        "workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8});
    if hit_test {
        v["hitTest"] = json!(true);
    }
    if let Some(preview) = preview {
        v["preview"] = preview.clone();
    }
    let request = json!({"apiVersion":2,"requestId":"mixed","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"view":v}});
    let response = snapshot
        .dispatch_with_subselect_view(serde_json::from_value(request).unwrap(), Ok(&projection))
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    Ok(response["result"].clone())
}
/// The per-item geometry page `readPresentedStrokes` (hit test) or `readSubselect` (editable) fetches for one record.
fn geometry(doc: &Value, stroke: &str, hit_test: bool) -> Value {
    let owner = owner(doc);
    let snapshot = support::head(&owner);
    let mut page = json!({"cursor":0,"limit":16});
    if hit_test {
        page["hitTest"] = json!(true);
    }
    let request = json!({"apiVersion":2,"requestId":"geometry","instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"stableTarget":support::target("L0", stroke),"subselect":page}});
    serde_json::to_value(snapshot.dispatch(serde_json::from_value(request).unwrap()).unwrap()).unwrap()
}
fn without(doc: &Value, kind: &str) -> Value {
    let mut d = doc.clone();
    d["items"].as_object_mut().unwrap().remove(kind);
    let drop = if kind == "text" { "text" } else { "image" };
    let elements = d["compositionOrder"]["contexts"][0]["scopes"][0]["elements"].as_array_mut().unwrap();
    elements.retain(|e| e["strokeId"] != drop);
    for (i, e) in elements.iter_mut().enumerate() {
        e["naturalPosition"] = json!(i);
    }
    d
}
fn strokes(items: &Value) -> Vec<&str> {
    items["items"].as_array().unwrap().iter().map(|i| i["stableTarget"]["strokeId"].as_str().unwrap()).collect()
}

#[test]
fn a_vector_text_image_layer_lists_and_pages_its_vector_geometry_for_select() {
    for doc in [mixed(), without(&mixed(), "text"), without(&mixed(), "raster")] {
        let listing = view(&doc, true).unwrap();
        let vector = &listing["items"][0];
        assert_eq!(vector["stableTarget"]["strokeId"], "vector", "{listing}");
        let page = geometry(&doc, "vector", true);
        assert_eq!(page["ok"], true, "{page}");
        let s = &page["result"]["subselect"];
        assert_eq!(s["hitTest"], true);
        assert_eq!(s["total"], vector["count"], "the geometry agrees with the listing's count");
        assert_eq!(s["closed"], vector["closed"], "and with its closed flag");
    }
}
#[test]
fn the_editable_listing_of_a_mixed_layer_carries_only_its_vectors_with_the_hit_listings_pose() {
    for doc in [mixed(), without(&mixed(), "text"), without(&mixed(), "raster")] {
        let hit = view(&doc, true).unwrap();
        let editable = view(&doc, false).unwrap();
        assert_eq!(strokes(&editable), ["vector"], "the image and the text block are never listed for vertex editing: {editable}");
        let (h, e) = (&hit["items"][0], &editable["items"][0]);
        assert_eq!(h["matrix"], e["matrix"], "the listing and the geometry share one pose");
        assert_eq!((h["closed"].clone(), h["count"].clone()), (e["closed"].clone(), e["count"].clone()));
        let page = geometry(&doc, "vector", false);
        assert_eq!(page["ok"], true, "the editable geometry page reads the vector: {page}");
        assert_eq!(page["result"]["subselect"]["total"], e["count"]);
    }
}
#[test]
fn the_editable_read_and_edit_of_the_raster_and_text_items_stay_refused() {
    let doc = mixed();
    for stroke in ["image", "text"] {
        for hit_test in [false] {
            let page = geometry(&doc, stroke, hit_test);
            assert_eq!(page["ok"], false, "{stroke}: {page}");
        }
        let mut owner = owner(&doc);
        let before = owner.content_revision();
        let request = json!({"apiVersion":2,"requestId":"edit","instanceId":owner.instance_id(),"documentId":owner.document_id(),
            "expectedRevision":before,"operation":"command.document.object.subselect.edit","payload":{"command":"object.subselect.edit",
            "stableTarget":support::target("L0", stroke),"intent":{"kind":"move","indices":[0],"delta":[3,4]}}});
        let response = owner.try_dispatch_object_command(&serde_json::from_value(request).unwrap());
        assert!(!matches!(&response, Ok(Some(r)) if r.ok), "{stroke}: a vertex edit of an item that has no vertices is refused");
        assert_eq!(owner.content_revision(), before);
        assert_eq!(owner.history_depths(), (0, 0));
    }
}
#[test]
fn a_subselect_move_of_the_vector_in_a_mixed_layer_is_one_history_entry() {
    let mut owner = owner(&mixed());
    let before = owner.content_revision();
    let first = |o: &crate::history::NativeObjectHistory| support::head(o).document().objects.iter().find(|r| r.target.stroke_id == "vector").unwrap().geometry.segments[0].point.x.as_f64().unwrap();
    let x0 = first(&owner);
    let request = json!({"apiVersion":2,"requestId":"edit","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":before,"operation":"command.document.object.subselect.edit","payload":{"command":"object.subselect.edit",
        "stableTarget":support::target("L0", "vector"),"intent":{"kind":"move","indices":[0],"delta":[3,4]}}});
    let response = owner
        .try_dispatch_object_command(&serde_json::from_value(request).unwrap())
        .unwrap()
        .unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"], json!({"applied":true,"historyEntriesAdded":1}));
    assert_eq!(owner.history_depths(), (1, 0));
    assert!((first(&owner) - x0 - 3.0).abs() < 1e-9, "the vertex moved by the delta");
}
#[test]
fn a_vertex_preview_on_a_rotated_mixed_layer_poses_like_the_committed_edit() {
    // The layer rotates about the centre of everything it paints, so moving a vertex outward moves that pivot: the preview's
    // listing must re-derive it from the edited geometry, or the shape jumps when the drag is released.
    let mut doc = mixed();
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[{"property":"rotation","static":[30]}]}]);
    let intent = json!({"kind":"move","indices":[0],"delta":[400,300]});
    let preview = json!({"stableTarget":support::target("L0", "vector"),"intent":intent});
    let mut owner = owner(&doc);
    let before = view_of(&owner, false, None).unwrap()["items"][0]["matrix"].clone();
    let during = view_of(&owner, false, Some(&preview)).unwrap()["items"][0]["matrix"].clone();
    let request = json!({"apiVersion":2,"requestId":"edit","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":"command.document.object.subselect.edit",
        "payload":{"command":"object.subselect.edit","stableTarget":support::target("L0", "vector"),"intent":intent}});
    let response = owner.try_dispatch_object_command(&serde_json::from_value(request).unwrap()).unwrap().unwrap();
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let after = view_of(&owner, false, None).unwrap()["items"][0]["matrix"].clone();
    let m = |v: &Value| v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect::<Vec<_>>();
    let far = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
    assert!(far(&m(&before), &m(&after)) > 1.0, "the edit moves the layer pivot: {before} -> {after}");
    assert!(far(&m(&during), &m(&after)) < 1e-6, "the preview poses like the committed edit: {during} vs {after}");
}


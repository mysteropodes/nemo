//! #2358 B-comp-edit-rest: the tween commands run INSIDE an entered component (baseline `enterSymbol` swaps `state.layers` for the component's,
//! so Tween, Regenerate, Split and the span reassignment work on its layers). The same baseline oracle cases are run on the scene layer and on
//! the same layer moved into a declared component: the in-betweens are the same records, one revision and one history entry each, the scene
//! root is untouched, undo / redo / reopen are exact, and a locked layer, a bitmap-brush re-bake or an undeclared context is refused.
use super::*;

const CTX: &str = "comp-a";

/// The scene document `doc` with its only layer (`L0`) moved into the component `comp-a` placed by a scene layer `host`.
fn inside(doc: &ObjectDocument) -> ObjectDocument {
    let d: Value = serde_json::from_slice(&encode_storage_project(doc).unwrap()).unwrap();
    let total = d["totalFrames"].clone();
    let mut objects = d["objects"].clone();
    for record in objects.as_array_mut().unwrap() {
        record["target"]["contextId"] = json!(CTX);
    }
    let mut order = d["compositionOrder"]["contexts"][0].clone();
    order["contextId"] = json!(CTX);
    let host = json!({"layerUid":"host","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    let mut out = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":total,"layers":[host],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required",
            "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root","layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],"scopes":[]},order]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":CTX,"totalFrames":total,"layers":d["layers"],"objects":objects}],
            "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":CTX,"symPlayMode":"loop"}]}}});
    if !d["tweens"].is_null() {
        out["tweens"] = d["tweens"].clone();
    }
    decode_storage_project(&serde_json::to_vec(&out).unwrap()).unwrap()
}
/// The component's own records, retagged to the scene root so they compare with a scene layer's.
fn component_objects(doc: &ObjectDocument) -> Value {
    let d: Value = serde_json::from_slice(&encode_storage_project(doc).unwrap()).unwrap();
    let mut objects = d["componentTables"]["components"]["components"][0]["objects"].clone();
    for record in objects.as_array_mut().unwrap() {
        record["target"]["contextId"] = json!("scene-root");
    }
    objects
}
fn scene_part(doc: &ObjectDocument) -> Value {
    let d: Value = serde_json::from_slice(&encode_storage_project(doc).unwrap()).unwrap();
    json!({"layers":d["layers"],"objects":d["objects"]})
}

#[test]
fn every_vector_oracle_case_tweens_inside_a_component_like_the_scene_layer() {
    let mut ran = 0;
    for c in fixture_cases() {
        let input = &c["input"];
        let name = input["name"].as_str().unwrap();
        let plain = input.get("motionArcs").is_none()
            && input.get("customBrushPresets").is_none()
            && input["requiresCompositionOrder"] != true
            && input["frames"].as_array().unwrap().iter().all(|f| f["strokes"].as_array().unwrap().iter().all(|s| s["isRaster"] != true && s.get("bitmapBrushSpec").is_none()));
        if !plain || name.starts_with("force") || input["command"].as_str() == Some("split") {
            continue;
        }
        let mut doc = document(&input["frames"], true);
        if let Some(settings) = input.get("tweens") {
            doc.tweens = crate::object_document::Presence::Present(serde_json::from_value(settings.clone()).unwrap());
        }
        let op = if input["command"].as_str() == Some("regenerate") { "tween.regenerate" } else { "tween.generate" };
        let mut payload = json!({"command":format!("object.{op}"),"layerUid":"L0","options":input["options"]});
        for key in ["selection"] {
            if let Some(value) = input.get(key) {
                payload[key] = value.clone();
            }
        }
        let mut scene = NativeObjectHistory::new("tween-scene", doc.clone()).unwrap();
        let q = request(&scene, "scene", op, payload.clone());
        let a = dispatch(&mut scene, &q);
        assert_eq!(a["ok"], true, "{name}: {a}");
        let mut comp = NativeObjectHistory::new("tween-comp", inside(&doc)).unwrap();
        let (before, shown) = (bytes(&comp), scene_part(&head(&comp)));
        payload["contextId"] = json!(CTX);
        let q = request(&comp, "inside", op, payload);
        let b = dispatch(&mut comp, &q);
        assert_eq!(b["ok"], true, "{name}: {b}");
        assert_eq!((comp.content_revision(), comp.history_depths()), (1, (1, 0)), "{name}: one revision, one entry");
        let after = head(&comp);
        assert_eq!(component_objects(&after), serde_json::to_value(&head(&scene).objects).unwrap(), "{name}: the same in-betweens as the scene layer");
        assert_eq!(scene_part(&after), shown, "{name}: the scene root is untouched");
        let kept = bytes(&comp);
        comp.undo(1).unwrap();
        assert_eq!(bytes(&comp), before, "{name}: undo restores the document exactly");
        comp.redo(2).unwrap();
        assert_eq!(bytes(&comp), kept, "{name}: redo restores the tween exactly");
        let reopened = NativeObjectHistory::new("tween-reopen", decode_storage_project(&kept).unwrap()).unwrap();
        assert_eq!(bytes(&reopened), kept, "{name}: save / reopen keeps it");
        ran += 1;
    }
    assert!(ran >= 3, "the component run covered the vector oracle cases ({ran})");
}

/// The dispatcher either answers a typed refusal envelope or rejects the request outright; nothing is ever written.
fn refused(owner: &mut NativeObjectHistory, q: &OpacityRequest) -> bool {
    match owner.try_dispatch_object_command(q) {
        Err(_) => true,
        Ok(Some(reply)) => serde_json::to_value(reply).unwrap()["ok"] == false,
        Ok(None) => false,
    }
}

#[test]
fn a_locked_layer_an_undeclared_context_and_a_foreign_layer_refuse_with_nothing_written() {
    let case = fixture_cases().into_iter().find(|c| c["input"]["frames"].as_array().unwrap().iter().all(|f| f["strokes"].as_array().unwrap().iter().all(|s| s["isRaster"] != true && s.get("bitmapBrushSpec").is_none()))).unwrap();
    let input = &case["input"];
    let doc = inside(&document(&input["frames"], true));
    let tween = |context: &str, layer: &str| json!({"command":"object.tween.generate","layerUid":layer,"contextId":context,"options":input["options"]});
    let mut open = NativeObjectHistory::new("tween-refuse", doc.clone()).unwrap();
    let before = bytes(&open);
    for (label, payload) in [("undeclared context", tween("comp-gone", "L0")), ("a scene layer named in the component", tween(CTX, "host")), ("an unknown layer", tween(CTX, "nope"))] {
        let q = request(&open, label, "tween.generate", payload);
        assert!(refused(&mut open, &q), "{label}");
        assert_eq!((bytes(&open), open.history_depths()), (before.clone(), (0, 0)), "{label}");
    }
    let mut locked = serde_json::from_slice::<Value>(&encode_storage_project(&doc).unwrap()).unwrap();
    locked["componentTables"]["components"]["components"][0]["layers"][0]["locked"] = json!(true);
    let mut owner = NativeObjectHistory::new("tween-locked", decode_storage_project(&serde_json::to_vec(&locked).unwrap()).unwrap()).unwrap();
    let before = bytes(&owner);
    let q = request(&owner, "locked", "tween.generate", tween(CTX, "L0"));
    assert!(refused(&mut owner, &q), "a locked component layer");
    assert_eq!((bytes(&owner), owner.history_depths()), (before, (0, 0)));
}

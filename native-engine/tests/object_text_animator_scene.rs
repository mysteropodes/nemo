//! Real retained TextAnimator + Motion -> shared Text plan and independent SVG.
//! Source authoring only; every execution gate is UNRUN.
use super::*;

#[test]
fn svg_stored_text_animator_track_and_layer_motion_reach_same_pinned_glyphs_once() {
    let mut doc = drawing_without_raster(true);
    doc["layers"][0]["textAnimators"] = json!([{"id":"retained", "selector":{},
        "props":{"position":[20,0],"opacity":50},
        "properties":[{"property":"amount","track":{"keys":[
            {"frame":0,"v":[0]},{"frame":4,"v":[100]}]}}]}]);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","static":[7,9]},
            {"property":"opacity","static":[50]}]}]);
    let o = owner(&doc);
    let before = support::state(&o);
    let snapshot = support::head(&o);
    let p = plan(&snapshot, 2);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(p.commands().len(), 0);
    assert_eq!(labels(&p), ["scene-root:L0:text"]);
    let text = ordered_entries(&p)
        .find_map(|entry| match entry {
            PaintEntry::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert!(text
        .item
        .painted()
        .all(|(_, rgba)| rgba == [17, 34, 51, 96]));
    let output = export_checked(&p);
    let parsed = reader::parse(&output.bytes).unwrap();
    assert_eq!(parsed.paths.len(), 3);
    // Original Roboto A oracle plus selector amount50: +10px; layer Motion +7,+9.
    // The animator opacity50 at weight0.5 is75%, then layer opacity50 =>96/255.
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for path in &parsed.paths {
        for point in std::iter::once(&path.start).chain(path.curves.iter().map(|c| &c[2])) {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
        assert_eq!(path.alpha, 96.0 / 255.0);
    }
    for (got, want) in bounds
        .into_iter()
        .zip([27.33984375, 31.1375, 42.3515625, 48.2])
    {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
    let pins = selector(&snapshot, 2);
    let viewport = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert_eq!(
        viewport.document_snapshot_id(),
        export.document_snapshot_id()
    );
    let versions = p.scene_resource_versions().unwrap().into_iter().collect();
    viewport.verify_scene_resources(&versions).unwrap();
    export.verify_scene_resources(&versions).unwrap();
    let mut stale = pins.clone();
    stale.frame = 3;
    assert!(prepare_object_composition(&p, &stale).is_err());
    assert!(prepare_object_viewport_composition(&p, &stale, None).is_err());
    assert_eq!(support::state(&o), before);
    let saved = o
        .serialize_project_file(o.content_revision(), snapshot.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("ta-svg-reopen", &saved).unwrap();
    assert_eq!(
        export_checked(&plan(&support::head(&reopened), 2)).bytes,
        output.bytes
    );
    drop(o);
    assert_eq!(export_checked(&p).bytes, output.bytes);
}

#[test]
fn owner_evaluated_text_base_and_layer_motion_fold_with_selector_before_real_parent_svg() {
    let mut doc = drawing_without_raster(true);
    let mut parent = drawing()["objects"][0].clone();
    parent["target"]["layerUid"] = json!("P");
    parent["target"]["strokeId"] = json!("parent-geometry");
    doc["objects"] = json!([parent]);
    let mut parent_layer = doc["layers"][0].clone();
    parent_layer["layerUid"] = json!("P");
    parent_layer["visible"] = json!(false);
    doc["layers"].as_array_mut().unwrap().push(parent_layer);
    doc["layers"][0]["parentLayerUid"] = json!("P");
    doc["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P","naturalPosition":1,"order":{}}));
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P","strokeId":"parent-geometry",
            "naturalPosition":0,"order":{}}));
    doc["layers"][0]["textAnimators"] = json!([{"id":"retained","selector":{},
        "props":{"position":[20,0],"opacity":50},
        "properties":[{"property":"amount","static":[50]}]}]);
    doc["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
            {"property":"position","static":[7,9],"expression":{"enabled":true,"code":"[7,9]"}},
            {"property":"opacity","static":[50]}]},
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"text"},
            "properties":[{"property":"position","static":[3,4],
                "expression":{"enabled":true,"code":"[3,4]"}}]},
        {"target":{"contextId":"scene-root","layerUid":"P"},
            "properties":[{"property":"position","static":[100,-5]}]}]);
    let o = owner(&doc);
    let snapshot = support::head(&o);
    let before = support::state(&o);
    let pins = selector(&snapshot, 2);
    if let Ok(raw) = prepare_object_exposure_frame(&snapshot, &pins) {
        assert!(
            prepare_object_render_scene(&raw, &pins).is_err(),
            "raw snapshot path cannot borrow an owner's enabled-expression proof"
        );
    }
    let mut effects = None;
    let packet = crate::application::prepare_object_motion_exposure_frame_for_owner(
        &o,
        &snapshot,
        &pins,
        &mut effects,
    )
    .unwrap();
    assert!(
        effects.is_none(),
        "clean owner results need no effect finalization"
    );
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let p = prepare_object_paint_plan(&scene, &pins).unwrap();
    assert_eq!(labels(&p), ["scene-root:L0:text"]);
    assert_eq!(
        p.commands().len(),
        0,
        "hidden parent stays provenance, not ordinary paint"
    );
    let output = export_checked(&p);
    let parsed = reader::parse(&output.bytes).unwrap();
    assert_eq!(parsed.paths.len(), 3);
    // Independent original A bounds + element(3,4) + TA(10,0) + layer(7,9) + parent(100,-5).
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for path in &parsed.paths {
        assert_eq!(path.alpha, 96.0 / 255.0);
        for point in std::iter::once(&path.start).chain(path.curves.iter().map(|c| &c[2])) {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
    }
    for (got, want) in bounds
        .into_iter()
        .zip([130.33984375, 30.1375, 145.3515625, 47.2])
    {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
    let viewport = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    let resources = p.scene_resource_versions().unwrap().into_iter().collect();
    viewport.verify_scene_resources(&resources).unwrap();
    export.verify_scene_resources(&resources).unwrap();
    assert_eq!(support::state(&o), before);
    let mut foreign = pins.clone();
    foreign.document_snapshot_id = "foreign-snapshot";
    assert!(prepare_object_render_scene(&packet, &foreign).is_err());
    assert!(prepare_object_composition(&p, &foreign).is_err());
    drop(o);
    assert_eq!(export_checked(&p).bytes, output.bytes);
}

#[test]
fn raw_text_element_motion_without_animators_still_reaches_real_svg() {
    let mut doc = drawing_without_raster(true);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0",
        "strokeId":"text"},"properties":[{"property":"position","static":[3,4]},
        {"property":"opacity","static":[50]}]}]);
    let o = owner(&doc);
    let before = support::state(&o);
    let snapshot = support::head(&o);
    let p = plan(&snapshot, 2);
    let output = export_checked(&p);
    let parsed = reader::parse(&output.bytes).unwrap();
    assert_eq!(parsed.paths.len(), 3);
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for path in &parsed.paths {
        assert_eq!(path.alpha, 128.0 / 255.0);
        for point in std::iter::once(&path.start).chain(path.curves.iter().map(|c| &c[2])) {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
    }
    for (got, want) in bounds
        .into_iter()
        .zip([13.33984375, 26.1375, 28.3515625, 43.2])
    {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
    assert_eq!(support::state(&o), before);
}

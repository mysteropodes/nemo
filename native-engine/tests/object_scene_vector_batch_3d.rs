//! #2520/#2525: the 3D layer in the common vector batch: a gradient keeps its raw endpoints and a layer parented to a 3D layer keeps
//! that parent's 2D Motion. Same fixtures as the parent module.
use super::*;

/// #2520/#2525: a gradient under 3D keeps its raw endpoints (the baseline projects the geometry and leaves the paint alone),
/// where the scene used to refuse the whole frame.
#[test]
fn a_gradient_fill_under_3d_builds_and_keeps_its_raw_endpoints() {
    let mut source = three_d(false);
    source["objects"][0]["vectorExt"] = json!({"fillGradient":{
        "kind":"linear","from":[0,0],"to":[100,60],"stops":[
            {"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}});
    let o = owner(&source);
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 2);
    let plan = plan(&p);
    let ramps: Vec<String> = plan
        .commands()
        .filter_map(|c| c.style().ok().and_then(|style| style.fill.gradient().map(|g| format!("{:?}", g.geometry))))
        .collect();
    assert_eq!(ramps.len(), 1, "the one gradient record keeps its ramp");
    assert_eq!(ramps[0], "Linear { from: [0.0, 0.0], to: [100.0, 60.0] }", "Position (10, 0) moves the geometry, not the ramp");
}

/// #2520/#2525: a layer parented to a 3D layer keeps the chain of that parent's 2D Motion (baseline `computeMotionMat`
/// ignores the parent's 3D), so the frame is available and the child sits where it does under the same parent switched off.
#[test]
fn a_child_of_a_3d_layer_takes_the_parents_2d_motion_like_the_flat_parent() {
    let child_at = |parent_3d: bool| {
        let mut source = three_d(false);
        source["layers"][0]["threeD"] = json!(parent_3d);
        let mut record = source["objects"][0].clone();
        record["target"]["layerUid"] = json!("L2");
        record["target"]["strokeId"] = json!("child");
        source["objects"].as_array_mut().unwrap().push(record);
        source["layers"].as_array_mut().unwrap().push(json!({"layerUid":"L2","parentLayerUid":"L1",
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
        let context = &mut source["compositionOrder"]["contexts"][0];
        context["layers"].as_array_mut().unwrap().push(json!({"layerUid":"L2","naturalPosition":1,"order":{}}));
        context["scopes"][0]["elements"].as_array_mut().unwrap()
            .push(json!({"layerUid":"L2","strokeId":"child","naturalPosition":0,"order":{}}));
        let o = owner(&source);
        let s = o.acquire_snapshot(0).unwrap();
        let p = packet(&o, &s, 2);
        starts(&plan(&p)).into_iter().find(|(id, _)| id == "child").unwrap().1
    };
    let flat = child_at(false);
    let under_3d = child_at(true);
    assert_eq!(flat, under_3d);
    // The parent's Motion (scale 200% / 100%, Position 10) really moved it.
    assert_ne!(flat, [0., 0.]);
}

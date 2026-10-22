//! Literal and scoped document fixtures for the image-mesh binding history contract.
//! These builders use the existing admitted-asset fixture; publication and assertions stay in the history tests.
use super::support;
use serde_json::{json, Value};

pub(super) fn mesh() -> Value {
    // Literal corpus fixture: eight boundary vertices, one interior vertex and eight triangles.
    json!({"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2})
}

pub(super) fn target(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":context,"layerUid":layer,"frameScope":{"kind":"authored","frame":frame},
        "strokeId":stroke})
}

pub(super) fn at(frame: u32) -> Value {
    target("scene-root", "L0", frame, &format!("raster-{frame}"))
}

pub(super) fn inner(frame: u32) -> Value {
    target(
        "component-A",
        "inner-only",
        frame,
        &format!("inner-{frame}"),
    )
}

pub(super) fn entry(target: Value, tag: Option<&str>) -> Value {
    let mut item = support::raster_item("img-1", 3, 5, 200, 100);
    if let Some(id) = tag {
        item["meshId"] = json!(id);
    }
    json!({"target":target,"item":item})
}

pub(super) fn push_raster(doc: &mut Value, target: Value, tag: Option<&str>) {
    doc["items"]["raster"]
        .as_array_mut()
        .unwrap()
        .push(entry(target, tag));
}

pub(super) fn fixture_path(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    let mut path = support::legacy_record(layer, stroke);
    path["target"] = target(context, layer, frame, stroke);
    path
}

pub(super) fn text(target: Value) -> Value {
    json!({"target":target,"block":{"text":"Hi",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}})
}

pub(super) fn drawing(frames: u32, tag: Option<&str>) -> Value {
    let raster: Vec<_> = (0..frames).map(|frame| entry(at(frame), tag)).collect();
    let mut value = support::drawing(json!({"totalFrames":frames,
        "layers":[{"layerUid":"L0"},{"layerUid":"L2"}],"items":{"raster":raster}}));
    if let Some(id) = tag {
        let mut store = serde_json::Map::new();
        store.insert(id.to_string(), mesh());
        value["imageMeshes"] = Value::Object(store);
    }
    value
}

pub(super) fn element(layer: &str, stroke: &str, position: u32) -> Value {
    json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}})
}

pub(super) fn scope(frame: u32, elements: Vec<Value>) -> Value {
    json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
}

pub(super) fn mixed() -> Value {
    let mut value = drawing(4, None);
    value["formatVersion"] = json!(2);
    // Raster-array indices disagree with authored mixed slots; frame 2 has a path at slot 1,
    // and frame 3 has only slot 0. Different stroke IDs across frames must not prevent propagation.
    value["items"]["raster"] = json!([
        entry(at(0), None),
        entry(at(1), None),
        entry(target("scene-root", "L0", 0, "other-0"), None)
    ]);
    value["objects"] = json!([
        (0, "path-0"),
        (2, "path-2-first"),
        (2, "path-2"),
        (3, "path-3")
    ]
    .map(|(frame, stroke)| fixture_path("scene-root", "L0", frame, stroke)));
    value["items"]["text"] = json!([text(target("scene-root", "L0", 1, "text-1"))]);
    let e = |stroke, position| element("L0", stroke, position);
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},
            {"layerUid":"L2","naturalPosition":1,"order":{}}],"scopes":[
            scope(0,vec![e("path-0",0),e("raster-0",1),e("other-0",2)]),
            scope(1,vec![e("text-1",0),e("raster-1",1)]),
            scope(2,vec![e("path-2-first",0),e("path-2",1)]),scope(3,vec![e("path-3",0)])]}]});
    value
}

pub(super) fn component_drawing() -> Value {
    let mut value = drawing(2, Some("im_1"));
    value["formatVersion"] = json!(2);
    for frame in 0..3 {
        push_raster(&mut value, inner(frame), Some("im_1"));
    }
    value["componentTables"] = json!({"components":{"format":"nemo.native-component-set",
        "formatVersion":1,"components":[{"symbolId":"component-A","totalFrames":3,
            "layers":[{"layerUid":"inner-only"}],"objects":[]}],"instances":[]}});
    let root_scopes: Vec<_> = (0..2)
        .map(|frame| scope(frame, vec![element("L0", &format!("raster-{frame}"), 0)]))
        .collect();
    let inner_scopes: Vec<_> = (0..3)
        .map(|frame| {
            scope(
                frame,
                vec![element("inner-only", &format!("inner-{frame}"), 0)],
            )
        })
        .collect();
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
        "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L2","naturalPosition":1,"order":{}}],
            "scopes":root_scopes},{"contextId":"component-A","layers":[
            {"layerUid":"inner-only","naturalPosition":0,"order":{}}],"scopes":inner_scopes}]});
    value
}

pub(super) fn component_mixed() -> Value {
    let mut value = component_drawing();
    let other = target("component-A", "inner-only", 2, "other-inner");
    push_raster(&mut value, other.clone(), Some("im_1"));
    value["componentTables"]["components"]["components"][0]["objects"] =
        json!([0, 2].map(|frame| fixture_path(
            "component-A",
            "inner-only",
            frame,
            &format!("inner-path-{frame}")
        )));
    value["items"]["text"] = json!([text(target("component-A", "inner-only", 1, "inner-text"))]);
    let e = |stroke, position| element("inner-only", stroke, position);
    value["compositionOrder"]["contexts"][1]["scopes"] = json!([
        scope(0, vec![e("inner-path-0", 0), e("inner-0", 1)]),
        scope(1, vec![e("inner-1", 0), e("inner-text", 1)]),
        scope(
            2,
            vec![e("inner-path-2", 0), e("inner-2", 1), e("other-inner", 2)]
        )
    ]);
    value
}

//! #2520/#2525: the `layerBounds` view of the property read. Literal oracles over an admitted document: a drawn layer answers
//! its own tight bounds BEFORE its Motion, a Null answers its rest point, a parented layer names its parent, and the view
//! refuses a stroke holder, a fractional or negative frame and an unknown layer with typed codes. `parentAffine` is the map
//! every ancestor applies to a point already moved by the layer's own Motion (the baseline `outerWorldPoint`).
use crate::object_codec::decode_storage_project;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn rect(layer: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    let corner = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":layer,"strokeId":format!("s{layer}")},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[corner(x, y), corner(x + w, y), corner(x + w, y + h), corner(x, y + h)]},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn layer(uid: &str, parent: Option<&str>, null: bool) -> Value {
    let mut layer = json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875",
        "parentLayerUid":parent,"timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
            "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}});
    if null {
        layer["isNullLayer"] = json!(true);
        layer["nullPos"] = json!([123, 45]);
    }
    layer
}
fn source() -> Value {
    let element = |uid: &str| json!({"layerUid":uid,"strokeId":format!("s{uid}"),"naturalPosition":0,"order":{}});
    let order: Vec<Value> = ["A", "B", "N", "C"]
        .iter()
        .enumerate()
        .map(|(n, u)| json!({"layerUid":u,"naturalPosition":n,"order":{}}))
        .collect();
    let source = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[layer("A", None, false), layer("B", Some("A"), false), layer("N", None, true), layer("C", Some("B"), false)],
        "objects":[rect("A", 20., 30., 100., 60.), rect("B", 200., 10., 50., 40.), rect("C", 0., 0., 10., 10.)],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"A"},
            "properties":[{"property":"position","static":[50, 50]},{"property":"rotation","static":[30]}]}],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":order,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[element("A"), element("B"), element("C")]}]}]}});
    source
}
fn pin() -> ObjectSnapshot {
    ObjectSnapshot::new("layer-bounds", decode_storage_project(source().to_string().as_bytes()).unwrap()).unwrap()
}
fn read(pin: &ObjectSnapshot, target: Value, frame: Value) -> Result<Value, DispatchErrorCode> {
    let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"layerBounds","target":target,"frame":frame}});
    pin.query_property_read_json(selector.to_string().as_bytes())
}
fn layer_target(uid: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":uid})
}

#[test]
fn a_drawn_layer_answers_its_bounds_before_its_motion() {
    let pin = pin();
    let result = read(&pin, layer_target("A"), json!(0)).unwrap();
    assert_eq!(result["kind"], "layerBounds");
    assert_eq!(
        result["bounds"],
        json!([20.0, 30.0, 120.0, 90.0]),
        "position 50,50 and rotation 30 do not move the pre-motion bounds"
    );
    assert_eq!(result["parentLayerUid"], Value::Null);
    assert_eq!(result["frame"], json!(0));
    assert_eq!(result["target"], layer_target("A"));
    assert_eq!(result["atRevision"], json!(pin.content_revision()));
    assert_eq!(result["documentSnapshotId"], json!(pin.snapshot_id()));
}
#[test]
fn a_parented_layer_names_its_parent() {
    let result = read(&pin(), layer_target("B"), json!(0)).unwrap();
    assert_eq!(result["bounds"], json!([200.0, 10.0, 250.0, 50.0]));
    assert_eq!(result["parentLayerUid"], "A");
}
fn affine(result: &Value) -> [f64; 6] {
    let a = result["parentAffine"].as_array().unwrap();
    std::array::from_fn(|i| a[i].as_f64().unwrap())
}
fn close(a: [f64; 6], b: [f64; 6]) {
    assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-9), "{a:?} != {b:?}");
}
#[test]
fn an_unparented_layer_has_the_identity_ancestor_map() {
    close(affine(&read(&pin(), layer_target("A"), json!(0)).unwrap()), [1., 0., 0., 1., 0., 0.]);
    close(affine(&read(&pin(), layer_target("N"), json!(0)).unwrap()), [1., 0., 0., 1., 0., 0.]);
}
#[test]
fn a_parented_layer_carries_the_ancestor_pose_about_the_ancestor_pivot() {
    // A: bounds centre (70, 60), rotation 30 degrees, position (50, 50): p -> pivot + R (p - pivot) + position.
    let (s, c) = 30f64.to_radians().sin_cos();
    let want = [c, s, -s, c, 70. - (c * 70. - s * 60.) + 50., 60. - (s * 70. + c * 60.) + 50.];
    close(affine(&read(&pin(), layer_target("B"), json!(0)).unwrap()), want);
    // C hangs under B, which has no Motion of its own: the chain is the same one pose.
    close(affine(&read(&pin(), layer_target("C"), json!(0)).unwrap()), want);
    // the map takes B's own box corner (200, 10) to where A's pose renders it
    let m = want;
    let (x, y) = (m[0] * 200. + m[2] * 10. + m[4], m[1] * 200. + m[3] * 10. + m[5]);
    assert!((x - (70. + c * 130. - s * -50. + 50.)).abs() < 1e-9 && (y - (60. + s * 130. + c * -50. + 50.)).abs() < 1e-9);
}
#[test]
fn a_null_answers_its_rest_point() {
    let result = read(&pin(), layer_target("N"), json!(3)).unwrap();
    assert_eq!(result["bounds"], json!([123.0, 45.0, 123.0, 45.0]));
}
#[test]
fn the_view_refuses_what_it_cannot_answer() {
    let pin = pin();
    let stroke = json!({"contextId":"scene-root","layerUid":"A","strokeId":"sA"});
    for (target, frame, want) in [
        (stroke, json!(0), DispatchErrorCode::InvalidRequest),
        (
            layer_target("A"),
            json!(1.5),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            layer_target("A"),
            json!(-1),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            layer_target("missing"),
            json!(0),
            DispatchErrorCode::NotFound,
        ),
    ] {
        assert_eq!(
            read(&pin, target.clone(), frame.clone()).unwrap_err(),
            want,
            "{target} at {frame}"
        );
    }
}
#[test]
fn a_stale_snapshot_is_a_stale_revision() {
    let pin = pin();
    let selector = json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision() + 1,
        "documentSnapshotId":pin.snapshot_id(),"view":{"kind":"layerBounds","target":layer_target("A"),"frame":0}});
    assert_eq!(
        pin.query_property_read_json(selector.to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
}

// #2256 B-comp-edit-rest: the same document with ALL its layers moved into the declared component `comp` placed by a scene layer `host`. Its Motion
// holders stay behind: the component paints render a component layer's Opacity only, and the box is read BEFORE the layer's own Motion.
const COMP: &str = "comp";
fn component_pin() -> ObjectSnapshot {
    fn retag(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Object(target)) = map.get_mut("target") {
                    target.insert("contextId".into(), json!(COMP));
                }
                map.values_mut().for_each(retag);
            }
            Value::Array(rows) => rows.iter_mut().for_each(retag),
            _ => {}
        }
    }
    let d = source();
    let (mut objects, mut order) = (d["objects"].clone(), d["compositionOrder"]["contexts"][0].clone());
    retag(&mut objects);
    // The storage format admits no parenting inside a component yet: every component layer is a root.
    let mut layers = d["layers"].clone();
    layers.as_array_mut().unwrap().iter_mut().for_each(|l| l["parentLayerUid"] = Value::Null);
    order["contextId"] = json!(COMP);
    let out = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,"layers":[layer("host", None, false)],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required",
            "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root","layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],"scopes":[]},order]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":COMP,"totalFrames":8,"layers":layers,"objects":objects}],
            "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":COMP,"symPlayMode":"loop"}]}}});
    ObjectSnapshot::new("layer-bounds-component", decode_storage_project(out.to_string().as_bytes()).unwrap()).unwrap()
}
fn component_target(uid: &str) -> Value {
    json!({"contextId":COMP,"layerUid":uid})
}

#[test]
fn a_component_layer_answers_the_same_bounds_and_identity_ancestor_map_as_the_scene_layer() {
    let (scene, inside) = (pin(), component_pin());
    for uid in ["A", "B", "C", "N"] {
        let expected = read(&scene, layer_target(uid), json!(0)).unwrap();
        let got = read(&inside, component_target(uid), json!(0)).unwrap();
        assert_eq!(got["bounds"], expected["bounds"], "{uid}: bounds, before the layer's own Motion");
        assert_eq!(got["parentLayerUid"], Value::Null, "{uid}: a component layer is a root");
        close(affine(&got), [1., 0., 0., 1., 0., 0.]);
        assert_eq!(got["target"], component_target(uid), "{uid}: the answer names the component context");
        assert_eq!(got["frame"], json!(0));
    }
}

#[test]
fn a_component_read_never_answers_for_another_context_or_an_unknown_layer() {
    let inside = component_pin();
    assert_eq!(read(&inside, component_target("nope"), json!(0)), Err(DispatchErrorCode::NotFound));
    assert_eq!(read(&inside, json!({"contextId":"comp-gone","layerUid":"A"}), json!(0)), Err(DispatchErrorCode::NotFound));
    assert_eq!(read(&inside, layer_target("A"), json!(0)), Err(DispatchErrorCode::NotFound), "the scene root has no layer A here");
    assert_eq!(read(&inside, json!({"contextId":COMP,"layerUid":"A","strokeId":"sA"}), json!(0)), Err(DispatchErrorCode::InvalidRequest));
}

//! Controls for the D17 self extent kernel (#2072): in/out points in frames and
//! seconds and `contentBox()` over actual pinned native records. Included as a
//! private child of `property_expression_self_extent.rs`.

use super::{content_box, layer_extent, self_extent, ContentBox, SelfExtentError as E};
use crate::evaluation::property_expression_views::ExpressionContextView;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{Presence, PropertyHolderTarget, RangeError};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn corner(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
fn path(ctx: &str, layer: &str, stroke: &str, frame: u32, closed: bool, pts: &[[f64; 2]]) -> Value {
    let segments: Vec<Value> = pts.iter().map(|p| corner(p[0], p[1])).collect();
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":ctx,"frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":segments},
        "fill":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":3,"strokeCap":"round","strokeJoin":"round","opacity":1}})
}
fn rect(layer: &str, stroke: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    path(
        "scene-root",
        layer,
        stroke,
        0,
        true,
        &[[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
    )
}
/// The timing codec names the scene root for component layers too.
fn timing(keys: &[u32]) -> Value {
    let keys: Vec<Value> = keys.iter().map(|k| json!({"frame":k})).collect();
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys,"samples":[]})
}
fn layer(uid: &str, keys: Option<&[u32]>, range: Option<Value>) -> Value {
    let mut l = json!({"layerUid":uid,"name":uid,"visible":true,"locked":false,"color":"#5fa875"});
    if let Some(keys) = keys {
        l["timing"] = timing(keys);
    }
    if let Some(range) = range {
        l["range"] = range;
    }
    l
}

/// A: two rects, stored in/out 2..20. B: drawn, no range. C: blank key at 10.
/// D: stored edges outside the timeline. P: two coincident anchors. N: nothing drawn.
/// F: f1 at key 0, only f2 at key 5. Total 40 frames.
fn drawing() -> Value {
    let layers = vec![
        layer("A", Some(&[0]), Some(json!({"inPoint":2,"outPoint":20}))),
        layer("B", Some(&[0]), None),
        layer("C", Some(&[0, 10]), None),
        layer("D", Some(&[0]), Some(json!({"inPoint":-5,"outPoint":100}))),
        layer("P", Some(&[0]), None),
        layer("N", Some(&[0]), None),
        layer("F", Some(&[0, 5]), None),
    ];
    let objects = vec![
        rect("A", "a1", 20., 30., 100., 60.),
        rect("A", "a2", 200., 200., 10., 10.),
        rect("B", "b1", 0., 0., 10., 10.),
        rect("C", "c1", 5., 5., 5., 5.),
        rect("D", "d1", 1., 1., 1., 1.),
        path("scene-root", "P", "p1", 0, false, &[[50., 60.], [50., 60.]]),
        rect("F", "f1", 0., 0., 4., 4.),
        path(
            "scene-root",
            "F",
            "f2",
            5,
            true,
            &[[8., 8.], [9., 8.], [9., 9.]],
        ),
    ];
    let element = |(l, s, n): (&str, &str, u32)| json!({"layerUid":l,"strokeId":s,"naturalPosition":n,"order":{}});
    let order_layers: Vec<Value> = ["A", "B", "C", "D", "P", "N", "F"]
        .iter()
        .enumerate()
        .map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}}))
        .collect();
    let elements: Vec<Value> = [
        ("A", "a1", 0),
        ("A", "a2", 1),
        ("B", "b1", 0),
        ("C", "c1", 0),
        ("D", "d1", 0),
        ("P", "p1", 0),
        ("F", "f1", 0),
    ]
    .into_iter()
    .map(element)
    .collect();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":40,
        "layers":layers,"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":order_layers,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements},
                    {"frameScope":{"kind":"authored","frame":5},"elements":[element(("F", "f2", 0))]}]}]}})
}
/// [`drawing`] plus T, a layer with no timing provider (no frame packet admits it).
fn untimed() -> Value {
    let mut d = drawing();
    d["layers"]
        .as_array_mut()
        .unwrap()
        .push(layer("T", None, None));
    let order = &mut d["compositionOrder"]["contexts"][0]["layers"];
    order
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"T","naturalPosition":7,"order":{}}));
    d
}
fn project(drawing: Value, fps: f64) -> NativeObjectHistory {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":fps},"drawing":drawing,
        "viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("self-extent", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn target(ctx: &str, layer: &str, stroke: Option<&str>) -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: ctx.into(),
        layer_uid: layer.into(),
        stroke_id: stroke.map_or(Presence::Missing, |s| Presence::Present(s.into())),
    }
}
fn view(p: &ObjectSnapshot, ctx: &str, layer: &str, stroke: Option<&str>) -> ExpressionContextView {
    ExpressionContextView::bind(p, &target(ctx, layer, stroke), "position").unwrap()
}
fn frames(p: &ObjectSnapshot, uid: &str) -> (f64, f64) {
    let e = self_extent(&view(p, "scene-root", uid, None), p, 3).unwrap();
    (e.in_point(), e.out_point())
}
fn bounds(x: f64, y: f64, width: f64, height: f64) -> ContentBox {
    ContentBox::Bounds {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn in_and_out_points_resolve_in_frames_and_alias_seconds() {
    let p = pin(&project(untimed(), 24.0));
    // Stored edges; the drawn default spans the timeline; a blank key ends the layer
    // the frame before it (autoOutPointFromBlankKeyframe); stored edges are clamped.
    assert_eq!(frames(&p, "A"), (2.0, 20.0));
    assert_eq!(frames(&p, "B"), (0.0, 39.0));
    assert_eq!(frames(&p, "C"), (0.0, 9.0));
    assert_eq!(frames(&p, "D"), (0.0, 39.0));
    let a = self_extent(&view(&p, "scene-root", "A", None), &p, 3).unwrap();
    assert!(a.is_resolved());
    assert_eq!(a.alias_in_point(), Ok(Some(2.0 / 24.0)));
    assert_eq!(a.alias_out_point(), Ok(Some(20.0 / 24.0)));
    // layerSnapshot reads the same frames for any layer by uid.
    assert_eq!(layer_extent(&p, "scene-root", "A", 0), Ok(a));
    // A layer whose default edge needs a timing provider is refused, not defaulted.
    assert_eq!(
        layer_extent(&p, "scene-root", "T", 0),
        Err(E::Range(RangeError::TimingUnavailable))
    );
    assert_eq!(layer_extent(&p, "scene-root", "Z", 0), Err(E::UnknownLayer));
    assert_eq!(layer_extent(&p, "nowhere", "A", 0), Err(E::UnknownContext));
    assert_eq!(
        layer_extent(&p, "scene-root", "A", 40),
        Err(E::Range(RangeError::FrameOutOfRange))
    );
}

#[test]
fn an_unresolved_holder_reads_motion_fallbacks_and_a_null_alias() {
    let p = pin(&project(drawing(), 30.0));
    let v = view(&p, "scene-root", "missing-layer", None);
    assert!(v.self_view().is_none());
    let e = self_extent(&v, &p, 0).unwrap();
    assert_eq!((e.in_point(), e.out_point()), (0.0, 39.0));
    assert!(!e.is_resolved());
    assert_eq!(
        (e.alias_in_point(), e.alias_out_point()),
        (Ok(None), Ok(None))
    );
    assert_eq!(content_box(&v, &p, 0), Ok(ContentBox::Missing));
}

#[test]
fn a_view_bound_to_another_revision_is_refused() {
    let first = pin(&project(drawing(), 24.0));
    let other = pin(&project(drawing(), 25.0));
    let v = view(&first, "scene-root", "A", None);
    assert_eq!(self_extent(&v, &other, 0), Err(E::StaleView));
    assert_eq!(content_box(&v, &other, 0), Err(E::StaleView));
}

#[test]
fn a_bare_drawing_keeps_frames_but_refuses_seconds() {
    let o = NativeObjectHistory::new(
        "bare",
        decode_storage_project(drawing().to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let p = pin(&o);
    let e = self_extent(&view(&p, "scene-root", "A", None), &p, 0).unwrap();
    assert_eq!((e.in_point(), e.out_point()), (2.0, 20.0));
    assert_eq!(e.alias_in_point(), Err(E::NoProject));
    assert_eq!(e.alias_out_point(), Err(E::NoProject));
}

#[test]
fn a_component_uses_its_own_clock() {
    let wire = |fps: Option<f64>| {
        let mut component = json!({"symbolId":"sym","totalFrames":24,
            "layers":[{"layerUid":"in","range":{"inPoint":6,"outPoint":99}},
                {"layerUid":"bare","timing":timing(&[0])}],"objects":[]});
        if let Some(fps) = fps {
            component["fps"] = json!(fps);
        }
        json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":320,"canvasH":180,"fps":24},
            "drawing":{"format":"nemo.native-object-document","formatVersion":2,
                "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[],
                "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
                    "documentFormatVersion":2,"migration":"explicit-plan-required",
                    "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}]},
                "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
                    "components":[component],"instances":[]}}},
            "viewportReference":null,"assets":[]})
    };
    let o = NativeObjectHistory::from_project_file("c", wire(Some(12.0)).to_string().as_bytes())
        .unwrap();
    let p = pin(&o);
    // The out point clamps to the component's 24 frames, seconds use its 12 fps.
    let e = self_extent(&view(&p, "sym", "in", None), &p, 0).unwrap();
    assert_eq!((e.in_point(), e.out_point()), (6.0, 23.0));
    assert_eq!(e.alias_out_point(), Ok(Some(23.0 / 12.0)));
    let e = self_extent(&view(&p, "sym", "bare", None), &p, 0).unwrap();
    assert_eq!((e.in_point(), e.out_point()), (0.0, 23.0));
    // An unresolved holder in the component falls back to the component's frames.
    let e = self_extent(&view(&p, "sym", "gone", None), &p, 0).unwrap();
    assert_eq!(e.out_point(), 23.0);
    // A frame past the component clock is refused, even if the scene has it.
    assert_eq!(
        self_extent(&view(&p, "sym", "in", None), &p, 30),
        Err(E::Range(RangeError::FrameOutOfRange))
    );
    // Without its own fps the component inherits the project fps.
    let o = NativeObjectHistory::from_project_file("d", wire(None).to_string().as_bytes()).unwrap();
    let p = pin(&o);
    let e = self_extent(&view(&p, "sym", "in", None), &p, 0).unwrap();
    assert_eq!(e.alias_in_point(), Ok(Some(6.0 / 24.0)));
}

#[test]
fn content_box_reads_the_pinned_rest_geometry() {
    let p = pin(&project(drawing(), 24.0));
    let layer = view(&p, "scene-root", "A", None);
    assert_eq!(content_box(&layer, &p, 3), Ok(bounds(20., 30., 190., 180.)));
    // An element holder boxes its own stroke only.
    let own = view(&p, "scene-root", "A", Some("a1"));
    assert_eq!(content_box(&own, &p, 3), Ok(bounds(20., 30., 100., 60.)));
    // A holder whose stroke is not drawn at the held key falls back to its layer
    // (liveItemByStrokeId finds no item), here the frame-5 drawing.
    let gone = view(&p, "scene-root", "F", Some("f1"));
    assert_eq!(content_box(&gone, &p, 0), Ok(bounds(0., 0., 4., 4.)));
    assert_eq!(content_box(&gone, &p, 6), Ok(bounds(8., 8., 1., 1.)));
    assert_eq!(
        content_box(&layer, &p, 3).unwrap().motion_value(),
        [20., 30., 190., 180.]
    );
}

#[test]
fn a_zero_size_box_is_not_missing_geometry() {
    let p = pin(&project(drawing(), 24.0));
    let point = content_box(&view(&p, "scene-root", "P", None), &p, 0).unwrap();
    assert_eq!(point, bounds(50., 60., 0., 0.));
    let empty = content_box(&view(&p, "scene-root", "N", None), &p, 0).unwrap();
    assert_eq!(empty, ContentBox::Missing);
    assert_ne!(point, empty);
    // Outside its range the layer shows nothing: missing, not a stale box.
    let late = content_box(&view(&p, "scene-root", "A", None), &p, 30).unwrap();
    assert_eq!(late, ContentBox::Missing);
    // Motion maps both absences to zeros; only a real box keeps its position.
    assert_eq!(empty.motion_value(), [0.0; 4]);
    assert_eq!(point.motion_value(), [50., 60., 0., 0.]);
}

//! Controls for the D16-K2 marker binding (#2072): the D16-K1 marker API over
//! actual pinned native composition and layer records. Included as a private
//! child of `property_expression_marker_binding.rs`.

use super::{composition_markers, snapshot_fps, snapshot_layer_markers, MarkerBindingError as E};
use crate::evaluation::property_expression_lookup::{
    Fps, MarkerEntry, MarkerQuery as Q, MAX_MARKERS,
};
use crate::history::NativeObjectHistory;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn owner(fps: f64, markers: Option<Value>, layers: Value) -> NativeObjectHistory {
    let mut metadata = json!({"canvasW":320,"canvasH":180,"fps":fps});
    if let Some(markers) = markers {
        metadata["markers"] = markers;
    }
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":metadata, "drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":layers,"objects":[]},
        "viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("marker-binding", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn entry(frame: f64, fps: f64, name: &str, color: Option<&str>, index: usize) -> MarkerEntry {
    MarkerEntry {
        frame,
        time: frame / fps,
        name: name.into(),
        color: color.map(str::to_owned),
        index,
    }
}

#[test]
fn composition_markers_keep_stored_order_ties_and_raw_names() {
    let o = owner(
        30.0,
        Some(json!([{"frame":30.5},{"frame":-2,"name":"","color":""},
            {"frame":30.5,"name":"X","color":"blue"},{"frame":0,"name":"X"}])),
        json!([{"layerUid":"L"}]),
    );
    let p = pin(&o);
    let fps = snapshot_fps(&p, "scene-root").unwrap();
    assert_eq!(fps, Fps::new(30.0).unwrap());
    let list = composition_markers(&p, "scene-root").unwrap();
    assert_eq!(list.count(), 4);
    // The first marker named exactly "X"; an empty color reads null.
    assert_eq!(
        list.at(Q::Name("X"), fps),
        Ok(Some(entry(30.5, 30.0, "X", Some("blue"), 3)))
    );
    assert_eq!(
        list.at(Q::Number(2.9), fps),
        Ok(Some(entry(-2.0, 30.0, "", None, 2)))
    );
    assert_eq!(
        list.at(Q::Name(""), fps),
        Ok(Some(entry(30.5, 30.0, "", None, 1)))
    );
    assert_eq!(list.at(Q::Number(5.0), fps), Ok(None));
    assert_eq!(list.at(Q::Number(f64::NAN), fps), Ok(None));
    // Equal distance: the first stored marker wins, sorted or not.
    assert_eq!(
        list.nearest(30.5, fps),
        Some(entry(30.5, 30.0, "", None, 1))
    );
    assert_eq!(
        list.nearest(-1.0, fps),
        Some(entry(-2.0, 30.0, "", None, 2))
    );
    assert_eq!(
        list.nearest_key_seconds(0.01, fps),
        Some(entry(0.0, 30.0, "X", None, 4))
    );
}

#[test]
fn composition_binding_agrees_with_the_pinned_composition_read() {
    let o = owner(
        24.0,
        Some(
            json!([{"frame":12,"name":"in","color":"#5FA875"},{"frame":3},
            {"frame":12,"name":"in"}]),
        ),
        json!([{"layerUid":"L"}]),
    );
    let p = pin(&o);
    let (fps, list) = (
        snapshot_fps(&p, "scene-root").unwrap(),
        composition_markers(&p, "scene-root").unwrap(),
    );
    let read = |selector: Value| {
        let q = json!({"command":"object.project.composition.read",
            "instanceId":p.instance_id(),"documentId":p.document_id(),
            "atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector});
        p.query_composition_metadata_json(q.to_string().as_bytes())
            .unwrap()["result"]
            .clone()
    };
    let as_json = |m: MarkerEntry| json!({"index":m.index,"frame":m.frame,"time":m.time,"name":m.name,"color":m.color});
    for (selector, got) in [
        (
            json!({"kind":"markerAt","index":2}),
            list.at(Q::Number(2.0), fps),
        ),
        (
            json!({"kind":"markerNamed","name":"in"}),
            list.at(Q::Name("in"), fps),
        ),
        (
            json!({"kind":"markerNearest","frame":7.5}),
            Ok(list.nearest(7.5, fps)),
        ),
        (
            json!({"kind":"markerNearest","frame":11.0}),
            Ok(list.nearest(11.0, fps)),
        ),
    ] {
        assert_eq!(read(selector), as_json(got.unwrap().unwrap()));
    }
}

#[test]
fn layer_markers_come_from_each_layers_stored_range() {
    let o = owner(
        24.0,
        Some(json!([{"frame":1,"name":"comp"}])),
        json!([
            {"layerUid":"a","name":"Ink","range":{"inPoint":2,"outPoint":20,
                "markers":[{"frame":12,"name":"α"},{"frame":3,"color":"#e8b64c"},
                {"frame":12,"name":"β","color":""}]}},
            {"layerUid":"b","name":"Bare"},
            {"layerUid":"c","range":{"inPoint":1}}
        ]),
    );
    let p = pin(&o);
    let fps = snapshot_fps(&p, "scene-root").unwrap();
    let a = snapshot_layer_markers(&p, "scene-root", "a").unwrap();
    assert_eq!(a.count(), 3);
    assert_eq!(
        a.at(Q::Name("α"), fps),
        Ok(Some(entry(12.0, 24.0, "α", None, 1)))
    );
    assert_eq!(
        a.at(Q::Number(2.0), fps),
        Ok(Some(entry(3.0, 24.0, "", Some("#e8b64c"), 2)))
    );
    assert_eq!(a.nearest(12.0, fps), Some(entry(12.0, 24.0, "α", None, 1)));
    assert_eq!(a.at(Q::Name("comp"), fps), Ok(None));
    // No range, or a range without markers, reads an empty list.
    for uid in ["b", "c"] {
        let list = snapshot_layer_markers(&p, "scene-root", uid).unwrap();
        assert_eq!(list.count(), 0);
        assert_eq!(list.nearest(0.0, fps), None);
    }
    assert_eq!(
        snapshot_layer_markers(&p, "scene-root", "z").unwrap_err(),
        E::UnknownLayer
    );
    assert_eq!(
        snapshot_layer_markers(&p, "no-such-context", "a").unwrap_err(),
        E::UnknownContext
    );
}

#[test]
fn missing_composition_list_reads_empty_and_bounds_refuse() {
    let o = owner(12.0, None, json!([{"layerUid":"L"}]));
    let p = pin(&o);
    let list = composition_markers(&p, "scene-root").unwrap();
    assert_eq!(list.count(), 0);
    assert_eq!(
        list.at(Q::Number(1.0), snapshot_fps(&p, "scene-root").unwrap()),
        Ok(None)
    );
    // The project codec admits exactly the lookup bound and refuses one more, so
    // the binding's own TooManyMarkers refusal is unreachable from a real pin.
    let list = |n: usize| -> Value { (0..n).map(|f| json!({"frame":f})).collect() };
    let o = owner(12.0, Some(list(MAX_MARKERS)), json!([{"layerUid":"L"}]));
    assert_eq!(
        composition_markers(&pin(&o), "scene-root").unwrap().count(),
        MAX_MARKERS
    );
    let mut metadata = json!({"canvasW":320,"canvasH":180,"fps":12});
    metadata["markers"] = list(MAX_MARKERS + 1);
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":metadata, "drawing":{"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]});
    assert!(NativeObjectHistory::from_project_file("over", wire.to_string().as_bytes()).is_err());
}

#[test]
fn a_component_context_reads_its_own_fps_and_an_empty_composition_list() {
    let wire = |component_fps: Option<f64>| {
        let mut component = json!({"symbolId":"sym","totalFrames":24,
            "layers":[{"layerUid":"in","range":{"inPoint":0,
                "markers":[{"frame":12,"name":"hit"}]}}],"objects":[]});
        if let Some(fps) = component_fps {
            component["fps"] = json!(fps);
        }
        json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":320,"canvasH":180,"fps":24,
                "markers":[{"frame":12,"name":"in"}]},
            "drawing":{"format":"nemo.native-object-document","formatVersion":2,
                "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[],
                "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
                    "documentFormatVersion":2,"migration":"explicit-plan-required",
                    "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}]},
                "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,"components":[component],"instances":[]}}},
            "viewportReference":null,"assets":[]})
    };
    let o = NativeObjectHistory::from_project_file("c", wire(Some(12.0)).to_string().as_bytes())
        .unwrap();
    let p = pin(&o);
    // enterSymbol: state.fps = sym.fps || state.fps, state.markers = sym.markers || [].
    let fps = snapshot_fps(&p, "sym").unwrap();
    assert_eq!(fps, Fps::new(12.0).unwrap());
    assert_eq!(
        snapshot_fps(&p, "scene-root").unwrap(),
        Fps::new(24.0).unwrap()
    );
    let comp = composition_markers(&p, "sym").unwrap();
    assert_eq!(comp.count(), 0);
    assert_eq!(comp.at(Q::Name("in"), fps), Ok(None));
    assert_eq!(composition_markers(&p, "scene-root").unwrap().count(), 1);
    // A layer marker inside the component is timed at the component fps.
    let layer = snapshot_layer_markers(&p, "sym", "in").unwrap();
    assert_eq!(
        layer.at(Q::Name("hit"), fps),
        Ok(Some(entry(12.0, 12.0, "hit", None, 1)))
    );
    assert_eq!(
        layer.nearest_key_seconds(1.0, fps),
        Some(entry(12.0, 12.0, "hit", None, 1))
    );
    // Without its own fps a component inherits the project fps.
    let o = NativeObjectHistory::from_project_file("d", wire(None).to_string().as_bytes()).unwrap();
    assert_eq!(
        snapshot_fps(&pin(&o), "sym").unwrap(),
        Fps::new(24.0).unwrap()
    );
    assert_eq!(
        snapshot_fps(&p, "no-such-context").unwrap_err(),
        E::UnknownContext
    );
    assert_eq!(
        composition_markers(&p, "no-such-context").unwrap_err(),
        E::UnknownContext
    );
}

//! Real sparse drawing sources, not the duplicator oracle's answer-table port.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn drawing(frame: u32, x: i32) -> Value {
    let examples: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = examples["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root", "layerUid":"L0",
        "strokeId":format!("stroke-{frame}"),"frameScope":{"kind":"authored","frame":frame}});
    record["geometry"]["segments"][0]["point"]["x"] = json!(x);
    record
}

fn document() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
        "layers":[{"layerUid":"L0","visible":false,"timing":{
            "format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root",
            "keys":[{"frame":0},{"frame":4},{"frame":6}],"samples":[{"frame":2}]}}],
        "objects":[drawing(0,10),drawing(2,20),drawing(6,60)]})
}

fn snapshot(value: Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "real-duplicator-sources",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}

fn selector(snapshot: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}

#[test]
fn shifted_sources_use_real_keys_samples_holds_and_blank_boundaries_even_when_hidden() {
    let snapshot = snapshot(document());
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    let id = |frame| {
        port.read("L0", frame)
            .unwrap()
            .into_iter()
            .map(|r| r.target().stroke_id().to_owned())
            .collect::<Vec<_>>()
    };
    assert!(port.has_layer("L0"));
    assert!(!port.has_layer("foreign"));
    assert_eq!(id(0.), ["stroke-0"]);
    assert_eq!(id(1.), ["stroke-0"]);
    assert_eq!(id(2.), ["stroke-2"]);
    assert_eq!(id(3.), ["stroke-0"]); // An interpolated sample is NOT a hold key.
    assert!(id(4.).is_empty()); // Blank key stops the older drawing's hold.
    assert!(id(5.).is_empty());
    assert_eq!(id(7.), ["stroke-6"]);
}

#[test]
fn fractional_and_outside_array_frames_are_empty_without_rounding_or_poisoning() {
    let snapshot = snapshot(document());
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    for frame in [-1., 0.5, 2.5, 8., 1e20] {
        assert!(port.strokes_at("L0", frame).is_empty(), "{frame}");
    }
    assert!(port.strokes_at("missing", 0.).is_empty());
    assert_eq!(port.failure.get(), None);
    assert_eq!(
        port.read("L0", f64::NAN),
        Err(ObjectFrameError::Unavailable)
    );
}

#[test]
fn every_returned_copy_is_detached_from_the_retained_snapshot_and_other_frames() {
    let snapshot = snapshot(document());
    let before = encode_storage_project(snapshot.document()).unwrap();
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    let mut copies = port.read("L0", 6.).unwrap();
    copies[0].geometry.segments[0].point.x = serde_json::Number::from(999);
    assert_eq!(
        port.read("L0", 6.).unwrap()[0].geometry.segments[0].point.x,
        serde_json::Number::from(60)
    );
    assert_eq!(
        port.read("L0", 0.).unwrap()[0].geometry.segments[0].point.x,
        serde_json::Number::from(10)
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn missing_timing_is_an_error_and_the_legacy_empty_port_cannot_hide_it() {
    let mut source = document();
    source["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let snapshot = snapshot(source);
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    assert_eq!(port.read("L0", 1.), Err(ObjectFrameError::Unavailable));
    assert!(port.strokes_at("L0", 1.).is_empty());
    assert_eq!(port.failure.get(), Some(ObjectFrameError::Unavailable));
}

#[test]
fn exact_scoped_inputs_preserve_pin_admission_and_nonexposure_source_semantics() {
    let snapshot = snapshot(document());
    let mut selector = selector(&snapshot);
    selector.document_snapshot_id = "foreign";
    assert!(matches!(
        SnapshotSources::new(&snapshot, &selector, true),
        Err(ObjectFrameError::WrongSnapshot)
    ));
    selector.document_snapshot_id = snapshot.snapshot_id();
    let exact = SnapshotSources::new(&snapshot, &selector, false).unwrap();
    assert!(exact.read("L0", 1.).unwrap().is_empty());
    assert_eq!(exact.read("L0", 2.).unwrap().len(), 1);
}

pub(crate) fn duplicated_document(offset: u32) -> Value {
    let mut value = document();
    let scopes = [0, 2, 6].map(|frame| {
        json!({"frameScope":{"kind":"authored","frame":frame},
        "elements":[{"layerUid":"L0","strokeId":format!("stroke-{frame}"),
            "naturalPosition":0,"order":{}}]})
    });
    value["formatVersion"] = json!(2);
    value["layers"][0]["visible"] = json!(true);
    value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
        "formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required",
        "referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
        "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
        "scopes":scopes}]});
    value["componentTables"] = json!({"duplicators":{"format":"nemo.native-duplicator-set",
        "formatVersion":1,"duplicators":[{"contextId":"scene-root","layerUid":"L0",
        "duplicator":{"mode":"grid","rows":1,"cols":3,"spacingX":10,
            "timeOffset":{"enabled":true,"offsetFrames":offset,"direction":"forward"}}}],"effectorLayers":[]}});
    value
}

fn run(snapshot: &ObjectSnapshot, frame: u32) -> Result<Evaluation, ObjectFrameError> {
    use crate::evaluation::object_component_kernels::duplicator_placement_temporal::SpanBounds;
    use crate::evaluation::object_component_kernels::object_duplicator_evaluate::snapshot::SnapshotMotion;
    use crate::evaluation::object_component_kernels::object_duplicator_evaluate::Entry;
    let mut selector = selector(snapshot);
    selector.frame = frame;
    let sources = SnapshotSources::new(snapshot, &selector, true)?;
    let base = snapshot
        .document()
        .objects()
        .iter()
        .filter(|r| r.target().frame() == frame)
        .cloned()
        .collect::<Vec<_>>();
    let request = Request {
        context_id: "scene-root",
        host_uid: "L0",
        set: snapshot.document().duplicator_set().unwrap(),
        layers: snapshot.document().layers(),
        entry: Entry::Rendered { edit_source: false },
        is_3d_layer: false,
        base: &base,
        frame_idx: f64::from(frame),
        canvas_w: 100.,
        canvas_h: 100.,
        span: SpanBounds { start: 0., len: 8. },
    };
    sources.evaluate(
        &request,
        &SnapshotMotion::new(snapshot, "scene-root", f64::from(frame)),
    )
}

#[test]
fn actual_kernel_reads_shifted_stored_drawings_and_preserves_copy_source_identity() {
    let snapshot = snapshot(duplicated_document(1));
    let before = encode_storage_project(snapshot.document()).unwrap();
    let Evaluation::Copies(copies) = run(&snapshot, 2).unwrap() else {
        panic!("expected real copies")
    };
    assert_eq!(copies.copies.len(), 3);
    assert_eq!(
        copies
            .copies
            .iter()
            .map(|c| c.record.target().stroke_id())
            .collect::<Vec<_>>(),
        ["stroke-2", "stroke-0", "stroke-0"]
    );
    assert_eq!(
        copies
            .copies
            .iter()
            .map(|c| c.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [20., 20., 30.]
    );
    assert_eq!(
        copies
            .copies
            .iter()
            .map(|c| c.dup_index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn real_blank_temporal_source_keeps_original_fallback_but_missing_provider_does_not() {
    let value = duplicated_document(2);
    let snapshot_ok = snapshot(value.clone());
    let Evaluation::Copies(copies) = run(&snapshot_ok, 6).unwrap() else {
        panic!("expected copies")
    };
    assert_eq!(
        copies
            .copies
            .iter()
            .map(|c| c.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [60., 70., 40.]
    );
    let mut unavailable = value;
    unavailable["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("timing");
    assert_eq!(
        run(&snapshot(unavailable), 6),
        Err(ObjectFrameError::Unavailable)
    );
}

#[test]
fn effect_layers_have_no_source_drawing_even_when_no_timing_provider_exists() {
    let mut value = document();
    value["objects"] = json!([]);
    value["layers"][0]["isEffectLayer"] = json!(true);
    value["layers"][0].as_object_mut().unwrap().remove("timing");
    let snapshot = snapshot(value);
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    assert!(port.has_layer("L0"));
    assert!(port.read("L0", 2.).unwrap().is_empty());
    assert_eq!(port.failure.get(), None);
}

#[test]
fn stored_effector_role_is_not_treated_as_hidden_pool_drawing_content() {
    let mut value = duplicated_document(0);
    value["componentTables"]["duplicators"]["duplicators"] = json!([]);
    value["componentTables"]["duplicators"]["effectorLayers"] = json!([
        {"contextId":"scene-root","layerUid":"L0","effector":{"targetLayerUid":"removed-host"}}]);
    value["layers"][0].as_object_mut().unwrap().remove("timing");
    let snapshot = snapshot(value);
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    assert!(port.has_layer("L0"));
    assert!(port.strokes_at("L0", 2.).is_empty());
    assert_eq!(port.failure.get(), None);
}

#[test]
fn a_zero_content_role_does_not_hide_its_unavailable_linked_range_provider() {
    let mut value = document();
    value["objects"] = json!([]);
    value["layers"][0]["isEffectLayer"] = json!(true);
    value["layers"][0].as_object_mut().unwrap().remove("timing");
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1","mode":"in","srcAnchor":"in"}});
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"L1","range":{"inPoint":0,"outPoint":7}}));
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"timeLinkInOffset","expression":{"enabled":true,"code":"0"}}]}]);
    let snapshot = snapshot(value);
    let port = SnapshotSources::new(&snapshot, &selector(&snapshot), true).unwrap();
    assert_eq!(port.read("L0", 2.), Err(ObjectFrameError::Unavailable));
}

//! Actual retained project -> production frame -> detached copy expansion.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) fn fixture(offset: u32) -> Value {
    crate::object_frame_packet::duplicator_sources::tests::duplicated_document(offset)
}
pub(crate) fn snapshot(value: Value) -> ObjectSnapshot {
    let document = decode_storage_project(value.to_string().as_bytes()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.).unwrap(), document);
    ObjectSnapshot::from_admitted_project("duplicator-production-frame".into(), project).unwrap()
}
pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

#[test]
fn production_frame_calls_real_temporal_sources_without_persisting_or_selecting_copies() {
    let snapshot = snapshot(fixture(1));
    let before = encode_storage_project(snapshot.document()).unwrap();
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(frame.records().len(), 1); // Original source provenance only.
    assert_eq!(frame.duplicator_outputs().len(), 1);
    let copies = &frame.duplicator_outputs()[0];
    assert_eq!(copies.source_indices, [1, 0, 0]);
    assert_eq!(
        copies
            .output
            .copies
            .iter()
            .map(|copy| copy.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [20., 20., 30.]
    );
    assert_eq!(snapshot.document().objects().len(), 3);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(snapshot.content_revision(), 0);
}

#[test]
fn original_stored_duplicator_properties_reach_the_actual_production_frame() {
    let mut value = fixture(1);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[2,0]}]}]);
    let snapshot = snapshot(value);
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(
        frame.duplicator_outputs()[0]
            .output
            .copies
            .iter()
            .map(|copy| copy.record.geometry.segments[0].point.x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [20., 22., 34.]
    );
}

#[test]
fn expression_backed_duplicate_properties_keep_the_existing_unavailable_admission() {
    let mut value = fixture(1);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[2,0],"expression":{"enabled":true,"code":"[9,9]"}}]}]);
    let snapshot = snapshot(value);
    assert!(matches!(
        prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)),
        Err(crate::object_frame_packet::ObjectFrameError::Unavailable)
    ));
}

pub(crate) fn empty_host_pool() -> Value {
    let mut value = fixture(0);
    value["layers"][0]["visible"] = json!(false);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"H0","timing":{
        "format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    value["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"H0","naturalPosition":1,"order":{}}));
    let descriptor = &mut value["componentTables"]["duplicators"]["duplicators"][0];
    descriptor["layerUid"] = json!("H0");
    descriptor["duplicator"]["sourceMode"] = json!("objects");
    descriptor["duplicator"]["sourceLayerUids"] = json!(["L0"]);
    descriptor["duplicator"]["timeOffset"]["enabled"] = json!(false);
    for record in value["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([
            {"point":{"x":-10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]);
    }
    value
}

#[test]
fn an_empty_host_uses_real_hidden_pool_content_and_never_adds_synthetic_source_records() {
    let snapshot = snapshot(empty_host_pool());
    let before = encode_storage_project(snapshot.document()).unwrap();
    let frame = prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 2)).unwrap();
    assert_eq!(frame.records().len(), 0);
    let copies = &frame.duplicator_outputs()[0];
    assert_eq!(copies.layer_index, 1);
    assert_eq!(copies.source_indices, [1, 1, 1]);
    assert_eq!(
        copies
            .output
            .copies
            .iter()
            .map(|c| [
                c.record.geometry.segments[0].point.x.as_f64().unwrap(),
                c.record.geometry.segments[0].point.y.as_f64().unwrap()
            ])
            .collect::<Vec<_>>(),
        [[40., 45.], [50., 45.], [60., 45.]]
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

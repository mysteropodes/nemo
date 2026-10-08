//! Independent scoped content/provenance oracle; no compositor expectations.
use crate::object_codec::{decode_project, encode_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_frame, ObjectFrameError, ObjectFramePacket, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}
fn source() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases()["records"]})
}
fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "packet-instance",
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn selector(
    snapshot: &ObjectSnapshot,
    kind: FrameScopeKind,
    frame: u32,
) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: kind,
        frame,
    }
}
fn packet(snapshot: &ObjectSnapshot, kind: FrameScopeKind, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(snapshot, &selector(snapshot, kind, frame)).unwrap()
}
fn contents(packet: &ObjectFramePacket) -> Value {
    json!({"instanceId":packet.instance_id(),"documentId":packet.document_id(),
        "contentRevision":packet.content_revision(),"documentSnapshotId":packet.document_snapshot_id(),
        "contextId":packet.context_id(),"scopeKind":packet.scope_kind(),"frame":packet.frame(),
        "layers":packet.source_layers(),"records":packet.records().map(|entry| {
            json!({"sourceLayerIndex":entry.source_layer_index(),
                "sourceObjectIndex":entry.source_object_index(),"record":entry.record()})
        }).collect::<Vec<_>>()})
}

#[test]
fn frozen_scopes_keep_complete_relative_geometry_straight_numbers_and_all_pins() {
    let value = source();
    let snapshot = snapshot(&value);
    let before = encode_project(snapshot.document()).unwrap();
    for (kind, frame, index) in [
        (FrameScopeKind::Authored, 7, 0),
        (FrameScopeKind::Reference, 12, 1),
    ] {
        let selected = packet(&snapshot, kind.clone(), frame);
        assert_eq!(selected.records().len(), 1);
        let entry = selected.records().next().unwrap();
        assert_eq!(entry.source_layer_index(), index);
        assert_eq!(entry.source_object_index(), index);
        assert_eq!(
            serde_json::to_value(entry.record()).unwrap(),
            value["objects"][index]
        );
        assert_eq!(entry.record(), &snapshot.document().objects()[index]);
        assert!(std::ptr::eq(
            entry.record(),
            &snapshot.document().objects()[index]
        ));
        assert_eq!(selected.instance_id(), snapshot.instance_id());
        assert_eq!(selected.document_id(), snapshot.document_id());
        assert_eq!(selected.document_snapshot_id(), snapshot.snapshot_id());
        assert_eq!(selected.content_revision(), 0);
        assert_eq!(selected.context_id(), "scene-root");
        assert_eq!(selected.scope_kind(), &kind);
        assert_eq!(selected.frame(), frame);
        assert_eq!(
            serde_json::to_value(selected.source_layers()).unwrap(),
            value["layers"]
        );
    }
    let authored = packet(&snapshot, FrameScopeKind::Authored, 7);
    let record = authored.records().next().unwrap().record();
    assert_eq!(record.fill.r, json!(0.2).as_number().unwrap().clone());
    assert_eq!(record.fill.g, json!(0.4).as_number().unwrap().clone());
    assert_eq!(record.fill.b, json!(0.6).as_number().unwrap().clone());
    assert_eq!(record.fill.a, json!(0.75).as_number().unwrap().clone());
    assert_eq!(record.geometry.handle_space, "relative");
    assert_eq!(record.geometry.segments[0].point.x.as_f64(), Some(10.125));
    assert_eq!(
        record.geometry.segments[0].handle_out.x.as_f64(),
        Some(3.25)
    );
    assert_eq!(encode_project(snapshot.document()).unwrap(), before);
}

#[test]
fn same_ids_across_scopes_layers_frames_and_multiple_objects_keep_source_provenance() {
    let mut value = source();
    let first = value["objects"][0].clone();
    let reference = value["objects"][1].clone();
    let mut other_layer = first.clone();
    other_layer["target"]["layerUid"] = json!("0");
    let mut other_stroke = first.clone();
    other_stroke["target"]["strokeId"] = json!("0001");
    let mut later = first.clone();
    later["target"]["frameScope"]["frame"] = json!(8);
    let mut same_frame_reference = first.clone();
    same_frame_reference["target"]["frameScope"]["kind"] = json!("reference");
    value["objects"] = json!([
        reference,
        first,
        other_layer,
        other_stroke,
        later,
        same_frame_reference
    ]);
    let snapshot = snapshot(&value);
    let selected = packet(&snapshot, FrameScopeKind::Authored, 7);
    let positions: Vec<_> = selected
        .records()
        .map(|entry| (entry.source_layer_index(), entry.source_object_index()))
        .collect();
    assert_eq!(positions, vec![(0, 1), (1, 2), (0, 3)]);
    for (entry, index) in selected.records().zip([1, 2, 3]) {
        assert_eq!(
            serde_json::to_value(entry.record()).unwrap(),
            value["objects"][index]
        );
        assert!(std::ptr::eq(
            entry.record(),
            &snapshot.document().objects()[index]
        ));
    }
    for (kind, frame, index) in [
        (FrameScopeKind::Authored, 8, 4),
        (FrameScopeKind::Reference, 7, 5),
        (FrameScopeKind::Reference, 12, 0),
    ] {
        let selected = packet(&snapshot, kind, frame);
        assert_eq!(selected.records().len(), 1);
        assert_eq!(
            selected.records().next().unwrap().source_object_index(),
            index
        );
        assert_eq!(
            serde_json::to_value(selected.records().next().unwrap().record()).unwrap(),
            value["objects"][index]
        );
    }
}

#[test]
fn valid_empty_selection_is_distinct_from_invalid_context_or_frame() {
    let snapshot = snapshot(&source());
    for (kind, frame) in [
        (FrameScopeKind::Authored, 0),
        (FrameScopeKind::Authored, 6),
        (FrameScopeKind::Authored, 8),
        (FrameScopeKind::Authored, 12),
        (FrameScopeKind::Reference, 7),
        (FrameScopeKind::Reference, 20),
    ] {
        let selected = packet(&snapshot, kind.clone(), frame);
        assert_eq!(selected.records().len(), 0);
        assert_eq!(selected.frame(), frame);
        assert_eq!(selected.scope_kind(), &kind);
        assert_eq!(selected.source_layers().len(), 2);
    }
    for context in ["", "component-1", "Scene-root", "scene-root\0"] {
        let mut request = selector(&snapshot, FrameScopeKind::Authored, 20);
        request.context_id = context;
        assert_eq!(
            prepare_object_frame(&snapshot, &request).unwrap_err(),
            ObjectFrameError::InvalidContext
        );
    }
    for frame in [21, u32::MAX] {
        assert_eq!(
            prepare_object_frame(
                &snapshot,
                &selector(&snapshot, FrameScopeKind::Authored, frame)
            )
            .unwrap_err(),
            ObjectFrameError::FrameOutOfRange
        );
    }
}

#[test]
fn each_wrong_pin_refuses_even_an_empty_selection_without_mutating_snapshot() {
    let snapshot = snapshot(&source());
    let before = encode_project(snapshot.document()).unwrap();
    let good = selector(&snapshot, FrameScopeKind::Authored, 20);
    let mut wrong_instance = good.clone();
    wrong_instance.instance_id = "other-instance";
    let mut wrong_document = good.clone();
    wrong_document.document_id = "old-document";
    let mut wrong_revision = good.clone();
    wrong_revision.content_revision = 1;
    let mut wrong_snapshot = good.clone();
    wrong_snapshot.document_snapshot_id = "other-snapshot";
    for (request, expected) in [
        (wrong_instance, ObjectFrameError::WrongInstance),
        (wrong_document, ObjectFrameError::WrongDocument),
        (wrong_revision, ObjectFrameError::WrongRevision),
        (wrong_snapshot, ObjectFrameError::WrongSnapshot),
    ] {
        assert_eq!(
            prepare_object_frame(&snapshot, &request).unwrap_err(),
            expected
        );
        assert_eq!(encode_project(snapshot.document()).unwrap(), before);
    }
    assert_eq!(
        prepare_object_frame(&snapshot, &good)
            .unwrap()
            .records()
            .len(),
        0
    );
}

#[test]
fn precise_numbers_and_codec_admitted_geometry_receive_no_renderer_gate() {
    let mut value = source();
    value["objects"][0]["fill"] = json!({"kind":"solid","r":0.20000000000000004,
        "g":0.4000000000000001,"b":0.6000000000000001,"a":0.7500000000000001});
    let segment = value["objects"][0]["geometry"]["segments"][0].clone();
    for count in [2, 256] {
        value["objects"][0]["geometry"]["segments"] = Value::Array(vec![segment.clone(); count]);
        let snapshot = snapshot(&value);
        let selected = packet(&snapshot, FrameScopeKind::Authored, 7);
        let record = selected.records().next().unwrap().record();
        assert!(std::ptr::eq(record, &snapshot.document().objects()[0]));
        assert!(std::ptr::eq(
            &record.geometry.segments[0],
            &snapshot.document().objects()[0].geometry.segments[0]
        ));
        assert_eq!(serde_json::to_value(record).unwrap(), value["objects"][0]);
        assert_eq!(record.geometry.segments.len(), count);
        assert_eq!(record.fill, snapshot.document().objects()[0].fill);
    }
}

#[test]
fn strict_snapshot_admission_cannot_be_bypassed_with_invalid_source_records() {
    for (pointer, replacement) in [
        ("/objects/0/target/contextId", json!("component-1")),
        ("/objects/0/target/layerUid", json!("missing")),
        ("/objects/0/fill/a", json!(1.1)),
        ("/objects/0/target/frameScope/frame", json!(21)),
    ] {
        let mut invalid = source();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        let unchecked = serde_json::from_value(invalid).unwrap();
        assert!(ObjectSnapshot::new("invalid-instance", unchecked).is_err());
    }
    let mut duplicate = source();
    let first = duplicate["objects"][0].clone();
    duplicate["objects"].as_array_mut().unwrap().push(first);
    assert!(decode_project(&serde_json::to_vec(&duplicate).unwrap()).is_err());
}

#[cfg(feature = "history")]
#[test]
fn actual_history_fill_undo_redo_and_reopen_leave_retained_packets_unchanged() {
    use crate::history::NativeObjectHistory;
    let original = source();
    let mut owner = NativeObjectHistory::new(
        "history-packet",
        decode_project(&serde_json::to_vec(&original).unwrap()).unwrap(),
    )
    .unwrap();
    let zero = owner.acquire_snapshot(0).unwrap();
    let retained = packet(&zero, FrameScopeKind::Authored, 7);
    let original_packet = contents(&retained);
    let reference = contents(&packet(&zero, FrameScopeKind::Reference, 12))["records"].clone();
    let payload = cases()["command"]["request"]["payload"].clone();
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    let one = owner.acquire_snapshot(1).unwrap();
    assert_eq!(owner.undo(1), Ok(2));
    let two = owner.acquire_snapshot(2).unwrap();
    assert_eq!(owner.redo(2), Ok(3));
    let three = owner.acquire_snapshot(3).unwrap();
    for (snapshot, revision, expected) in [
        (&zero, 0, original["objects"][0].clone()),
        (&one, 1, cases()["command"]["afterRecord"].clone()),
        (&two, 2, original["objects"][0].clone()),
        (&three, 3, cases()["command"]["afterRecord"].clone()),
    ] {
        let selected = packet(snapshot, FrameScopeKind::Authored, 7);
        assert_eq!(selected.content_revision(), revision);
        assert_eq!(selected.document_snapshot_id(), snapshot.snapshot_id());
        assert_eq!(
            serde_json::to_value(selected.records().next().unwrap().record()).unwrap(),
            expected
        );
        assert_eq!(
            contents(&packet(snapshot, FrameScopeKind::Reference, 12))["records"],
            reference
        );
        assert_eq!(contents(&retained), original_packet);
    }
    let mut stale = selector(&three, FrameScopeKind::Authored, 7);
    stale.document_snapshot_id = zero.snapshot_id();
    assert_eq!(
        prepare_object_frame(&three, &stale).unwrap_err(),
        ObjectFrameError::WrongSnapshot
    );
    let saved = encode_project(three.document()).unwrap();
    drop(owner);
    drop(zero);
    let reopened =
        NativeObjectHistory::new("history-packet", decode_project(&saved).unwrap()).unwrap();
    assert_eq!(reopened.content_revision(), 0);
    assert_eq!(reopened.history_depths(), (0, 0));
    let fresh = reopened.acquire_snapshot(0).unwrap();
    assert_ne!(fresh.document_id(), retained.document_id());
    assert_eq!(
        prepare_object_frame(&fresh, &selector(&three, FrameScopeKind::Authored, 7)).unwrap_err(),
        ObjectFrameError::WrongDocument
    );
    assert_eq!(encode_project(fresh.document()).unwrap(), saved);
    assert_eq!(contents(&retained), original_packet);
    let mut detached = contents(&retained);
    detached["records"][0]["record"]["fill"]["a"] = json!(0);
    assert_eq!(contents(&retained), original_packet);
}

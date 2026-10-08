//! Fixed contour/source oracles for the order-neutral production consumer.
use crate::object_codec::{decode_project, encode_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_render_scene, ObjectRenderScene, ObjectSceneError,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::PathEl;

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
        "contour-instance",
        decode_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}
fn selector(packet: &ObjectFramePacket) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    }
}
fn packet(snapshot: &ObjectSnapshot, kind: FrameScopeKind, frame: u32) -> ObjectFramePacket {
    prepare_object_frame(
        snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: kind,
            frame,
        },
    )
    .unwrap()
}
fn scene(packet: &ObjectFramePacket) -> ObjectRenderScene {
    prepare_object_render_scene(packet, &selector(packet)).unwrap()
}
fn contents(scene: &ObjectRenderScene) -> Value {
    json!({"documentId":scene.packet().document_id(),
        "revision":scene.packet().content_revision(),
        "snapshotId":scene.packet().document_snapshot_id(),
        "records":scene.entries().map(|entry| json!({
            "layerIndex":entry.source().source_layer_index(),
            "objectIndex":entry.source().source_object_index(),
            "record":entry.source().record(),"bounds":entry.envelope()
        })).collect::<Vec<_>>()})
}

#[test]
fn frozen_authored_and_reference_compile_exact_controls_and_borrow_source() {
    let mut value = source();
    value["objects"][0]["fill"] = json!({"kind":"solid","r":0.20000000000000004,
        "g":0.4000000000000001,"b":0.6000000000000001,"a":0.7500000000000001});
    let snapshot = snapshot(&value);
    let before = encode_project(snapshot.document()).unwrap();
    let expected = [
        vec![
            PathEl::MoveTo((10.125, 20.).into()),
            PathEl::CurveTo((13.375, 19.).into(), (37., 22.).into(), (40., 20.).into()),
            PathEl::CurveTo((40., 26.).into(), (29., 48.).into(), (25., 50.).into()),
            PathEl::CurveTo((20., 47.).into(), (8.125, 20.).into(), (10.125, 20.).into()),
            PathEl::ClosePath,
        ],
        vec![
            PathEl::MoveTo((-10., 0.).into()),
            PathEl::CurveTo((-10., 0.).into(), (10., 0.).into(), (10., 0.).into()),
            PathEl::CurveTo((10., 0.).into(), (0., 10.).into(), (0., 10.).into()),
            PathEl::CurveTo((0., 10.).into(), (-10., 0.).into(), (-10., 0.).into()),
            PathEl::ClosePath,
        ],
    ];
    for (index, kind, frame, bounds) in [
        (0, FrameScopeKind::Authored, 7, [8.125, 19., 40., 50.]),
        (1, FrameScopeKind::Reference, 12, [-10., 0., 10., 10.]),
    ] {
        let packet = packet(&snapshot, kind, frame);
        let prepared = scene(&packet);
        assert_eq!(prepared.entries().len(), 1);
        let entry = prepared.entries().next().unwrap();
        assert_eq!(entry.contour().commands(), expected[index]);
        assert_eq!(entry.contour().control_bounds(), bounds);
        assert_eq!(entry.envelope(), bounds);
        assert_eq!(entry.source().source_layer_index(), index);
        assert_eq!(entry.source().source_object_index(), index);
        let record = entry.source().record();
        assert!(std::ptr::eq(record, &snapshot.document().objects()[index]));
        assert!(std::ptr::eq(
            &record.geometry.segments[0],
            &snapshot.document().objects()[index].geometry.segments[0]
        ));
        assert_eq!(
            serde_json::to_value(record).unwrap(),
            value["objects"][index]
        );
        assert_eq!(record.fill, snapshot.document().objects()[index].fill);
        assert_eq!(encode_project(snapshot.document()).unwrap(), before);
    }
}

#[test]
fn curved_cap_is_a_real_contour_and_clones_share_compiled_storage() {
    let mut value = source();
    value["objects"][0]["geometry"]["segments"] = json!([
        {"point":{"x":40,"y":40},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":40}},
        {"point":{"x":80,"y":40},"handleIn":{"x":0,"y":40},"handleOut":{"x":0,"y":0}}
    ]);
    let snapshot = snapshot(&value);
    let packet = packet(&snapshot, FrameScopeKind::Authored, 7);
    let prepared = scene(&packet);
    let cloned = prepared.clone();
    let a = prepared.entries().next().unwrap();
    let b = cloned.entries().next().unwrap();
    assert_eq!(
        a.contour().commands(),
        [
            PathEl::MoveTo((40., 40.).into()),
            PathEl::CurveTo((40., 80.).into(), (80., 80.).into(), (80., 40.).into()),
            PathEl::CurveTo((80., 40.).into(), (40., 40.).into(), (40., 40.).into()),
            PathEl::ClosePath,
        ]
    );
    assert_eq!(a.envelope(), [40., 40., 80., 80.]);
    assert!(std::ptr::eq(a.contour(), b.contour()));
    assert!(std::ptr::eq(
        a.contour().commands().as_ptr(),
        b.contour().commands().as_ptr()
    ));
    assert!(std::ptr::eq(a.source().record(), b.source().record()));
    let retained = contents(&cloned);
    drop(prepared);
    drop(packet);
    drop(snapshot);
    assert_eq!(contents(&cloned), retained);
}

#[test]
fn complete_scoped_targets_and_multiple_objects_per_layer_remain_distinct() {
    let mut value = source();
    let first = value["objects"][0].clone();
    for (layer, kind, frame, stroke) in [
        ("0", "authored", 7, "ink/path_A"),
        ("legacy layer:alpha", "authored", 8, "ink/path_A"),
        ("legacy layer:alpha", "reference", 7, "ink/path_A"),
        ("legacy layer:alpha", "authored", 7, "second"),
    ] {
        let mut record = first.clone();
        record["target"]["layerUid"] = json!(layer);
        record["target"]["frameScope"] = json!({"kind":kind,"frame":frame});
        record["target"]["strokeId"] = json!(stroke);
        value["objects"].as_array_mut().unwrap().push(record);
    }
    let snapshot = snapshot(&value);
    for (kind, frame, indices) in [
        (FrameScopeKind::Authored, 7, vec![(0, 0), (1, 2), (0, 5)]),
        (FrameScopeKind::Authored, 8, vec![(0, 3)]),
        (FrameScopeKind::Reference, 7, vec![(0, 4)]),
        (FrameScopeKind::Reference, 12, vec![(1, 1)]),
    ] {
        let prepared = scene(&packet(&snapshot, kind, frame));
        assert_eq!(prepared.entries().len(), indices.len());
        for (entry, (layer, object)) in prepared.entries().zip(indices) {
            assert_eq!(entry.source().source_layer_index(), layer);
            assert_eq!(entry.source().source_object_index(), object);
            assert!(std::ptr::eq(
                entry.source().record(),
                &snapshot.document().objects()[object]
            ));
        }
    }
}

#[test]
fn every_pin_scope_and_frame_must_match_even_for_valid_empty_packets() {
    let snapshot = snapshot(&source());
    for frame in [7, 0] {
        let packet = packet(&snapshot, FrameScopeKind::Authored, frame);
        let good = selector(&packet);
        let mut requests = Vec::new();
        let mut wrong = good.clone();
        wrong.instance_id = "foreign";
        requests.push((wrong, ObjectSceneError::WrongInstance));
        let mut wrong = good.clone();
        wrong.document_id = "foreign";
        requests.push((wrong, ObjectSceneError::WrongDocument));
        let mut wrong = good.clone();
        wrong.content_revision += 1;
        requests.push((wrong, ObjectSceneError::WrongRevision));
        let mut wrong = good.clone();
        wrong.document_snapshot_id = "foreign";
        requests.push((wrong, ObjectSceneError::WrongSnapshot));
        let mut wrong = good.clone();
        wrong.context_id = "component-1";
        requests.push((wrong, ObjectSceneError::InvalidContext));
        let mut wrong = good.clone();
        wrong.scope_kind = FrameScopeKind::Reference;
        requests.push((wrong, ObjectSceneError::WrongScope));
        for other in [frame + 1, 21] {
            let mut wrong = good.clone();
            wrong.frame = other;
            requests.push((wrong, ObjectSceneError::WrongFrame));
        }
        for (request, expected) in requests {
            assert_eq!(
                prepare_object_render_scene(&packet, &request).unwrap_err(),
                expected
            );
        }
        assert_eq!(
            scene(&packet).entries().len(),
            if frame == 7 { 1 } else { 0 }
        );
    }
}

fn segments(points: &[[f64; 2]]) -> Value {
    json!(points
        .iter()
        .map(|[x, y]| json!({"point":{"x":x,"y":y},
        "handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}))
        .collect::<Vec<_>>())
}

#[test]
fn codec_admitted_renderer_refusals_are_atomic_after_an_earlier_valid_record() {
    let mut overflow = cases()["records"][0]["geometry"]["segments"].clone();
    overflow[0]["point"]["x"] = json!(1e308);
    overflow[0]["handleOut"]["x"] = json!(1e308);
    for (bad, reason) in [
        (
            segments(&[[0., 0.], [0., 0.]]),
            "cubic control bounds must have finite positive extent",
        ),
        (
            segments(&[[0., 0.], [1., 1.], [2., 2.]]),
            "cubic control hull must have positive area",
        ),
        (
            segments(&[
                [16777216., 0.],
                [16777217., 0.],
                [16777216., 20.],
                [16777196., 20.],
            ]),
            "cubic segment is degenerate at Vello's local encoding precision",
        ),
        (overflow, "absolute cubic controls must be finite"),
        (
            segments(&[[1e40, 0.], [2e40, 0.], [1e40, 1e40]]),
            "cubic control bounds must have finite positive extent",
        ),
        (
            segments(&[[1000000., 0.], [1000020., 0.], [1000000., 20.]]),
            "cubic GPU rounding bound exceeds the output precision budget",
        ),
    ] {
        let mut value = source();
        let mut record = value["objects"][0].clone();
        record["target"]["strokeId"] = json!("unsupported-second");
        record["geometry"]["segments"] = bad;
        value["objects"].as_array_mut().unwrap().push(record);
        let snapshot = snapshot(&value);
        let before = encode_project(snapshot.document()).unwrap();
        let packet = packet(&snapshot, FrameScopeKind::Authored, 7);
        assert_eq!(packet.records().len(), 2);
        assert_eq!(
            prepare_object_render_scene(&packet, &selector(&packet)).unwrap_err(),
            ObjectSceneError::UnsupportedGeometry {
                source_object_index: 2,
                reason
            }
        );
        assert_eq!(encode_project(snapshot.document()).unwrap(), before);
    }
}

#[cfg(feature = "history")]
#[test]
fn actual_history_fill_undo_redo_and_reopen_preserve_old_contours_and_fresh_pins() {
    use crate::history::NativeObjectHistory;
    let original = source();
    let mut owner = NativeObjectHistory::new(
        "contour-instance",
        decode_project(&serde_json::to_vec(&original).unwrap()).unwrap(),
    )
    .unwrap();
    let zero = owner.acquire_snapshot(0).unwrap();
    let old_packet = packet(&zero, FrameScopeKind::Authored, 7);
    let retained = scene(&old_packet);
    let original_scene = contents(&retained);
    let reference =
        contents(&scene(&packet(&zero, FrameScopeKind::Reference, 12)))["records"].clone();
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&cases()["command"]["request"]["payload"]).unwrap())
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
        let current_packet = packet(snapshot, FrameScopeKind::Authored, 7);
        let current = scene(&current_packet);
        assert_eq!(current.packet().content_revision(), revision);
        assert_eq!(
            current.packet().document_snapshot_id(),
            snapshot.snapshot_id()
        );
        let entry = current.entries().next().unwrap();
        assert_eq!(
            serde_json::to_value(entry.source().record()).unwrap(),
            expected
        );
        assert_eq!(
            entry.contour().commands(),
            retained.entries().next().unwrap().contour().commands()
        );
        assert_eq!(
            contents(&scene(&packet(snapshot, FrameScopeKind::Reference, 12)))["records"],
            reference
        );
        assert_eq!(contents(&retained), original_scene);
    }
    let current_packet = packet(&three, FrameScopeKind::Authored, 7);
    let mut stale = selector(&current_packet);
    stale.document_snapshot_id = zero.snapshot_id();
    assert_eq!(
        prepare_object_render_scene(&current_packet, &stale).unwrap_err(),
        ObjectSceneError::WrongSnapshot
    );
    let saved = encode_project(three.document()).unwrap();
    drop(owner);
    let reopened =
        NativeObjectHistory::new("contour-instance", decode_project(&saved).unwrap()).unwrap();
    let fresh = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(reopened.history_depths(), (0, 0));
    let fresh_packet = packet(&fresh, FrameScopeKind::Authored, 7);
    assert_ne!(fresh_packet.document_id(), old_packet.document_id());
    assert_eq!(
        prepare_object_render_scene(&fresh_packet, &selector(&old_packet)).unwrap_err(),
        ObjectSceneError::WrongDocument
    );
    assert_eq!(scene(&fresh_packet).packet().content_revision(), 0);
    assert_eq!(encode_project(fresh.document()).unwrap(), saved);
    assert_eq!(contents(&retained), original_scene);
}

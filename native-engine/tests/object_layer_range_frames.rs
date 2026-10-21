//! Actual immutable frame packet outputs and unchanged refusal gates, not resolver-only tests.
use super::tests::{admit, drawing};
use crate::object_codec::encode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFrameError, ObjectFrameSelector,
};
use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn snapshot(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new("range-frames", admit(value)).unwrap()
}
fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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
fn v1_and_v2_real_exposure_packets_hide_outside_ranges_preserve_hold_sources_and_storage() {
    for v2 in [false, true] {
        let mut value = drawing(2, 8, v2);
        value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":4});
        let snapshot = snapshot(&value);
        let bytes = encode_storage_project(snapshot.document()).unwrap();
        for frame in 0..8 {
            let packet =
                prepare_object_exposure_frame(&snapshot, &selector(&snapshot, frame)).unwrap();
            assert_eq!(packet.frame(), frame);
            let active = (2..=4).contains(&frame);
            assert_eq!(packet.source_frame_for_layer(0), active.then_some(0));
            assert_eq!(packet.source_frame_for_layer(1), Some(0));
            let ids: Vec<_> = packet
                .records()
                .map(|record| record.record().target().stroke_id().to_owned())
                .collect();
            assert_eq!(ids, if active { vec!["s0", "s1"] } else { vec!["s1"] });
            assert_eq!(packet.document_snapshot_id(), snapshot.snapshot_id());
            assert_eq!(packet.content_revision(), snapshot.content_revision());
        }
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), bytes);
    }
}

#[test]
fn root_packet_consumes_actual_static_offsets_and_refuses_active_expression() {
    let mut value = drawing(2, 8, true);
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1","mode":"in"}});
    value["layers"][1]["range"] = json!({"inPoint":2});
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"timeLinkInOffset","static":[2]}]}]);
    let plain = snapshot(&value);
    for (frame, shown) in [(3, false), (4, true), (7, true)] {
        let packet = prepare_object_exposure_frame(&plain, &selector(&plain, frame)).unwrap();
        assert_eq!(packet.source_frame_for_layer(0), shown.then_some(0));
    }
    value["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":true,"code":"value+frame"});
    let expression = snapshot(&value);
    assert_eq!(
        prepare_object_exposure_frame(&expression, &selector(&expression, 4)).err(),
        Some(ObjectFrameError::Unavailable)
    );
}

#[test]
fn automatic_defaults_do_not_clip_a_manual_sample_when_no_range_is_active() {
    let mut value = drawing(1, 8, false);
    value["layers"][0]["timing"]["keys"] = json!([{"frame":0},{"frame":5}]);
    value["layers"][0]["timing"]["samples"] = json!([{"frame":6,"manualEdit":true}]);
    let mut sample = value["objects"][0].clone();
    sample["target"]["strokeId"] = json!("manual");
    sample["target"]["frameScope"]["frame"] = json!(6);
    value["objects"].as_array_mut().unwrap().push(sample);
    let plain = snapshot(&value);
    let packet = prepare_object_exposure_frame(&plain, &selector(&plain, 6)).unwrap();
    assert_eq!(packet.source_frame_for_layer(0), Some(6));
    assert_eq!(
        packet
            .records()
            .next()
            .unwrap()
            .record()
            .target()
            .stroke_id(),
        "manual"
    );
    value["layers"][0]["range"] = json!({"inPoint":0});
    let ranged = snapshot(&value);
    let packet = prepare_object_exposure_frame(&ranged, &selector(&ranged, 6)).unwrap();
    assert_eq!(packet.source_frame_for_layer(0), None);
    assert_eq!(packet.records().len(), 0);
}

#[test]
fn missing_timing_outside_the_window_is_still_unavailable_not_fake_empty() {
    let mut value = drawing(1, 8, false);
    value["layers"][0].as_object_mut().unwrap().remove("timing");
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":4});
    let snapshot = snapshot(&value);
    assert_eq!(
        prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 0)).err(),
        Some(ObjectFrameError::Unavailable)
    );
    // The legacy exact-frame route can use two explicit edges without inventing timing.
    assert_eq!(
        prepare_object_frame(&snapshot, &selector(&snapshot, 0))
            .unwrap()
            .records()
            .len(),
        0
    );
}

#[test]
fn source_range_support_does_not_admit_unsupported_visible_vector_parenting() {
    let mut value = drawing(2, 8, true);
    value["layers"][0]["parentLayerUid"] = json!("L1");
    // The static2D parent producer is admitted; a3D ancestor is still unjoined.
    value["layers"][1]["threeD"] = json!(true);
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":4});
    let snapshot = snapshot(&value);
    let bytes = encode_storage_project(snapshot.document()).unwrap();
    assert_eq!(
        prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 3)).err(),
        Some(ObjectFrameError::Unavailable)
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), bytes);
}

#[test]
fn source_range_static_parent_motion_reaches_real_paint_with_multiplicative_opacity() {
    let mut value = drawing(2, 8, true);
    value["layers"][0]["parentLayerUid"] = json!("L1");
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":4});
    value["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},
         "properties":[{"property":"opacity","static":[50]}]},
        {"target":{"contextId":"scene-root","layerUid":"L1"},
         "properties":[{"property":"position","static":[10,-4]},
                       {"property":"opacity","static":[50]}]}
    ]);
    let snapshot = snapshot(&value);
    let bytes = encode_storage_project(snapshot.document()).unwrap();
    for (frame, expected) in [
        (0, vec![("s1", 0.5)]),
        (3, vec![("s0", 0.25), ("s1", 0.5)]),
        (5, vec![("s1", 0.5)]),
    ] {
        let pins = selector(&snapshot, frame);
        let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
        assert_eq!(packet.source_frame_for_layer(0), (frame == 3).then_some(0));
        assert_eq!(packet.source_frame_for_layer(1), Some(0));
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        let paint = prepare_object_paint_plan(&scene, &pins).unwrap();
        assert_eq!(paint.commands().len(), expected.len());
        for (command, (id, alpha)) in paint.commands().zip(expected) {
            assert_eq!(command.source().record().target().stroke_id(), id);
            assert_eq!(command.contour().segments()[0].point, [11., 16.]);
            assert_eq!(command.contour().segments()[0].handle_in, [-2., 0.]);
            assert_eq!(command.contour().segments()[0].handle_out, [3., -1.]);
            assert_eq!(command.paint()[3].as_f64(), Some(alpha));
            assert_eq!(
                command.source().record().geometry.segments[0]
                    .point
                    .x
                    .as_f64(),
                Some(1.)
            );
        }
        assert_eq!(paint.packet().frame(), frame);
        assert_eq!(
            paint.packet().document_snapshot_id(),
            snapshot.snapshot_id()
        );
    }
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), bytes);
}

#[test]
fn original_pin_and_frame_guards_precede_range_evaluation() {
    let snapshot = snapshot(&drawing(1, 8, false));
    let mut wrong = selector(&snapshot, 0);
    wrong.document_snapshot_id = "stale";
    assert_eq!(
        prepare_object_exposure_frame(&snapshot, &wrong).err(),
        Some(ObjectFrameError::WrongSnapshot)
    );
    assert_eq!(
        prepare_object_exposure_frame(&snapshot, &selector(&snapshot, 8)).err(),
        Some(ObjectFrameError::FrameOutOfRange)
    );
}

#[test]
fn visibility_and_unconsumed_motion_keep_their_original_publication_guards() {
    let mut value = drawing(2, 8, true);
    value["layers"][0]["visible"] = json!(false);
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":4});
    let hidden = snapshot(&value);
    let packet = prepare_object_exposure_frame(&hidden, &selector(&hidden, 3)).unwrap();
    let ids: Vec<_> = packet
        .records()
        .map(|entry| entry.record().target().stroke_id().to_owned())
        .collect();
    assert_eq!(ids, vec!["s1"]);
    value["layers"][0]["visible"] = json!(true);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"rotation","static":[30]}]}]);
    let rotation = snapshot(&value);
    let bytes = encode_storage_project(rotation.document()).unwrap();
    let pins = selector(&rotation, 3);
    let packet = prepare_object_exposure_frame(&rotation, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let paint = prepare_object_paint_plan(&scene, &pins).unwrap();
    assert_eq!(paint.commands().len(), 2);
    let unchanged = paint.commands().nth(1).unwrap();
    assert_eq!(unchanged.source().record().target().stroke_id(), "s1");
    assert_eq!(unchanged.contour().segments()[0].point, [1., 20.]);
    assert!(paint
        .commands()
        .all(|command| command.paint()[3].as_f64() == Some(1.)));
    assert_eq!(encode_storage_project(rotation.document()).unwrap(), bytes);
    // A dormant3D-only field has no2D consumer even when its drawing is outside the range.
    value["propertyHolders"][0]["properties"][0] = json!({"property":"positionZ","static":[30]});
    let unsupported = snapshot(&value);
    let bytes = encode_storage_project(unsupported.document()).unwrap();
    assert_eq!(
        prepare_object_exposure_frame(&unsupported, &selector(&unsupported, 0)).err(),
        Some(ObjectFrameError::Unavailable),
        "out-of-range selection does not conceal an unconsumed authored property"
    );
    assert_eq!(
        encode_storage_project(unsupported.document()).unwrap(),
        bytes
    );
}

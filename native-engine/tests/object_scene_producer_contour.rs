//! Real producer-to-consumer tests for the contour scene producer (L07-F; F16, D32, G03a).
//!
//! The producer output is built from a real `ObjectFramePacket`, joined by the L07-C
//! pin registry and read back through the real `ObjectPaintPlan`; no mock stands in for
//! the packet, the scene or the plan.

use super::rows::{
    cases, packet, paint_scene, plan_labels, reference_scene, selector, snapshot, ORACLE,
};
use super::{produce, PRODUCER};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerEntry, ScenePin,
};
use crate::object_codec::encode_storage_project;
use crate::object_render_scene::{
    frame_cases, join_cases, order_cases, paint_test_document, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectSceneError,
};
use serde_json::{json, Value};

#[test]
fn oracle_header_and_counts_are_pinned() {
    for pinned in [
        "baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc",
        "fee314b515913202c05babfa3425769f57d4a95e7b5dcd5b88ac9233704a9a5c",
        "4a756742c0e00f4bf4410c548b889aa95669ba2cc62bd7c1e929ba45c391f53b",
        "5da829bafb0d9d6f9a99703c74fea2d3d68ddd2d2c4b463a0b19d7324b717f96",
        r#"{"P":120,"naturalDiffers":68,"compactDiffers":4,"storageDiffers":87,"hiddenCases":67,"emptyScenes":13,"shapes":482}"#,
    ] {
        assert!(ORACLE.contains(pinned), "{pinned}");
    }
    let all = cases();
    assert_eq!(all.len(), 120);
    assert_eq!(all.iter().filter(|c| c.expected.is_empty()).count(), 13);
    assert_eq!(
        all.iter()
            .filter(|c| c.layers.iter().any(|l| l.hidden))
            .count(),
        67
    );
    let visible: usize = all
        .iter()
        .map(|c| {
            c.layers
                .iter()
                .filter(|l| !l.hidden)
                .map(|l| l.ords.len())
                .sum::<usize>()
        })
        .sum();
    assert_eq!(visible, 482);
    assert_eq!(all.iter().map(|c| c.expected.len()).sum::<usize>(), 482);
}

#[test]
fn paint_plan_order_matches_the_baseline_scene_order_for_every_oracle_row() {
    for case in cases() {
        assert_eq!(plan_labels(&case), case.expected, "{}", case.id);
    }
}

#[test]
fn hidden_layers_keep_their_slot_and_rank_but_paint_nothing() {
    let all = cases();
    let find = |id: &str| all.iter().find(|c| c.id == id).expect("hand row");
    // A hidden layer at index 1 shifts where explicit rank 2 lands (baseline `{ items: [] }`).
    let shifted = find("hidden-slot-shifts-explicit-rank");
    assert_eq!(shifted.expected, ["0.0", "2.0"]);
    assert_eq!(plan_labels(shifted), ["0.0", "2.0"]);
    assert_eq!(shifted.compact, ["2.0", "0.0"]);
    assert_eq!(find("all-hidden").expected, Vec::<String>::new());
    assert!(plan_labels(find("all-hidden")).is_empty());
    assert_eq!(
        plan_labels(find("hidden-front-keeps-its-slot")),
        ["0.0", "1.0"]
    );
}

#[test]
fn the_producer_output_carries_the_packet_pin_and_one_item_per_selected_record() {
    let all = cases();
    let case = all
        .iter()
        .find(|c| c.id == "layer-and-elements")
        .expect("hand row");
    let snap = snapshot(&super::rows::document(case), "contour-pin");
    let pk = packet(&snap);
    let production = produce(&pk);
    assert_eq!(production.output.producer, PRODUCER);
    assert_eq!(production.output.producer, "contour");
    assert_eq!(production.output.pin, ScenePin::of_packet(&pk));
    assert!(production.output.resources.is_empty());
    assert!(production.rejections.is_empty());
    let indices: Vec<usize> = pk.records().map(|r| r.source_object_index()).collect();
    assert_eq!(indices.len(), 6);
    let claimed: Vec<usize> = production
        .output
        .entries
        .iter()
        .map(|entry| match entry {
            ProducerEntry::Item {
                source_object_index,
                ..
            } => *source_object_index,
            ProducerEntry::Unavailable { .. } => panic!("no record may be unavailable"),
        })
        .collect();
    assert_eq!(claimed, indices);
    // Every payload agrees with its immutable source record.
    for (entry, record) in production.output.entries.iter().zip(pk.records()) {
        let ProducerEntry::Item { payload, .. } = entry else {
            unreachable!("checked above")
        };
        let fill = record.record().solid_fill().unwrap();
        assert_eq!(
            json!(payload.solid.channels()),
            json!([fill.r, fill.g, fill.b, fill.a])
        );
        assert_eq!(payload.envelope, [0.0, 0.0, 10.0, 10.0]);
    }
    let admitted = production.into_admitted().expect("all drawable");
    assert_eq!(admitted.len(), 6);
}

#[test]
fn typed_paint_equals_the_source_fill_and_the_straight_f32_color_for_every_command() {
    let value = paint_test_document(&[7]);
    let sc = paint_scene(&value, 7);
    let plan = prepare_object_paint_plan(&sc, &selector(sc.packet())).unwrap();
    assert_eq!(plan.commands().len(), 5);
    for command in plan.commands() {
        let literal = json!([
            0.20000000000000004,
            0.4000000000000001,
            0.6000000000000001,
            0.7500000000000001
        ]);
        assert_eq!(json!(command.paint()), literal);
        assert_eq!(json!(command.solid().channels()), literal);
        // paint() still borrows the immutable source; the typed copy equals it by value.
        assert!(std::ptr::eq(
            command.paint()[3],
            &command.source().record().solid_fill().unwrap().a
        ));
        assert_eq!(
            command.solid().rgba_f32(),
            Ok([0.2f32, 0.4f32, 0.6f32, 0.75f32])
        );
        // The command and the retained scene entry share one typed paint.
        let entry = sc
            .entries()
            .find(|e| e.source().source_object_index() == command.source().source_object_index())
            .unwrap();
        assert!(std::ptr::eq(command.solid(), entry.solid()));
    }
}

#[test]
fn existing_literal_order_frame_and_join_cases_pass_through_the_pin_registry() {
    let reference = reference_scene();
    order_cases(paint_scene);
    frame_cases(paint_scene);
    join_cases(paint_scene, &reference);
}

/// Replace the segments of every object of `layer` by `segments` (admitted by the codec,
/// refused by the contour compiler).
fn corrupt(value: &mut Value, layer: &str, points: &[[f64; 2]]) {
    let segments: Vec<Value> = points
        .iter()
        .map(|[x, y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}))
        .collect();
    for object in value["objects"].as_array_mut().unwrap() {
        if object["target"]["layerUid"] == layer {
            object["geometry"]["segments"] = json!(segments);
        }
    }
}

#[test]
fn unavailable_geometry_is_a_typed_entry_and_the_scene_still_refuses_atomically() {
    let mut value = paint_test_document(&[7]);
    corrupt(&mut value, "L1", &[[0., 0.], [1., 1.], [2., 2.]]);
    corrupt(&mut value, "L2", &[[0., 0.], [0., 0.]]);
    let snap = snapshot(&value, "contour-unavailable");
    let before = encode_storage_project(snap.document()).unwrap();
    let pk = packet(&snap);
    assert_eq!(pk.records().len(), 5);
    let production = produce(&pk);
    // Every record is still reported: three items and two typed unavailable entries.
    assert_eq!(production.output.entries.len(), 5);
    let mut unavailable = Vec::new();
    for entry in &production.output.entries {
        if let ProducerEntry::Unavailable {
            source_object_index,
            reason,
        } = entry
        {
            unavailable.push((*source_object_index, reason.clone()));
        }
    }
    assert_eq!(unavailable.len(), 2);
    let index_of = |layer: &str| {
        pk.records()
            .find(|r| r.record().target().layer_uid() == layer)
            .unwrap()
            .source_object_index()
    };
    let (l1, l2) = (index_of("L1"), index_of("L2"));
    assert!(unavailable.contains(&(l1, "cubic control hull must have positive area".to_owned())));
    assert!(unavailable.contains(&(
        l2,
        "cubic control bounds must have finite positive extent".to_owned()
    )));
    // The refusal names the FIRST failing record in provenance order, like the old collect.
    let (first, reason) = if l1 < l2 {
        (l1, "cubic control hull must have positive area")
    } else {
        (l2, "cubic control bounds must have finite positive extent")
    };
    let expected = ObjectSceneError::UnsupportedGeometry {
        source_object_index: first,
        reason,
    };
    assert_eq!(production.into_admitted().unwrap_err(), expected);
    assert_eq!(
        prepare_object_render_scene(&pk, &selector(&pk)).unwrap_err(),
        expected
    );
    // The refusal publishes nothing and leaves the immutable source bytes untouched.
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
}

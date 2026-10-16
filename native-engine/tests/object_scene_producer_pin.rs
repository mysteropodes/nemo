//! Behaviour tests for the pinned multi-producer scene join (L07-C; D32, G03a).
//!
//! The 300 baseline-oracle rows and the builders that turn each into a real v2
//! document, packet and three producers live in `rows`; see its header.

use super::rows::{
    case_of, cases, divergences, document, gid_of, join_case, labels, outputs, packet, record,
    snapshot, FRAME, ORACLE,
};
use super::{
    join_scene, ProducerEntry, ProducerOutput, SceneJoinError, SceneOutcome, ScenePin,
    UnavailableCause,
};
use crate::object_document::FrameScopeKind;
use serde_json::json;

#[test]
fn oracle_header_and_counts_are_pinned() {
    let all = cases();
    assert_eq!(all.len(), 300);
    assert_eq!(all[0].id, "natural");
    assert_eq!(all[15].id, "layer-and-elements");
    assert_eq!(all[16].id, "rand-0");
    let shapes: usize = all
        .iter()
        .map(|case| case.layers.iter().map(|l| l.ords.len()).sum::<usize>())
        .sum();
    assert_eq!(shapes, 1810);
    assert!(all
        .iter()
        .all(|c| c.expected.len() == c.layers.iter().map(|l| l.ords.len()).sum::<usize>()));
    assert!(all
        .iter()
        .any(|c| c.layers.iter().any(|l| l.rank.is_some())));
    assert!(ORACLE.contains("(baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc)"));
    assert!(ORACLE.contains("fee314b515913202c05babfa3425769f57d4a95e7b5dcd5b88ac9233704a9a5c"));
    assert!(ORACLE.contains("4a756742c0e00f4bf4410c548b889aa95669ba2cc62bd7c1e929ba45c391f53b"));
}

#[test]
fn joined_order_matches_the_baseline_scene_order_for_every_oracle_row() {
    let all = cases();
    assert_eq!(divergences(&all), Vec::<String>::new());
}

#[test]
fn every_item_payload_and_layer_agree_with_its_immutable_source_record() {
    for case in cases() {
        let snap = snapshot(&document(&case, &[]), "scene-pin");
        let before = snap.document().clone();
        let pk = packet(&snap);
        let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outputs(&pk)).unwrap();
        assert_eq!(scene.pin(), &ScenePin::of_packet(&pk), "{}", case.id);
        assert_eq!(scene.entries().len(), pk.records().len(), "{}", case.id);
        for entry in scene.entries() {
            let record = &snap.document().objects()[entry.source_object_index()];
            let SceneOutcome::Item { payload, .. } = entry.outcome() else {
                panic!("{}: unexpected unavailable entry", case.id);
            };
            assert_eq!(payload.0, record.target().layer_uid());
            assert_eq!(payload.1, record.target().stroke_id());
            assert_eq!(
                pk.source_layers()[entry.source_layer_index()].layer_uid(),
                record.target().layer_uid()
            );
            // Independent expectation: gid j-major, r = (gid + 1) / 100.
            let j: usize = payload.0[1..].parse().unwrap();
            let k = usize::from(payload.1.as_bytes()[0] - b'a');
            let want = (gid_of(&case, j, k) as f64 + 1.0) / 100.0;
            assert_eq!(payload.2, want.to_bits(), "{} {j}.{k}", case.id);
        }
        assert_eq!(
            snap.document(),
            &before,
            "the join never edits the document"
        );
    }
}

#[test]
fn baseline_json_layer_cases_and_producer_interleaving_are_pinned_literally() {
    // engineering/application/examples/native-object-order-v2/baseline.json layerCases.
    let natural = case_of(vec![
        (None, vec![None]),
        (None, vec![None]),
        (None, vec![None]),
    ]);
    assert_eq!(join_case(&natural), ["0.0", "1.0", "2.0"]);
    let collision = case_of(vec![
        (Some(1.0), vec![None]),
        (None, vec![None]),
        (Some(1.0), vec![None]),
    ]);
    assert_eq!(join_case(&collision), ["1.0", "0.0", "2.0"]);
    let displacement = case_of(vec![
        (None, vec![None]),
        (None, vec![None]),
        (Some(2.0), vec![None]),
    ]);
    assert_eq!(join_case(&displacement), ["0.0", "2.0", "1.0"]);

    let snap = snapshot(&document(&displacement, &[]), "scene-pin");
    let pk = packet(&snap);
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outputs(&pk)).unwrap();
    let producers: Vec<&str> = scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item { producer, .. } => producer.as_str(),
            SceneOutcome::Unavailable(_) => "unavailable",
        })
        .collect();
    // Plan order L0, L2, L1 interleaves contour, text, component.
    assert_eq!(producers, ["contour", "text", "component"]);
    // Document layers are stored [L2, L1, L0] and objects reversed: array positions
    // are provenance only.
    let layer_indices: Vec<usize> = scene
        .entries()
        .iter()
        .map(|e| e.source_layer_index())
        .collect();
    let object_indices: Vec<usize> = scene
        .entries()
        .iter()
        .map(|e| e.source_object_index())
        .collect();
    assert_eq!(layer_indices, [2, 0, 1]);
    assert_eq!(object_indices, [2, 0, 1]);
}

#[test]
fn the_pin_is_the_packet_pin_and_resources_feed_the_evaluation_key() {
    let case = case_of(vec![(None, vec![None, Some(1.0)]), (Some(1.0), vec![None])]);
    let snap = snapshot(&document(&case, &[]), "scene-pin");
    let pk = packet(&snap);
    let pin = ScenePin::of_packet(&pk);
    assert_eq!(pin.instance_id, pk.instance_id());
    assert_eq!(pin.instance_id, "scene-pin");
    assert_eq!(pin.document_id, pk.document_id());
    assert_eq!(pin.content_revision, pk.content_revision());
    assert_eq!(pin.document_snapshot_id, pk.document_snapshot_id());
    assert_eq!(pin.context_id, "scene-root");
    assert_eq!(pin.scope_kind, FrameScopeKind::Authored);
    assert_eq!(pin.frame, FRAME);
    let scene = join_scene(&pk, &pin, outputs(&pk)).unwrap();
    let merged: Vec<(&str, &str)> = scene
        .declared_resource_versions()
        .iter()
        .map(|(id, version)| (id.as_str(), version.as_str()))
        .collect();
    assert_eq!(
        merged,
        [
            ("res:component", "v1"),
            ("res:contour", "v1"),
            ("res:text", "v1"),
            ("shared", "7"),
        ]
    );
    // EvaluationKey::new takes `impl IntoIterator<Item = (String, String)>`; this is the
    // same bound (scheduler.rs:105-113), so the merged map is accepted by value.
    fn key_input(pairs: impl IntoIterator<Item = (String, String)>) -> usize {
        pairs.into_iter().count()
    }
    assert_eq!(key_input(scene.declared_resource_versions().clone()), 4);
}

#[test]
fn an_unclaimed_record_stays_in_its_plan_position_as_a_typed_unavailable_entry() {
    let case = case_of(vec![
        (None, vec![None]),
        (None, vec![None]),
        (None, vec![None]),
    ]);
    let snap = snapshot(&document(&case, &[]), "scene-pin");
    let pk = packet(&snap);
    let mut outs = outputs(&pk);
    for out in outs.iter_mut() {
        out.entries
            .retain(|e| !matches!(e, ProducerEntry::Item { payload, .. } if payload.0 == "L1"));
    }
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outs).unwrap();
    // L1.a is object index 1 (objects are stored reversed).
    assert_eq!(labels(&scene), ["0.0", "unavailable:1", "2.0"]);
    assert!(!scene.is_complete());
    let missing: Vec<_> = scene.unavailable().collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].source_object_index(), 1);
    assert_eq!(
        missing[0].outcome(),
        &SceneOutcome::Unavailable(UnavailableCause::Unclaimed)
    );
}

#[test]
fn a_producer_reported_unavailable_keeps_producer_index_and_reason() {
    let case = case_of(vec![
        (None, vec![None]),
        (None, vec![None]),
        (None, vec![None]),
    ]);
    let snap = snapshot(&document(&case, &[]), "scene-pin");
    let pk = packet(&snap);
    let mut outs = outputs(&pk);
    for out in outs.iter_mut() {
        out.entries
            .retain(|e| !matches!(e, ProducerEntry::Item { payload, .. } if payload.0 == "L1"));
    }
    assert_eq!(outs[0].producer, "text");
    outs[0].entries.push(ProducerEntry::Unavailable {
        source_object_index: 1,
        reason: "glyph layout not retained".to_owned(),
    });
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outs).unwrap();
    assert_eq!(labels(&scene), ["0.0", "unavailable:1", "2.0"]);
    let entry = &scene.entries()[1];
    assert_eq!(entry.source_object_index(), 1);
    assert_eq!(
        entry.outcome(),
        &SceneOutcome::Unavailable(UnavailableCause::Reported {
            producer: "text".to_owned(),
            reason: "glyph layout not retained".to_owned(),
        })
    );
}

#[test]
fn invisible_layers_are_the_only_intentional_omission() {
    let case = case_of(vec![
        (None, vec![None, None]),
        (None, vec![None]),
        (None, vec![None]),
    ]);
    let snap = snapshot(&document(&case, &[1]), "scene-pin");
    let pk = packet(&snap);
    assert_eq!(
        pk.records().len(),
        3,
        "the packet itself omits the hidden layer"
    );
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outputs(&pk)).unwrap();
    assert_eq!(labels(&scene), ["0.0", "0.1", "2.0"]);
    assert!(scene.is_complete());
}

#[test]
fn a_packet_without_a_numeric_order_plan_is_refused_not_ordered() {
    let value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"L0"}],"objects":[record("L0", "a", 0)]});
    let snap = snapshot(&value, "scene-pin");
    let pk = packet(&snap);
    assert!(pk.numeric_order().is_none());
    let result = join_scene(
        &pk,
        &ScenePin::of_packet(&pk),
        Vec::<ProducerOutput<u8>>::new(),
    );
    assert_eq!(result.unwrap_err(), SceneJoinError::NoNumericOrder);
}

#[test]
fn payloads_move_through_the_join_without_clone_or_equality() {
    struct Opaque(u32);
    let case = case_of(vec![(None, vec![None]), (Some(1.0), vec![None])]);
    let snap = snapshot(&document(&case, &[]), "scene-pin");
    let pk = packet(&snap);
    let pin = ScenePin::of_packet(&pk);
    // Object indices: L1.a = 0, L0.a = 1 (objects are stored reversed).
    let out = ProducerOutput::new("probe", pin.clone())
        .with_item(1, Opaque(10))
        .with_item(0, Opaque(20));
    let scene = join_scene(&pk, &pin, vec![out]).unwrap();
    let values: Vec<u32> = scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item { payload, .. } => payload.0,
            SceneOutcome::Unavailable(_) => u32::MAX,
        })
        .collect();
    // Rank 1 puts L1 frontmost, so back to front is L0 (10) then L1 (20).
    assert_eq!(values, [10, 20]);
}

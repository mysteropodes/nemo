//! L10-H atomicity: a failure at item i of N, a stale / foreign / forged candidate and a cancelled
//! preparation each leave revision, serialized bytes, history depth and the redo stack untouched.
//! Every case runs on a history with a LIVE redo entry (a real edit then undo), so "redo survives"
//! cannot pass by clearing an already empty stack.
use super::support::*;
use super::{ImportBatch, ImportError, ImportRecord};
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;

const N: usize = 5;

/// One new layer `g` and five records alternating between it and the existing layer `L1`.
fn batch_of_five() -> ImportBatch {
    ImportBatch {
        layers: vec![layer("g", "G")],
        records: (0..N)
            .map(|i| {
                let target = if i % 2 == 0 { "g" } else { "L1" };
                item(&format!("r:{i}"), target, 0, i as i64)
            })
            .collect(),
        ..ImportBatch::default()
    }
}

type Mutate = fn(&mut ImportRecord);
type Expect = fn(&ImportError, usize) -> bool;

const CORRUPTIONS: [(&str, Mutate, Expect); 7] = [
    (
        "not-a-vector-record",
        |r| r.record.family = "closed-solid-fill-cubic-path".into(),
        |e, i| *e == ImportError::NotVector { index: i },
    ),
    (
        "stroke-id-chosen-by-the-caller",
        |r| r.record.target.stroke_id = "mine".into(),
        |e, i| *e == ImportError::StrokeIdPreset { index: i },
    ),
    (
        "reference-scope",
        |r| r.record.target.frame_scope.kind = FrameScopeKind::Reference,
        |e, i| *e == ImportError::ReferenceScope { index: i },
    ),
    (
        "unknown-layer",
        |r| r.record.target.layer_uid = "nowhere".into(),
        |e, i| {
            *e == ImportError::UnknownLayer {
                index: i,
                reference: "nowhere".into(),
            }
        },
    ),
    (
        "frame-outside-the-document",
        |r| r.record.target.frame_scope.frame = 24,
        |e, i| *e == ImportError::InvalidTarget { index: i },
    ),
    (
        "foreign-context",
        |r| r.record.target.context_id = "other".into(),
        // v1 documents only have the scene-root context; a v2 plan does not declare "other".
        |e, i| {
            *e == ImportError::InvalidTarget { index: i }
                || *e == ImportError::ContextAbsent { index: i }
        },
    ),
    (
        "one-anchor-fails-storage-admission",
        |r| r.record.geometry.segments.truncate(1),
        |e, _| *e == ImportError::Dispatch(DispatchErrorCode::InvalidRequest),
    ),
];

#[test]
fn the_uncorrupted_batch_of_five_prepares_and_commits() {
    for version in [1, 2] {
        let mut h = history(version);
        let prepared = h.prepare_import_batch(&batch_of_five()).unwrap();
        assert_eq!(prepared.result().objects.len(), N);
        let (revision, _) = h.commit_import_batch(prepared).unwrap();
        assert_eq!(revision, 1);
    }
}

#[test]
fn failure_at_item_i_of_n_leaves_revision_bytes_history_and_redo_unchanged() {
    for version in [1, 2] {
        for (name, mutate, expect) in CORRUPTIONS {
            for i in 0..N {
                let mut h = history(version);
                prime_redo(&mut h);
                let before = frozen(&h);
                let mut batch = batch_of_five();
                mutate(&mut batch.records[i]);
                let error = h
                    .prepare_import_batch(&batch)
                    .expect_err(&format!("{name} at {i} must be refused"));
                assert!(expect(&error, i), "{name} at {i}: {error:?}");
                assert_eq!(frozen(&h), before, "{name} at {i}: nothing may publish");
                assert_eq!(h.history_depths(), (0, 1), "{name} at {i}: redo survives");
            }
        }
    }
}

#[test]
fn a_repeated_source_id_is_refused_whole() {
    let mut h = history(1);
    prime_redo(&mut h);
    let before = frozen(&h);
    let mut batch = batch_of_five();
    batch.records[3].source_id = "r:1".into();
    let error = h.prepare_import_batch(&batch).unwrap_err();
    assert!(matches!(error, ImportError::Remap(_)), "{error:?}");
    assert_eq!(error.code(), DispatchErrorCode::InvalidRequest);
    assert_eq!(frozen(&h), before);
}

#[test]
fn a_stale_candidate_is_refused_after_another_writer_or_an_undo() {
    let mut h = history(1);
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    let hide = json!({"command":"object.layer.visibility.set","layerUid":"L1","visible":false});
    let other = h
        .prepare_layer_visibility_json(hide.to_string().as_bytes())
        .unwrap();
    h.commit_layer_visibility(other).unwrap();
    let after_other = frozen(&h);
    assert_eq!(
        h.commit_import_batch(prepared).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        frozen(&h),
        after_other,
        "the refused commit changed nothing"
    );
    // The same holds when the head only moved because of undo/redo.
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.undo(1).unwrap();
    let after_undo = frozen(&h);
    assert_eq!(
        h.commit_import_batch(prepared).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(frozen(&h), after_undo);
    // A refused commit consumed no id: a fresh preparation uses the new head's ids.
    let fresh = h.prepare_import_batch(&mixed_batch()).unwrap();
    assert_eq!(fresh.result().layers[0].1, "native-layer:3:0");
}

#[test]
fn a_candidate_prepared_by_another_owner_is_refused_with_the_matching_pin() {
    let a = history_of(&base_doc(1), "same-instance");
    let mut same_instance = history_of(&base_doc(1), "same-instance");
    let mut other_instance = history_of(&base_doc(1), "other-instance");
    let before = (frozen(&same_instance), frozen(&other_instance));
    let prepared = a.prepare_import_batch(&mixed_batch()).unwrap();
    assert_eq!(
        same_instance.commit_import_batch(prepared).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let prepared = a.prepare_import_batch(&mixed_batch()).unwrap();
    assert_eq!(
        other_instance.commit_import_batch(prepared).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!((frozen(&same_instance), frozen(&other_instance)), before);
    assert_eq!(
        frozen(&a).revision,
        0,
        "the preparing owner never published"
    );
}

#[test]
fn the_owner_seal_refuses_a_foreign_origin_and_a_forged_after_document() {
    let mut h = history(1);
    let other = history_of(&base_doc(1), "forge");
    let foreign = other.acquire_snapshot(0).unwrap();
    assert_eq!(
        h.owner.seal_import_batch(foreign, None).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    let head = h.acquire_snapshot(0).unwrap();
    let mut forged = head.document().clone();
    let duplicate = forged.objects[0].clone();
    forged.objects.push(duplicate);
    assert_eq!(
        h.owner.seal_import_batch(head, Some(forged)).unwrap_err(),
        DispatchErrorCode::InvalidRequest,
        "the owner runs storage admission on the whole after-document itself"
    );
    // A valid document that is not the origin plus appended content (here: the origin's record
    // dropped, its layer order swapped, a frame count changed) is refused by the derivation guard.
    let mut unrelated = h.acquire_snapshot(0).unwrap().document().clone();
    unrelated.objects.clear();
    let mut reordered = h.acquire_snapshot(0).unwrap().document().clone();
    reordered.layers.swap(0, 1);
    let mut resized = h.acquire_snapshot(0).unwrap().document().clone();
    resized.total_frames += 1;
    for candidate in [unrelated, reordered, resized] {
        let head = h.acquire_snapshot(0).unwrap();
        assert_eq!(
            h.owner
                .seal_import_batch(head, Some(candidate))
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "an after-document must be the origin plus appended layers and records"
        );
    }
    let stale = h.acquire_snapshot(0).unwrap();
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.commit_import_batch(prepared).unwrap();
    assert_eq!(
        h.owner.seal_import_batch(stale, None).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
}

#[test]
fn a_cancelled_preparation_publishes_nothing_and_frees_its_ids() {
    let h: NativeObjectHistory = history(2);
    let before = frozen(&h);
    let first = h.prepare_import_batch(&mixed_batch()).unwrap();
    let ids = first.result().clone();
    drop(first);
    assert_eq!(frozen(&h), before);
    let second = h.prepare_import_batch(&mixed_batch()).unwrap();
    assert_eq!(
        *second.result(),
        ids,
        "the ids of the dropped candidate are still free"
    );
}

//! L10-H flows through the REAL owner and history: one publish, one history entry, literal native
//! ids and documents, undo/redo/reopen, the v2 composition-order plan, no-op and the accounting of
//! every input element. Expected documents are hand-derived literals, not re-computed by the
//! planner under test.
use super::support::*;
use super::{ImportBatch, ImportError, ImportSkip, MissingProvider, SkipKind, UnavailableElement};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn timing(frames: &[u32]) -> Value {
    let keys: Vec<Value> = frames.iter().map(|f| json!({"frame": f})).collect();
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys,"samples":[]})
}
fn new_layer(uid: &str, name: &str, color: &str, frames: &[u32]) -> Value {
    json!({"layerUid":uid,"name":name,"visible":true,"locked":false,"color":color,
        "parentLayerUid":null,"timing":timing(frames)})
}

/// Hand-derived result of `mixed_batch` on the base document at revision 0 (next revision 1).
/// Layer ids: the base holds 2 layers, so the first free suffixes are :0 and :1; the palette index
/// is the layer count before insertion (2, then 3). Stroke ids: :0..:3 in record order.
fn expected_mixed(mut doc: Value, version: u32) -> Value {
    let layers = doc["layers"].as_array_mut().unwrap();
    layers.push(new_layer("native-layer:1:0", "Frame A", "#F0A585", &[0, 3]));
    layers.push(new_layer("native-layer:1:1", "Frame B", "#E88BAE", &[0]));
    let objects = doc["objects"].as_array_mut().unwrap();
    objects.push(vector_json("native-layer:1:0", 0, "native-stroke:1:0", 1));
    objects.push(vector_json("native-layer:1:0", 3, "native-stroke:1:1", 2));
    objects.push(vector_json("native-layer:1:1", 0, "native-stroke:1:2", 3));
    objects.push(vector_json("L0", 0, "native-stroke:1:3", 9));
    if version == 2 {
        let element = |layer: &str, stroke: &str, position: u32| json!({"layerUid":layer,"strokeId":stroke,"naturalPosition":position,"order":{}});
        let add = |layer: &str, position: u32| json!({"layerUid":layer,"naturalPosition":position,"order":{}});
        let contexts = doc["compositionOrder"]["contexts"].as_array_mut().unwrap();
        for context in contexts.iter_mut() {
            let list = context["layers"].as_array_mut().unwrap();
            list.push(add("native-layer:1:0", 2));
            list.push(add("native-layer:1:1", 3));
        }
        let scopes = contexts[0]["scopes"].as_array_mut().unwrap();
        let first = scopes[0]["elements"].as_array_mut().unwrap();
        first.push(element("native-layer:1:0", "native-stroke:1:0", 0));
        first.push(element("native-layer:1:1", "native-stroke:1:2", 0));
        first.push(element("L0", "native-stroke:1:3", 1));
        scopes.push(json!({"frameScope":{"kind":"authored","frame":3},
            "elements":[element("native-layer:1:0", "native-stroke:1:1", 0)]}));
    }
    doc
}

fn receipt_literal() -> Value {
    json!({"applied":true,"historyEntriesAdded":1,
        "layers":[{"sourceId":"fig:a","layerUid":"native-layer:1:0"},
            {"sourceId":"fig:b","layerUid":"native-layer:1:1"}],
        "objects":[{"sourceId":"fig:a:0","strokeId":"native-stroke:1:0"},
            {"sourceId":"fig:a:1","strokeId":"native-stroke:1:1"},
            {"sourceId":"fig:b:0","strokeId":"native-stroke:1:2"},
            {"sourceId":"e:0","strokeId":"native-stroke:1:3"}],
        "skipped":[]})
}

#[test]
fn mixed_batch_publishes_once_with_one_history_entry_and_literal_ids_in_v1_and_v2() {
    for version in [1, 2] {
        let mut h = history(version);
        let before = state(&h);
        let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
        assert!(prepared.changed());
        assert_eq!(state(&h), before, "preparation publishes nothing");
        assert_eq!((h.content_revision(), h.history_depths()), (0, (0, 0)));
        let (revision, receipt) = h.commit_import_batch(prepared).unwrap();
        assert_eq!(revision, 1, "one owner publish for the whole batch");
        assert_eq!(h.history_depths(), (1, 0), "one history entry");
        assert_eq!(state(&h), expected_mixed(before, version));
        assert_eq!(receipt.result(), receipt_literal());
    }
}

#[test]
fn undo_and_redo_move_the_whole_batch_as_one_entry_and_reopen_equals_the_saved_set() {
    let mut h = history(2);
    let original = state(&h);
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.commit_import_batch(prepared).unwrap();
    let imported = state(&h);
    assert_eq!(h.undo(1), Ok(2));
    assert_eq!(state(&h), original, "undo restores every layer and record");
    assert_eq!(h.history_depths(), (0, 1));
    assert_eq!(h.redo(2), Ok(3));
    assert_eq!(state(&h), imported, "redo reapplies every layer and record");
    assert_eq!(h.history_depths(), (1, 0));
    let reopened = NativeObjectHistory::new(
        "reopen",
        decode_storage_project(&bytes(&h)).expect("saved bytes decode"),
    )
    .unwrap();
    assert_eq!(state(&reopened), imported);
    let uids = |doc: &Value| -> Vec<String> {
        doc["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["layerUid"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(uids(&state(&reopened)), uids(&imported));
    assert_eq!(
        uids(&imported),
        ["L0", "L1", "native-layer:1:0", "native-layer:1:1"]
    );
}

#[test]
fn ids_are_a_pure_function_of_the_origin_and_stay_monotonic_across_undo_and_redo() {
    let mut h = history(1);
    let first = h.prepare_import_batch(&mixed_batch()).unwrap();
    let second = h.prepare_import_batch(&mixed_batch()).unwrap();
    assert_eq!(
        first.result(),
        second.result(),
        "preparing twice consumes nothing"
    );
    drop(second);
    h.commit_import_batch(first).unwrap();
    h.undo(1).unwrap();
    h.redo(2).unwrap();
    assert_eq!(h.content_revision(), 3);
    let later = h.prepare_import_batch(&mixed_batch()).unwrap();
    let ids: Vec<&str> = later.result().layers.iter().map(|l| l.1.as_str()).collect();
    assert_eq!(ids, ["native-layer:4:0", "native-layer:4:1"]);
    assert_eq!(later.result().objects[0].1, "native-stroke:4:0");
}

#[test]
fn a_batch_that_creates_nothing_preserves_presence_bytes_history_and_redo() {
    let mut h = history(2);
    prime_redo(&mut h);
    let before = frozen(&h);
    for skipped in [
        vec![],
        vec![ImportSkip {
            kind: SkipKind::Element,
            source_id: "svg:3".into(),
            reason: "unsupported-element:Raster".into(),
        }],
    ] {
        let batch = ImportBatch {
            skipped: skipped.clone(),
            ..ImportBatch::default()
        };
        let prepared = h.prepare_import_batch(&batch).unwrap();
        assert!(!prepared.changed());
        let (revision, receipt) = h.commit_import_batch(prepared).unwrap();
        assert_eq!(revision, before.revision, "no revision is consumed");
        assert_eq!(
            frozen(&h),
            before,
            "bytes, revision and both stacks survive"
        );
        assert!(!receipt.applied);
        assert_eq!(
            receipt.skipped.len(),
            skipped.len(),
            "the skip report is still delivered"
        );
        assert_eq!(receipt.result()["historyEntriesAdded"], 0);
    }
    assert_eq!(h.history_depths(), (0, 1), "redo survived the no-op");
}

/// Every declared source entry is created or reported skipped, exactly once.
fn assert_accounted(batch: &ImportBatch, h: &super::ImportBatchResult) {
    let mut seen: Vec<&str> = h.layers.iter().map(|e| e.0.as_str()).collect();
    seen.extend(h.objects.iter().map(|e| e.0.as_str()));
    seen.extend(h.skipped.iter().map(|e| e.source_id.as_str()));
    let mut declared: Vec<&str> = batch.layers.iter().map(|e| e.source_id.as_str()).collect();
    declared.extend(batch.records.iter().map(|e| e.source_id.as_str()));
    declared.extend(batch.skipped.iter().map(|e| e.source_id.as_str()));
    seen.sort_unstable();
    declared.sort_unstable();
    assert_eq!(seen, declared);
}

#[test]
fn the_receipt_accounts_for_every_input_element_and_lists_each_skip_with_its_reason() {
    let mut batch = mixed_batch();
    batch.skipped = vec![
        ImportSkip {
            kind: SkipKind::Layer,
            source_id: "fig:hidden".into(),
            reason: "invisible-frame".into(),
        },
        ImportSkip {
            kind: SkipKind::Element,
            source_id: "fig:a:text".into(),
            reason: "unsupported-element:Text".into(),
        },
    ];
    let mut h = history(1);
    let prepared = h.prepare_import_batch(&batch).unwrap();
    assert_accounted(&batch, prepared.result());
    let (_, receipt) = h.commit_import_batch(prepared).unwrap();
    assert_accounted(&batch, &receipt);
    assert_eq!(
        receipt.result()["skipped"],
        json!([
        {"kind":"layer","sourceId":"fig:hidden","reason":"invisible-frame"},
        {"kind":"element","sourceId":"fig:a:text","reason":"unsupported-element:Text"}])
    );
    assert_eq!(
        receipt.layers.len() + receipt.objects.len() + receipt.skipped.len(),
        2 + 4 + 2
    );
}

#[test]
fn records_may_only_reference_a_layer_that_exists_or_is_created_never_a_skipped_one() {
    let h = history(1);
    let skipped_layer = ImportBatch {
        records: vec![item("r:0", "fig:gone", 0, 1)],
        skipped: vec![ImportSkip {
            kind: SkipKind::Layer,
            source_id: "fig:gone".into(),
            reason: "invisible-frame".into(),
        }],
        ..ImportBatch::default()
    };
    let before = frozen(&h);
    assert_eq!(
        h.prepare_import_batch(&skipped_layer).unwrap_err(),
        ImportError::SkippedLayer {
            index: 0,
            reference: "fig:gone".into()
        }
    );
    let unknown = ImportBatch {
        records: vec![item("r:0", "nowhere", 0, 1)],
        ..ImportBatch::default()
    };
    let error = h.prepare_import_batch(&unknown).unwrap_err();
    assert_eq!(
        error,
        ImportError::UnknownLayer {
            index: 0,
            reference: "nowhere".into()
        }
    );
    assert_eq!(error.code(), DispatchErrorCode::NotFound);
    assert_eq!(frozen(&h), before);
}

#[test]
fn an_element_whose_provider_is_absent_refuses_the_whole_batch_and_names_the_provider() {
    for need in [
        MissingProvider::RasterAsset,
        MissingProvider::Text,
        MissingProvider::Font,
    ] {
        let mut batch = mixed_batch();
        batch.unavailable = vec![UnavailableElement {
            source_id: "psd:2".into(),
            need,
        }];
        let h = history(1);
        let before = frozen(&h);
        let error = h.prepare_import_batch(&batch).unwrap_err();
        assert_eq!(
            error,
            ImportError::ProviderUnavailable {
                source_id: "psd:2".into(),
                need
            }
        );
        assert_eq!(error.code(), DispatchErrorCode::Unavailable);
        assert_eq!(
            frozen(&h),
            before,
            "nothing of the four valid records was published"
        );
    }
}

#[test]
fn an_existing_layer_must_already_be_keyed_at_the_frame_and_a_new_layer_is_keyed_for_it() {
    let mut doc = base_doc(1);
    doc["layers"][1] = json!({"layerUid":"L1","timing":timing(&[0, 5])});
    let mut h = history_of(&doc, "keyed");
    let at = |frame: u32| ImportBatch {
        records: vec![item("k:0", "L1", frame, 1)],
        ..ImportBatch::default()
    };
    for held in [3, 6, 23] {
        let before = frozen(&h);
        assert_eq!(
            h.prepare_import_batch(&at(held)).unwrap_err(),
            ImportError::KeyframeRequired {
                layer_uid: "L1".into(),
                frame: held
            },
            "the baseline would copy the held strokes into a new key; that is lane 5's edit"
        );
        assert_eq!(frozen(&h), before);
    }
    for key in [0, 5] {
        let prepared = h.prepare_import_batch(&at(key)).unwrap();
        assert!(prepared.changed(), "frame {key} is a key");
    }
    let new_layer_at_nine = ImportBatch {
        layers: vec![layer("n", "N")],
        records: vec![item("n:0", "n", 9, 1)],
        ..ImportBatch::default()
    };
    let prepared = h.prepare_import_batch(&new_layer_at_nine).unwrap();
    h.commit_import_batch(prepared).unwrap();
    let layers = state(&h)["layers"].clone();
    assert_eq!(
        layers[2]["timing"],
        timing(&[0, 9]),
        "ensureKeyframe on a blank new layer"
    );
}

#[test]
fn a_locked_existing_layer_is_refused_end_to_end_and_a_new_layer_never_is() {
    // F06 landed: a locked layer is admitted in a document and the policy edit locks one through the
    // real owner, so the refusal is proved from a live history with nothing published.
    let batch_into = |layer: &str| ImportBatch {
        layers: vec![],
        records: vec![item("e:0", layer, 0, 9)],
        skipped: vec![],
        unavailable: vec![],
    };
    let locked_error = |uid: &str| ImportError::LayerLocked {
        layer_uid: uid.into(),
    };
    // (a) locked in the authored base document.
    let mut doc = base_doc(1);
    doc["layers"][1]["locked"] = json!(true);
    let h = history_of(&doc, "import-locked-base");
    let before = frozen(&h);
    assert_eq!(
        h.prepare_import_batch(&batch_into("L1")).unwrap_err(),
        locked_error("L1")
    );
    assert_eq!(frozen(&h), before, "a refused batch publishes nothing");
    // (b) locked afterwards through the real layer-policy owner (an undoable history entry).
    let mut h = history(1);
    let payload =
        json!({"command":"object.layer.policy.edit","layerUid":"L1","flag":"locked","value":true});
    let prepared = h
        .prepare_layer_policy_json(payload.to_string().as_bytes())
        .unwrap();
    h.commit_layer_policy(prepared).unwrap();
    let before = frozen(&h);
    assert_eq!(
        h.prepare_import_batch(&batch_into("L1")).unwrap_err(),
        locked_error("L1")
    );
    assert_eq!(frozen(&h), before);
    // The unlocked layer still takes the import, and a layer the batch creates is never locked.
    assert!(h.prepare_import_batch(&batch_into("L0")).unwrap().changed());
    let mixed = h.prepare_import_batch(&mixed_batch()).unwrap();
    assert!(mixed.changed());
    // The detached guard: a batch-created layer skips the lock check, a missing layer is internal.
    let mut detached: ObjectDocument = serde_json::from_value(json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"X","locked":true,"timing":timing(&[0])}],"objects":[]}))
    .unwrap();
    assert_eq!(super::expose_frame(&mut detached, "X", 0, true, 3), Ok(()));
    assert_eq!(
        super::expose_frame(&mut detached, "missing", 0, false, 3),
        Err(ImportError::Internal),
        "a layer that is neither created nor present is an internal invariant failure"
    );
}

//! L10-H negative and mutation controls: the comparisons of the other test files must be able to
//! FAIL. Every mutation below is applied to a produced record or a receipt and must be detected by
//! the same comparison the main tests use; the detectors themselves are shown to see each kind of
//! change; the error and wire vocabularies are pinned as tables.
use super::rows::{
    expected_from_serp, oracle, production_input, same_json, spelled_like_json_stringify, target,
};
use super::session::OP_IMPORT_BATCH;
use super::support::*;
use super::svg::path_record;
use super::wire::parse_batch;
use super::{ImportBatch, ImportError, ImportSkip, MissingProvider, SkipKind, UnavailableElement};
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::RemapError;
use crate::object_document::{AuthoredOrder, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

type Mutation = (&'static str, fn(&mut Value) -> bool);

fn bump(value: &mut Value, delta: f64) {
    *value = json!(value.as_f64().unwrap() + delta);
}

/// Each mutation reports whether it applied to this record (a dash mutation cannot apply to a record
/// without dashes).
const MUTATIONS: [Mutation; 14] = [
    ("closed flipped", |r: &mut Value| {
        let c = !r["geometry"]["closed"].as_bool().unwrap();
        r["geometry"]["closed"] = json!(c);
        true
    }),
    ("first x moved by one thousandth", |r: &mut Value| {
        bump(&mut r["geometry"]["segments"][0]["point"]["x"], 0.001);
        true
    }),
    ("last segment dropped", |r: &mut Value| {
        r["geometry"]["segments"].as_array_mut().unwrap().pop();
        true
    }),
    ("cap swapped", |r: &mut Value| {
        let c = if r["vector"]["strokeCap"] == "butt" {
            "round"
        } else {
            "butt"
        };
        r["vector"]["strokeCap"] = json!(c);
        true
    }),
    ("join swapped", |r: &mut Value| {
        let j = if r["vector"]["strokeJoin"] == "miter" {
            "bevel"
        } else {
            "miter"
        };
        r["vector"]["strokeJoin"] = json!(j);
        true
    }),
    ("width moved", |r: &mut Value| {
        bump(&mut r["vector"]["strokeWidth"], 1.0);
        true
    }),
    ("opacity changed", |r: &mut Value| {
        let o = if r["vector"]["opacity"] == json!(0.25) {
            0.75
        } else {
            0.25
        };
        r["vector"]["opacity"] = json!(o);
        true
    }),
    ("hasRealStroke flipped", |r: &mut Value| {
        let v = !r["vector"]["hasRealStroke"].as_bool().unwrap();
        r["vector"]["hasRealStroke"] = json!(v);
        true
    }),
    ("stroke red channel off by one byte", |r: &mut Value| {
        if r["vector"]["stroke"].is_null() {
            return false;
        }
        bump(&mut r["vector"]["stroke"]["r"], 1.0 / 255.0);
        true
    }),
    ("fill green channel off by one byte", |r: &mut Value| {
        if r["fill"].is_null() {
            return false;
        }
        bump(&mut r["fill"]["g"], 1.0 / 255.0);
        true
    }),
    ("fill removed", |r: &mut Value| {
        if r["fill"].is_null() {
            return false;
        }
        r["fill"] = Value::Null;
        true
    }),
    ("miter limit dropped", |r: &mut Value| {
        r["vector"]
            .as_object_mut()
            .unwrap()
            .remove("miterLimit")
            .is_some()
    }),
    ("dash offset moved", |r: &mut Value| {
        bump(&mut r["vector"]["dashOffset"], 0.5);
        true
    }),
    ("dash array dropped", |r: &mut Value| {
        r["vector"]
            .as_object_mut()
            .unwrap()
            .remove("dashArray")
            .is_some()
    }),
];

#[test]
fn every_mutation_of_a_converted_record_is_caught_by_the_oracle_comparison() {
    let (_, _, pairs) = oracle();
    let mut applied = [0usize; MUTATIONS.len()];
    for pair in &pairs {
        let (path, extras) = production_input(&pair.input);
        let produced =
            serde_json::to_value(path_record(&path, &extras, target()).unwrap()).unwrap();
        let expected = expected_from_serp(&pair.serp);
        assert!(same_json(&produced, &expected), "unmutated {}", pair.id);
        for (index, (name, mutate)) in MUTATIONS.iter().enumerate() {
            let mut mutant = produced.clone();
            if mutate(&mut mutant) {
                applied[index] += 1;
                assert!(
                    !same_json(&mutant, &expected),
                    "mutation '{name}' survived on {}",
                    pair.id
                );
            } else {
                assert_eq!(
                    mutant, produced,
                    "'{name}' reported not applied but changed the record"
                );
            }
        }
    }
    for (index, (name, _)) in MUTATIONS.iter().enumerate() {
        assert!(
            applied[index] >= 1,
            "'{name}' never applied: the control proves nothing"
        );
    }
}

#[test]
fn the_spelling_detector_sees_a_float_spelling_of_an_integral_value() {
    assert!(spelled_like_json_stringify(
        &json!({"a":10,"b":2.5,"c":[0,-3]})
    ));
    let ten_point_zero: Value = serde_json::from_str("[10.0]").unwrap();
    assert!(!spelled_like_json_stringify(&ten_point_zero));
    let tiny: Value = serde_json::from_str("[1e-7]").unwrap();
    assert!(spelled_like_json_stringify(&tiny));
}

#[test]
fn the_frozen_detector_sees_bytes_revision_and_history_changes() {
    let mut h = history(1);
    let original = frozen(&h);
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.commit_import_batch(prepared).unwrap();
    assert_ne!(frozen(&h), original, "a publish is visible");
    h.undo(1).unwrap();
    let undone = frozen(&h);
    assert_ne!(
        undone, original,
        "undo restores the bytes but not the revision or the stacks"
    );
    assert_eq!(undone.bytes, original.bytes);
    // The contrast of the no-op test: a CHANGING batch clears the redo stack.
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.commit_import_batch(prepared).unwrap();
    assert_eq!(
        h.history_depths(),
        (1, 0),
        "redo is cleared by a real change"
    );
    let mut primed = history(1);
    prime_redo(&mut primed);
    assert_eq!(
        primed.history_depths(),
        (0, 1),
        "the primed state really holds a redo entry"
    );
}

/// The accounting every receipt must satisfy against its batch.
fn accounted(batch: &ImportBatch, result: &super::ImportBatchResult) -> bool {
    let mut seen: Vec<&str> = result.layers.iter().map(|e| e.0.as_str()).collect();
    seen.extend(result.objects.iter().map(|e| e.0.as_str()));
    seen.extend(result.skipped.iter().map(|e| e.source_id.as_str()));
    let mut declared: Vec<&str> = batch.layers.iter().map(|e| e.source_id.as_str()).collect();
    declared.extend(batch.records.iter().map(|e| e.source_id.as_str()));
    declared.extend(batch.skipped.iter().map(|e| e.source_id.as_str()));
    seen.sort_unstable();
    declared.sort_unstable();
    seen == declared
}

#[test]
fn a_receipt_with_fewer_entries_than_the_input_fails_the_accounting() {
    let mut batch = mixed_batch();
    batch.skipped = vec![ImportSkip {
        kind: SkipKind::Element,
        source_id: "svg:3".into(),
        reason: "unsupported-element:Raster".into(),
    }];
    let h = history(1);
    let receipt = h.prepare_import_batch(&batch).unwrap().result().clone();
    assert!(accounted(&batch, &receipt));
    let mut without_skip = receipt.clone();
    without_skip.skipped.clear();
    assert!(
        !accounted(&batch, &without_skip),
        "a silently dropped skip entry is detected"
    );
    let mut without_object = receipt.clone();
    without_object.objects.pop();
    assert!(
        !accounted(&batch, &without_object),
        "a missing object is detected"
    );
    let mut renamed = receipt;
    renamed.layers[0].0 = "someone-else".into();
    assert!(
        !accounted(&batch, &renamed),
        "a receipt for other source ids is detected"
    );
}

#[test]
fn the_error_table_pins_every_code_and_gives_each_refusal_a_fixed_message() {
    let cases: Vec<(ImportError, DispatchErrorCode)> = vec![
        (ImportError::Payload, DispatchErrorCode::InvalidRequest),
        (
            ImportError::Remap(RemapError::RevisionExhausted),
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::Remap(RemapError::AllocationExhausted { index: 0 }),
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::Remap(RemapError::DuplicateSource { index: 1, first: 0 }),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::Remap(RemapError::CollidesWithOrigin { index: 0 }),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::Remap(RemapError::EmptySkipReason { index: 0 }),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::ProviderUnavailable {
                source_id: "x".into(),
                need: MissingProvider::Font,
            },
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::NotVector { index: 0 },
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::StrokeIdPreset { index: 0 },
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::ReferenceScope { index: 0 },
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::UnknownLayer {
                index: 0,
                reference: "x".into(),
            },
            DispatchErrorCode::NotFound,
        ),
        (
            ImportError::SkippedLayer {
                index: 0,
                reference: "x".into(),
            },
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::InvalidTarget { index: 0 },
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::ContextAbsent { index: 0 },
            DispatchErrorCode::NotFound,
        ),
        (
            ImportError::LayerLocked {
                layer_uid: "x".into(),
            },
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::KeyframeRequired {
                layer_uid: "x".into(),
                frame: 3,
            },
            DispatchErrorCode::Unavailable,
        ),
        (
            ImportError::Timing { index: 0 },
            DispatchErrorCode::InvalidRequest,
        ),
        (ImportError::Overflow, DispatchErrorCode::Unavailable),
        (
            ImportError::ReceiptTooLarge,
            DispatchErrorCode::InvalidRequest,
        ),
        (
            ImportError::Dispatch(DispatchErrorCode::StaleRevision),
            DispatchErrorCode::StaleRevision,
        ),
        (ImportError::Internal, DispatchErrorCode::Internal),
    ];
    for (error, code) in &cases {
        assert_eq!(error.code(), *code, "{error:?}");
        assert!(
            !error.message().is_empty() && error.message().len() < 80,
            "{error:?}"
        );
    }
}

#[test]
fn the_wire_vocabulary_round_trips_to_the_typed_labels() {
    for (name, need) in [
        ("raster-asset", MissingProvider::RasterAsset),
        ("text", MissingProvider::Text),
        ("font", MissingProvider::Font),
    ] {
        assert_eq!(need.label(), name);
        let mut payload = wire_batch(vec![], vec![]);
        payload["unavailable"] = json!([{"sourceId":"u","need":name}]);
        let batch = parse_batch(payload.to_string().as_bytes()).unwrap();
        assert_eq!(
            batch.unavailable,
            vec![UnavailableElement {
                source_id: "u".into(),
                need
            }]
        );
    }
    for (name, kind) in [
        ("layer", SkipKind::Layer),
        ("element", SkipKind::Element),
        ("asset", SkipKind::Asset),
    ] {
        assert_eq!(kind.label(), name);
        let mut payload = wire_batch(vec![], vec![]);
        payload["skipped"] = json!([{"kind":name,"sourceId":"u","reason":"why"}]);
        let batch = parse_batch(payload.to_string().as_bytes()).unwrap();
        assert_eq!(batch.skipped[0].kind, kind);
    }
    assert_eq!(
        OP_IMPORT_BATCH, OP,
        "the dispatcher operation the tests send is the one that is routed"
    );
}

#[test]
fn composition_order_entries_for_new_layers_have_no_authored_order() {
    // A new layer's order entry is empty in every context (the layer-create precedent); a record's
    // entry starts empty too. An authored order must come from a later edit, never from an import.
    let mut h = history(2);
    let prepared = h.prepare_import_batch(&mixed_batch()).unwrap();
    h.commit_import_batch(prepared).unwrap();
    let after = state(&h);
    let empty = AuthoredOrder {
        static_value: Presence::Missing,
        track: Presence::Missing,
        expression: Presence::Missing,
    };
    assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));
    for context in after["compositionOrder"]["contexts"].as_array().unwrap() {
        for layer in context["layers"].as_array().unwrap().iter().skip(2) {
            assert_eq!(layer["order"], json!({}), "{layer}");
        }
        for scope in context["scopes"].as_array().unwrap() {
            for element in scope["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["strokeId"] != "s0")
            {
                assert_eq!(element["order"], json!({}), "{element}");
            }
        }
    }
}

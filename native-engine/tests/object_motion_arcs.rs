//! Literal persistence/reader/rekey controls for the document-owned Arc prerequisite.
//! No native Shift, public command, history or installed acceptance is claimed here.
use crate::object_codec::{
    decode_ordered_project, decode_storage_project, encode_ordered_project, encode_storage_project,
    migrate_ordered_project,
};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
use build::{Build, Cell, Stroke};

fn fixture(form: Build) -> Value {
    let cells = vec![
        Cell {
            flag: 'K',
            strokes: vec![Stroke {
                id: "a".into(),
                z: 1,
                g: 10,
            }],
        },
        Cell {
            flag: '.',
            strokes: vec![],
        },
        Cell {
            flag: '.',
            strokes: vec![],
        },
        Cell {
            flag: 'K',
            strokes: vec![],
        },
    ];
    build::document_json(&[cells.clone(), cells], form)
}
fn with_arcs(value: &Value, arcs: &str) -> Vec<u8> {
    let mut text = value.to_string();
    assert_eq!(text.pop(), Some('}'));
    text.push_str(",\"motionArcs\":");
    text.push_str(arcs);
    text.push('}');
    text.into_bytes()
}
fn document(form: Build, arcs: &str) -> ObjectDocument {
    decode_storage_project(&with_arcs(&fixture(form), arcs)).unwrap()
}
fn bytes(document: &ObjectDocument) -> Vec<u8> {
    encode_storage_project(document).unwrap()
}
fn value(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&bytes(document)).unwrap()
}
fn assert_arcs(document: &ObjectDocument, expected: &str) {
    let encoded = String::from_utf8(bytes(document)).unwrap();
    assert!(
        encoded.contains(&format!("\"motionArcs\":{expected}")),
        "motionArcs field, record values or entry order differ"
    );
    let reopened = decode_storage_project(encoded.as_bytes()).unwrap();
    assert_eq!(bytes(&reopened), encoded.as_bytes());
}
fn reject_arcs(arcs: &str) {
    for form in [Build::V1, Build::V2] {
        assert!(
            decode_storage_project(&with_arcs(&fixture(form), arcs)).is_err(),
            "{form:?}: {arcs}"
        );
    }
}
fn handles(
    document: &ObjectDocument,
    uid: &str,
    a: i64,
    b: i64,
    index: u32,
) -> ([f64; 2], [f64; 2]) {
    let result = document
        .motion_arc_handles(uid, a, b, index, [3.0, 6.0], [12.0, 18.0])
        .unwrap();
    (result.out, result.incoming)
}

#[test]
fn missing_null_and_empty_arcs_round_trip_distinctly_in_all_four_storage_forms() {
    for form in Build::ALL {
        let original = fixture(form);
        let absent = decode_storage_project(original.to_string().as_bytes()).unwrap();
        assert_eq!(value(&absent), original);
        assert!(value(&absent).get("motionArcs").is_none());
        for raw in ["null", "{}"] {
            let doc = document(form, raw);
            let mut expected = original.clone();
            expected["motionArcs"] = serde_json::from_str(raw).unwrap();
            assert_eq!(value(&doc), expected);
            assert_arcs(&doc, raw);
            assert_eq!(handles(&doc, "L0", 0, 3, 0), ([6.0, 10.0], [9.0, 14.0]));
        }
    }
}

#[test]
fn authored_offsets_zero_and_independent_nullable_handles_match_original_reader() {
    let raw = r#"{"L0:0-3-0":{"out":[0,0],"in":[-2,3],"cx":99,"cy":-8},"L0:0-3-1":{"out":null,"in":[0,0]},"L0:0-3-2":{"out":[-4.5,2.25],"in":null}}"#;
    for form in Build::ALL {
        let doc = document(form, raw);
        assert_eq!(handles(&doc, "L0", 0, 3, 0), ([3.0, 6.0], [10.0, 21.0]));
        assert_eq!(handles(&doc, "L0", 0, 3, 1), ([6.0, 10.0], [12.0, 18.0]));
        assert_eq!(handles(&doc, "L0", 0, 3, 2), ([-1.5, 8.25], [9.0, 14.0]));
        assert_eq!(handles(&doc, "L1", 0, 3, 0), ([6.0, 10.0], [9.0, 14.0]));
        assert_arcs(&doc, raw);
    }
}

#[test]
fn legacy_single_handle_null_records_and_absent_index_use_straight_line_without_migration() {
    let raw = r#"{"L0:0-3-0":{"cx":-999.25,"cy":12},"L0:0-3-1":null,"L0:0-3-2":{"cx":null,"cy":null},"L0:0-3-3":{}}"#;
    let doc = document(Build::V2Extras, raw);
    for index in [0, 1, 2, 3, u32::MAX] {
        assert_eq!(handles(&doc, "L0", 0, 3, index), ([6.0, 10.0], [9.0, 14.0]));
    }
    assert_arcs(&doc, raw);
}

#[test]
fn arbitrary_literal_keys_and_escaped_utf8_do_not_acquire_an_invented_frame_grammar() {
    let raw = r#"{"":null,"old-unscoped-key":{"cx":7},"L:a-b:-2-1-4294967295":{"out":[1,2]},"é:\"\\":{}}"#;
    let doc = document(Build::V1Ref, raw);
    assert_eq!(
        handles(&doc, "L:a-b", -2, 1, u32::MAX),
        ([4.0, 8.0], [9.0, 14.0])
    );
    assert_arcs(&doc, raw);
}

#[test]
fn sequential_rekey_collisions_keep_target_position_then_append_new_targets_in_original_order() {
    let input = r#"{"keep":null,"L0:0-3-0":{"out":[1,2]},"L0:2-5-0":{"out":[9,9]},"L0:0-3-tail":null,"L00:0-3-0":{},"L0:0-30-0":{},"L0:unknown":{}}"#;
    let first = r#"{"keep":null,"L0:2-5-0":{"out":[1,2]},"L00:0-3-0":{},"L0:0-30-0":{},"L0:unknown":{},"L0:2-5-tail":null}"#;
    let final_arcs = r#"{"keep":null,"L00:0-3-0":{},"L0:0-30-0":{},"L0:unknown":{},"L0:-2-1-0":{"out":[1,2]},"L0:-2-1-tail":null}"#;
    for form in Build::ALL {
        let mut doc = document(form, input);
        let mut untouched = value(&doc);
        untouched.as_object_mut().unwrap().remove("motionArcs");
        doc.retime_motion_arcs("L0", &[(0, 3, 2, 5)]).unwrap();
        assert_arcs(&doc, first);
        doc.retime_motion_arcs("L0", &[(2, 5, -2, 1)]).unwrap();
        assert_arcs(&doc, final_arcs);
        let mut actual = value(&doc);
        actual.as_object_mut().unwrap().remove("motionArcs");
        assert_eq!(
            actual, untouched,
            "no timing, records, contexts, references or layers changed"
        );
        assert_eq!(handles(&doc, "L0", -2, 1, 0), ([4.0, 8.0], [9.0, 14.0]));
        let mut single = document(form, input);
        single
            .retime_motion_arcs("L0", &[(0, 3, 2, 5), (2, 5, -2, 1)])
            .unwrap();
        assert_eq!(
            bytes(&single),
            bytes(&doc),
            "one call must honor sequential collision semantics"
        );
    }
}

#[test]
fn rekey_is_scoped_to_exact_uid_prefix_and_carries_every_suffix_without_parsing_it() {
    let mut doc = document(
        Build::V2,
        r#"{"a:b-c:0-3-0":null,"a:b-c:0-3-odd-suffix":{"in":[2,4]},"a:b-c-more:0-3-0":{},"a:b-c:0-30-0":{},"0-3-0":{}}"#,
    );
    doc.retime_motion_arcs("a:b-c", &[(0, 3, -2, 1)]).unwrap();
    assert_arcs(
        &doc,
        r#"{"a:b-c-more:0-3-0":{},"a:b-c:0-30-0":{},"0-3-0":{},"a:b-c:-2-1-0":null,"a:b-c:-2-1-odd-suffix":{"in":[2,4]}}"#,
    );
}

#[test]
fn absent_null_empty_and_same_pair_retime_preserve_exact_presence_and_bytes() {
    for form in [Build::V1, Build::V2] {
        for raw in [None, Some("null"), Some("{}"), Some(r#"{"L0:0-3-0":null}"#)] {
            let mut doc = match raw {
                None => decode_storage_project(fixture(form).to_string().as_bytes()).unwrap(),
                Some(arcs) => document(form, arcs),
            };
            let before = bytes(&doc);
            for pairs in [vec![], vec![(0, 3, 0, 3)], vec![(7, 8, -2, -1)]] {
                doc.retime_motion_arcs("L0", &pairs).unwrap();
                assert_eq!(bytes(&doc), before);
            }
        }
    }
}

#[test]
fn invalid_pair_later_in_batch_or_pair_budget_refuses_before_any_arc_assignment() {
    let original = r#"{"L0:0-3-0":{"out":[5,6]},"L0:2-5-0":null}"#;
    let max = 9_007_199_254_740_991_i64;
    let bad_batches = vec![
        vec![(0, 3, 2, 5), (0, 3, max + 1, 2)],
        vec![(0, 3, 2, 5), (-max - 1, 3, 2, 5)],
        vec![(0, max + 1, 2, 5)],
        vec![(0, 3, 2, -max - 1)],
        vec![(0, 3, 0, 3); 4097],
    ];
    for pairs in bad_batches {
        let mut doc = document(Build::V2, original);
        let before = bytes(&doc);
        assert!(doc.retime_motion_arcs("L0", &pairs).is_err());
        assert_eq!(bytes(&doc), before);
    }
    let mut doc = document(Build::V1, original);
    let before = bytes(&doc);
    assert!(doc.retime_motion_arcs("", &[(0, 3, 2, 5)]).is_err());
    assert_eq!(bytes(&doc), before);
    doc.retime_motion_arcs("L0", &vec![(0, 3, 0, 3); 4096])
        .unwrap();
    assert_eq!(bytes(&doc), before);
    doc.retime_motion_arcs("L0", &[(0, 3, -max, max)]).unwrap();
    assert_arcs(
        &doc,
        r#"{"L0:2-5-0":null,"L0:-9007199254740991-9007199254740991-0":{"out":[5,6]}}"#,
    );
}

#[test]
fn reader_rejects_unsafe_pairs_empty_uid_and_nonfinite_anchors_without_mutation() {
    let doc = document(Build::V2, r#"{"L0:0-3-0":{"out":[1,2]}}"#);
    let before = bytes(&doc);
    assert!(doc
        .motion_arc_handles("", 0, 3, 0, [0.0, 0.0], [3.0, 3.0])
        .is_err());
    for (a, b) in [(i64::MAX, 3), (0, i64::MIN)] {
        assert!(doc
            .motion_arc_handles("L0", a, b, 0, [0.0, 0.0], [3.0, 3.0])
            .is_err());
    }
    for point in [
        [f64::NAN, 0.0],
        [0.0, f64::INFINITY],
        [f64::NEG_INFINITY, 0.0],
    ] {
        assert!(doc
            .motion_arc_handles("L0", 0, 3, 0, point, [3.0, 3.0])
            .is_err());
        assert!(doc
            .motion_arc_handles("L0", 0, 3, 0, [0.0, 0.0], point)
            .is_err());
    }
    assert_eq!(bytes(&doc), before);
}

#[test]
fn strict_arc_raw_duplicates_unknown_fields_and_nonfinite_or_nonvector_values_refuse() {
    for raw in [
        "[]",
        "false",
        "3",
        "\"arcs\"",
        r#"{"k":[]}"#,
        r#"{"k":false}"#,
        r#"{"k":{},"k":null}"#,
        r#"{"k":{"out":[1,2],"out":[3,4]}}"#,
        r#"{"k":{"unknown":1}}"#,
        r#"{"k":{"out":[]}}"#,
        r#"{"k":{"out":[1]}}"#,
        r#"{"k":{"in":[1,2,3]}}"#,
        r#"{"k":{"out":[null,2]}}"#,
        r#"{"k":{"in":[true,2]}}"#,
        r#"{"k":{"out":{"x":1,"y":2}}}"#,
        r#"{"k":{"out":["1",2]}}"#,
        r#"{"k":{"cx":false}}"#,
        r#"{"k":{"cy":[]}}"#,
        r#"{"k":{"out":[1e400,2]}}"#,
        r#"{"k":{"cx":1e400}}"#,
    ] {
        reject_arcs(raw);
    }
    for form in [Build::V1, Build::V2] {
        let mut raw = with_arcs(&fixture(form), "{}");
        raw.pop();
        raw.extend_from_slice(b",\"motionArcs\":null}");
        assert!(
            decode_storage_project(&raw).is_err(),
            "duplicate document member"
        );
    }
}

#[test]
fn arc_key_utf8_and_entry_limits_admit_exact_bound_and_refuse_one_over() {
    let key = "é".repeat(512);
    assert_eq!(key.len(), 1024);
    let raw = format!("{{{}:null}}", serde_json::to_string(&key).unwrap());
    let doc = document(Build::V1, &raw);
    assert_arcs(&doc, &raw);
    reject_arcs(&format!(
        "{{{}:null}}",
        serde_json::to_string(&(key + "x")).unwrap()
    ));
    let entries: Vec<String> = (0..4096).map(|i| format!("\"k{i}\":null")).collect();
    let raw = format!("{{{}}}", entries.join(","));
    assert_arcs(&document(Build::V2, &raw), &raw);
    reject_arcs(&format!("{{{},\"overflow\":null}}", entries.join(",")));
}

#[test]
fn encoded_store_budget_counts_actual_keys_punctuation_and_null_at_exact_four_mib() {
    let entries: Vec<String> = (0..4096)
        .map(|i| {
            let tail = if i == 0 { 1011 } else { 1012 };
            format!("\"{i:04}{}\":null", "k".repeat(tail))
        })
        .collect();
    let raw = format!("{{{}}}", entries.join(","));
    assert_eq!(raw.len(), 4 * 1024 * 1024);
    let doc = document(Build::V2, &raw);
    assert_arcs(&doc, &raw);
    let over = raw.replacen("0000", "0000x", 1);
    assert_eq!(over.len(), raw.len() + 1);
    assert!(decode_storage_project(&with_arcs(&fixture(Build::V2), &over)).is_err());
}

#[test]
fn retime_that_expands_encoded_keys_beyond_budget_leaves_the_complete_document_unchanged() {
    let uid = "L".repeat(980);
    let mut keys: Vec<String> = (0..4096)
        .map(|i| {
            if i == 0 {
                format!("{uid}:0-3-0")
            } else {
                format!("{i:04}{}", "k".repeat(1012))
            }
        })
        .collect();
    let initial_cost = 1 + keys.iter().map(|key| key.len() + 8).sum::<usize>();
    let mut remaining = 4 * 1024 * 1024 - initial_cost;
    for key in keys.iter_mut().skip(1) {
        let padding = remaining.min(1024 - key.len());
        key.push_str(&"x".repeat(padding));
        remaining -= padding;
    }
    assert_eq!(remaining, 0);
    assert!(keys.iter().all(|key| key.len() <= 1024));
    let entries: Vec<String> = keys.iter().map(|key| format!("\"{key}\":null")).collect();
    let raw = format!("{{{}}}", entries.join(","));
    assert_eq!(raw.len(), 4 * 1024 * 1024);
    assert!(format!("{uid}:-9007199254740991-9007199254740991-0").len() <= 1024);
    let mut doc = document(Build::V1, &raw);
    let before = bytes(&doc);
    assert!(doc
        .retime_motion_arcs(&uid, &[(0, 3, -9007199254740991, 9007199254740991)])
        .is_err());
    assert_eq!(bytes(&doc), before);
}

fn locks(form: Build, mode: Option<Value>) -> Value {
    let mut source = fixture(form);
    source["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"opacity","track":{"keys":[{"frame":0,"v":[7]},{"frame":3,"v":[9]}]}}]}]);
    if let Some(mode) = mode {
        source["layers"][0]["keyLock"] = mode.clone();
        source["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["lockTo"] = mode.clone();
        if form.v2() {
            source["compositionOrder"]["contexts"][0]["layers"][0]["order"] =
                json!({"track":{"keys":[{"frame":0,"v":[0],"lockTo":mode}]}});
        }
    }
    source
}

#[test]
fn layer_and_property_order_key_lock_modes_preserve_missing_null_and_strings_through_codecs() {
    for form in Build::ALL {
        for mode in [
            None,
            Some(Value::Null),
            Some(json!("in")),
            Some(json!("out")),
            Some(json!("layer")),
        ] {
            let source = locks(form, mode);
            let doc = decode_storage_project(&with_arcs(&source, "null")).unwrap();
            let mut expected = source;
            expected["motionArcs"] = Value::Null;
            assert_eq!(value(&doc), expected);
            assert_eq!(
                value(&decode_storage_project(&bytes(&doc)).unwrap()),
                expected
            );
            let cloned = doc.clone();
            assert_eq!(bytes(&cloned), bytes(&doc));
        }
    }
}

#[test]
fn malformed_lock_modes_refuse_at_layer_property_and_order_keys_without_visibility_widening() {
    for form in [Build::V1, Build::V2] {
        for invalid in [
            json!(false),
            json!(0),
            json!(""),
            json!("IN"),
            json!("other"),
            json!({"in":{}}),
            json!([]),
        ] {
            let valid = locks(form, Some(json!("in")));
            let mut bad = valid.clone();
            bad["layers"][0]["keyLock"] = invalid.clone();
            assert!(decode_storage_project(bad.to_string().as_bytes()).is_err());
            let mut bad = valid.clone();
            bad["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["lockTo"] =
                invalid.clone();
            assert!(decode_storage_project(bad.to_string().as_bytes()).is_err());
            if form.v2() {
                let mut bad = valid;
                bad["compositionOrder"]["contexts"][0]["layers"][0]["order"]["track"]["keys"][0]
                    ["lockTo"] = invalid;
                assert!(decode_ordered_project(bad.to_string().as_bytes()).is_err());
            }
        }
    }
}

#[test]
fn explicit_v1_to_v2_migration_preserves_arcs_and_locks_without_interpreting_keys() {
    let raw = r#"{"L0:-2-1-0":{"out":[1,2]},"legacy-pair":{"cx":7,"cy":null}}"#;
    let source = locks(Build::V1, Some(json!("layer")));
    let doc = decode_storage_project(&with_arcs(&source, raw)).unwrap();
    let ordered_source = fixture(Build::V2);
    let plan = &ordered_source["compositionOrder"];
    let migrated = migrate_ordered_project(&doc, plan.to_string().as_bytes()).unwrap();
    let encoded = encode_ordered_project(&migrated).unwrap();
    let reopened = decode_ordered_project(&encoded).unwrap();
    assert_eq!(encode_ordered_project(&reopened).unwrap(), encoded);
    let storage = decode_storage_project(&encoded).unwrap();
    let mut expected: Value = serde_json::from_slice(&with_arcs(&source, raw)).unwrap();
    expected["formatVersion"] = json!(2);
    expected["compositionOrder"] = plan.clone();
    assert_eq!(value(&storage), expected);
    assert_arcs(&storage, raw);
    assert_eq!(handles(&storage, "L0", -2, 1, 0), ([4.0, 8.0], [9.0, 14.0]));
}

#[cfg(feature = "commands")]
#[test]
fn real_native_snapshot_retains_original_arc_order_and_locks_after_detached_retime_and_reopen() {
    let source = locks(Build::V2Extras, Some(json!("out")));
    let raw = r#"{"L0:0-3-0":{"out":[4,5]},"L1:0-3-0":null}"#;
    let admitted = decode_storage_project(&with_arcs(&source, raw)).unwrap();
    let retained =
        crate::revision::ObjectSnapshot::from_admitted("arcs-owner".into(), admitted).unwrap();
    let before = bytes(retained.document());
    let mut detached = retained.document().clone();
    detached.retime_motion_arcs("L0", &[(0, 3, -2, 1)]).unwrap();
    let fresh = decode_storage_project(&bytes(&detached)).unwrap();
    assert_arcs(&fresh, r#"{"L1:0-3-0":null,"L0:-2-1-0":{"out":[4,5]}}"#);
    assert_eq!(bytes(retained.document()), before);
    assert_eq!(retained.content_revision(), 0);
    assert_eq!(value(&fresh)["layers"], source["layers"]);
    assert_eq!(value(&fresh)["propertyHolders"], source["propertyHolders"]);
    assert_ne!(bytes(&fresh), before);
}

#[cfg(feature = "history")]
#[test]
fn actual_property_key_edit_retains_existing_lock_and_new_key_omits_lock_without_touching_arcs() {
    for form in Build::ALL {
        let source = locks(form, Some(json!("in")));
        let raw = r#"{"L0:0-3-0":{"out":[1,2]}}"#;
        let admitted = decode_storage_project(&with_arcs(&source, raw)).unwrap();
        let mut owner =
            crate::commands::ObjectFillOwner::new("lock-property-owner", admitted).unwrap();
        let retained = owner.acquire_snapshot(0).unwrap();
        let before = bytes(retained.document());
        for (frame, values) in [(0, json!([11])), (1, json!([5]))] {
            let payload = json!({"command":"object.property.edit",
                "target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity",
                "action":{"kind":"setKey","frame":frame,"values":values,"curvePoints":[]}});
            let candidate = owner
                .prepare_property_edit_json(payload.to_string().as_bytes())
                .unwrap();
            owner.commit_property_edit(candidate).unwrap();
        }
        let current = owner.acquire_snapshot(2).unwrap();
        let mut expected: Value = serde_json::from_slice(&with_arcs(&source, raw)).unwrap();
        expected["propertyHolders"][0]["properties"][0]["track"]["keys"] = json!([
            {"frame":0,"v":[11],"lockTo":"in","curvePoints":[]},
            {"frame":1,"v":[5],"curvePoints":[],"hIn":[0,0],"hOut":[0,0]},
            {"frame":3,"v":[9]}]);
        assert_eq!(value(current.document()), expected);
        assert_arcs(current.document(), raw);
        assert_eq!(bytes(retained.document()), before);
    }
}

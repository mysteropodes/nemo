//! Oracle-backed and literal tests for the image-mesh store record (L03-J, F17).
//!
//! The oracle rows and their comparison harness live in `object_image_mesh_record_rows.rs`;
//! rejection tables, mutants and guards in `object_image_mesh_record_controls.rs`. Literal
//! expectations below are hand-derived from the baseline JS or copied from decoded oracle
//! rows, never produced by the record under test.

use super::oracle_rows::{
    expected_last, issue_rows, issue_three, load_rows, loaded_state_mismatches, same_points,
    sequence_mismatches,
};
use super::{
    baseline_sequence_of, ImageMeshAdmissionError, ImageMeshStore, MeshIdSequence, DEFAULT_DENSITY,
    MAX_SAFE_SEQUENCE,
};
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_ordered_project,
    encode_project, encode_storage_project, migrate_ordered_project, ObjectCodecErrorKind,
};
use crate::object_document::Presence;
use serde_json::Value;

/// `tests/fixtures/lib/corpus.cjs:385-398`: outline 8, verts 9, fan of 8 triangles, offsets[8]=[0.1,0].
const CORPUS: &str = r#"{"im_1":{"outline":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5]],"verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],"tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],"offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0.1,0]],"cols":2,"rows":2}}"#;
const TRIANGLE: &str = r#"[[0,0],[1,0],[0,1]]"#;
const PLAN: &str = r#"{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],"scopes":[]}],"migration":"explicit-plan-required","referencePolicy":"unresolved"}"#;
const V1_HEAD: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":2,"layers":[{"layerUid":"L"}],"objects":[]"#;
const V2_HEAD: &str = r#"{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,"layers":[{"layerUid":"L"}],"objects":[]"#;

fn store(text: &str) -> ImageMeshStore {
    serde_json::from_str(text).expect("store decodes")
}

fn triangle_store(extra: &str) -> String {
    format!(r#"{{"im_1":{{"outline":{TRIANGLE},"verts":{TRIANGLE},"tris":[0,1,2]{extra}}}}}"#)
}

fn v1(meshes: Option<&str>) -> String {
    match meshes {
        Some(m) => format!(r#"{V1_HEAD},"imageMeshes":{m}}}"#),
        None => format!("{V1_HEAD}}}"),
    }
}

fn v2(meshes: &str) -> String {
    format!(r#"{V2_HEAD},"compositionOrder":{PLAN},"imageMeshes":{meshes}}}"#)
}

fn member(bytes: &[u8], key: &str) -> Value {
    let value: Value = serde_json::from_slice(bytes).expect("encoded json");
    value[key].clone()
}

#[test]
fn oracle_row_counts_are_pinned() {
    let rows = load_rows();
    assert_eq!(rows.len(), 67);
    assert_eq!(rows.iter().filter(|r| r.bytes).count(), 59);
    assert_eq!(issue_rows().len(), 9);
}

#[test]
fn oracle_stores_admit_and_evaluate_like_the_baseline_loaded_state() {
    for row in load_rows() {
        let decoded = store(&row.input);
        assert_eq!(decoded.validate(), Ok(()), "{}", row.name);
        let diffs = loaded_state_mismatches(&row, &decoded);
        assert!(diffs.is_empty(), "{}", diffs.join("\n"));
    }
}

#[test]
fn oracle_stores_reencode_in_native_field_order_and_keep_authored_absence() {
    for row in load_rows().into_iter().filter(|r| r.bytes) {
        let encoded = serde_json::to_string(&store(&row.input)).expect("encode");
        assert_eq!(encoded, row.expect, "{}", row.name);
    }
}

#[test]
fn oracle_issued_ids_follow_the_baseline_counter() {
    for row in load_rows() {
        let decoded = store(&row.input);
        let mut sequence = decoded.id_sequence().expect("safe ids");
        assert_eq!(sequence.last(), expected_last(&row), "{}", row.name);
        assert_eq!(
            issue_three(&mut sequence, &decoded),
            row.issued,
            "{}",
            row.name
        );
    }
}

#[test]
fn oracle_issue_rows_skip_ids_that_are_already_taken() {
    for row in issue_rows() {
        let table: Vec<String> = row
            .ids
            .iter()
            .map(|id| {
                format!(
                    "{}:{{\"outline\":[],\"verts\":[],\"tris\":[]}}",
                    Value::from(id.as_str())
                )
            })
            .collect();
        let decoded = store(&format!("{{{}}}", table.join(",")));
        // A store handed over without load(): the counter starts at zero and only issue() moves it.
        let mut sequence = MeshIdSequence::default();
        assert_eq!(
            issue_three(&mut sequence, &decoded),
            row.issued,
            "{}",
            row.name
        );
    }
}

#[test]
fn production_sequence_rule_matches_every_oracle_row() {
    let rule = |id: &str| baseline_sequence_of(id).expect("safe").unwrap_or(0);
    let diffs = sequence_mismatches(&rule);
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn corpus_entry_im_1_has_the_assigned_shape_and_reencodes_byte_identically() {
    let decoded = store(CORPUS);
    assert_eq!(decoded.validate(), Ok(()));
    assert_eq!(decoded.len(), 1);
    let (id, mesh) = &decoded.entries()[0];
    assert_eq!(id, "im_1");
    assert_eq!(mesh.outline.len(), 8);
    assert_eq!(mesh.verts.len(), 9);
    assert_eq!(mesh.triangles().len(), 24);
    let fan: Vec<u32> = (0..8u32).flat_map(|i| [8, i, (i + 1) % 8]).collect();
    assert_eq!(mesh.triangles(), fan.as_slice());
    assert_eq!((mesh.effective_cols(), mesh.effective_rows()), (2, 2));
    assert_eq!(serde_json::to_string(&decoded).expect("encode"), CORPUS);
}

#[test]
fn outline_is_the_vertex_prefix_and_offsets_stay_separate_from_rest() {
    let mesh = store(CORPUS).entries()[0].1.clone();
    let rest = mesh.rest_points();
    assert!(same_points(&mesh.outline_points(), &rest[..8]));
    assert_eq!(rest[8], [0.5, 0.5]);
    let offsets = mesh.effective_offsets();
    assert_eq!(offsets[8], [0.1, 0.0]);
    assert!(offsets[..8].iter().all(|o| *o == [0.0, 0.0]));
    // Deformed = rest + offset is the consumer's sum; the record never folds it.
    assert_eq!(
        [rest[8][0] + offsets[8][0], rest[8][1] + offsets[8][1]],
        [0.6, 0.5]
    );
    assert_eq!(rest[8], [0.5, 0.5]);
}

#[test]
fn absent_offsets_cols_rows_stay_missing_and_evaluate_as_the_load_defaults() {
    let text = triangle_store("");
    let decoded = store(&text);
    let mesh = &decoded.entries()[0].1;
    assert!(!mesh.offsets_authored());
    assert!(mesh.cols.missing() && mesh.rows.missing());
    assert_eq!(mesh.effective_offsets(), vec![[0.0, 0.0]; 3]);
    assert_eq!((mesh.effective_cols(), mesh.effective_rows()), (4, 4));
    assert_eq!(DEFAULT_DENSITY, 4);
    assert_eq!(serde_json::to_string(&decoded).expect("encode"), text);
}

#[test]
fn explicit_zero_offsets_and_explicit_four_stay_explicit_and_differ_from_absent() {
    let explicit = triangle_store(r#","offsets":[[0,0],[0,0],[0,0]],"cols":4,"rows":4"#);
    let decoded = store(&explicit);
    let mesh = &decoded.entries()[0].1;
    assert!(mesh.offsets_authored());
    assert_eq!(
        (mesh.cols.value().copied(), mesh.rows.value().copied()),
        (Some(4), Some(4))
    );
    assert_eq!(serde_json::to_string(&decoded).expect("encode"), explicit);
    assert_ne!(decoded, store(&triangle_store("")));
    // Evaluation agrees while authorship differs.
    assert_eq!(
        mesh.effective_offsets(),
        store(&triangle_store("")).entries()[0]
            .1
            .effective_offsets()
    );
}

#[test]
fn density_is_authored_per_axis() {
    let mixed = store(&triangle_store(r#","cols":64"#));
    let mesh = &mixed.entries()[0].1;
    assert_eq!((mesh.effective_cols(), mesh.effective_rows()), (64, 4));
    assert!(mesh.rows.missing());
}

#[test]
fn triangles_are_a_plain_integer_array_on_the_wire() {
    let encoded = serde_json::to_value(store(CORPUS)).expect("value");
    let tris = &encoded["im_1"]["tris"];
    assert!(tris.is_array());
    assert_eq!(tris.as_array().map(Vec::len), Some(24));
    assert_eq!(tris[0], 8);
}

#[test]
fn entry_order_is_authored_order_not_numeric_order() {
    let text = format!(
        "{{\"im_9\":{},\"im_2\":{}}}",
        r#"{"outline":[],"verts":[],"tris":[]}"#, r#"{"outline":[],"verts":[],"tris":[]}"#
    );
    let decoded = store(&text);
    let ids: Vec<&str> = decoded
        .entries()
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(ids, ["im_9", "im_2"]);
    assert_eq!(serde_json::to_string(&decoded).expect("encode"), text);
}

#[test]
fn numeric_suffixes_advance_the_counter_and_the_baseline_quirks_are_kept() {
    let advance = |id: &str| baseline_sequence_of(id).expect("safe");
    assert_eq!(advance("im_7"), Some(7));
    assert_eq!(advance("im_007"), Some(7));
    assert_eq!(advance("im_12abc"), Some(12));
    assert_eq!(advance("7"), Some(7), "parseInt takes a bare number too");
    assert_eq!(
        advance("im_ 9"),
        Some(9),
        "parseInt skips leading whitespace"
    );
    assert_eq!(advance("im_+8"), Some(8));
    for none in [
        "abc",
        "im_",
        "im_0",
        "im_-3",
        "im_im_9",
        "IM_4",
        "im-4",
        "im_0x10",
        "im_\u{180E}5",
    ] {
        assert_eq!(advance(none), None, "{none}");
    }
    assert_eq!(advance("im_2.9"), Some(2));
    assert_eq!(advance("im_1e3"), Some(1));
}

#[test]
fn counter_takes_the_largest_suffix_and_ignores_other_ids() {
    let decoded = store(
        r#"{"im_7":{"outline":[],"verts":[],"tris":[]},"im_3":{"outline":[],"verts":[],"tris":[]},"other":{"outline":[],"verts":[],"tris":[]}}"#,
    );
    assert_eq!(decoded.id_sequence().map(|s| s.last()), Ok(7));
    assert_eq!(decoded.entries()[2].0, "other");
    let only_other = store(r#"{"other":{"outline":[],"verts":[],"tris":[]}}"#);
    assert_eq!(only_other.id_sequence().map(|s| s.last()), Ok(0));
}

#[test]
fn issue_skips_taken_ids_and_never_lowers_the_counter() {
    let taken = store(
        r#"{"im_1":{"outline":[],"verts":[],"tris":[]},"im_2":{"outline":[],"verts":[],"tris":[]},"im_4":{"outline":[],"verts":[],"tris":[]}}"#,
    );
    let mut sequence = MeshIdSequence::default();
    assert_eq!(sequence.issue(&taken).as_deref(), Ok("im_3"));
    assert_eq!(sequence.last(), 3);
    assert_eq!(sequence.issue(&taken).as_deref(), Ok("im_5"));
    let mut high = MeshIdSequence::default();
    high.observe("im_40").expect("safe");
    high.observe("im_9").expect("safe");
    assert_eq!(high.last(), 40);
}

#[test]
fn unsafe_ids_and_exhaustion_are_typed_and_leave_the_counter_alone() {
    let past = format!("im_{}", MAX_SAFE_SEQUENCE + 1);
    assert_eq!(
        baseline_sequence_of(&past),
        Err(ImageMeshAdmissionError::IdSequenceUnsafe)
    );
    assert_eq!(
        baseline_sequence_of("99999999999999999999999999"),
        Err(ImageMeshAdmissionError::IdSequenceUnsafe)
    );
    assert_eq!(
        baseline_sequence_of("im_9007199254740991"),
        Ok(Some(MAX_SAFE_SEQUENCE))
    );
    let mixed = store(&format!(
        "{{\"im_5\":{{\"outline\":[],\"verts\":[],\"tris\":[]}},\"{past}\":{{\"outline\":[],\"verts\":[],\"tris\":[]}}}}"
    ));
    assert_eq!(
        mixed.validate(),
        Err(ImageMeshAdmissionError::IdSequenceUnsafe)
    );
    let mut sequence = MeshIdSequence::default();
    assert_eq!(
        sequence.observe_store(&mixed),
        Err(ImageMeshAdmissionError::IdSequenceUnsafe)
    );
    assert_eq!(sequence.last(), 0, "atomic: im_5 was not applied");
    let mut full = MeshIdSequence {
        last: MAX_SAFE_SEQUENCE,
    };
    assert_eq!(
        full.issue(&mixed),
        Err(ImageMeshAdmissionError::IdSequenceExhausted)
    );
    assert_eq!(full.last(), MAX_SAFE_SEQUENCE);
}

#[test]
fn try_new_runs_the_same_admission_as_the_wire() {
    let decoded = store(CORPUS);
    assert_eq!(
        ImageMeshStore::try_new(decoded.entries().to_vec()).as_ref(),
        Ok(&decoded)
    );
    let mut bad = decoded.entries().to_vec();
    bad[0].1.tris.push(0);
    assert_eq!(
        ImageMeshStore::try_new(bad),
        Err(ImageMeshAdmissionError::TrianglesNotTriples)
    );
}

#[test]
fn document_without_the_store_reencodes_byte_identically() {
    let text = v1(None);
    let document = decode_project(text.as_bytes()).expect("decode");
    assert!(document.image_meshes.missing());
    assert_eq!(encode_project(&document).expect("encode"), text.as_bytes());
}

#[test]
fn document_with_the_store_roundtrips_v1_bytes() {
    let text = v1(Some(CORPUS));
    let document = decode_project(text.as_bytes()).expect("decode");
    assert_eq!(document.image_meshes.value(), Some(&store(CORPUS)));
    assert_eq!(encode_project(&document).expect("encode"), text.as_bytes());
}

#[test]
fn present_empty_store_stays_present_and_differs_from_absent() {
    let text = v1(Some("{}"));
    let document = decode_project(text.as_bytes()).expect("decode");
    assert_eq!(
        document.image_meshes.value().map(ImageMeshStore::len),
        Some(0)
    );
    assert_eq!(encode_project(&document).expect("encode"), text.as_bytes());
    assert_ne!(
        document,
        decode_project(v1(None).as_bytes()).expect("decode")
    );
}

#[test]
fn v2_storage_keeps_the_store_through_decode_encode_and_migration() {
    let text = v2(CORPUS);
    let document = decode_storage_project(text.as_bytes()).expect("storage decode");
    assert_eq!(document.image_meshes.value(), Some(&store(CORPUS)));
    let bytes = encode_storage_project(&document).expect("storage encode");
    assert_eq!(
        member(&bytes, "imageMeshes"),
        serde_json::from_str::<Value>(CORPUS).unwrap()
    );
    assert_eq!(decode_storage_project(&bytes).expect("reopen"), document);
    let ordered = decode_ordered_project(text.as_bytes()).expect("ordered decode");
    let reencoded = encode_ordered_project(&ordered).expect("ordered encode");
    assert_eq!(
        member(&reencoded, "imageMeshes"),
        serde_json::from_str::<Value>(CORPUS).unwrap()
    );
    let from_v1 = decode_project(v1(Some(CORPUS)).as_bytes()).expect("v1");
    let migrated = migrate_ordered_project(&from_v1, PLAN.as_bytes()).expect("migrate");
    let migrated_bytes = encode_ordered_project(&migrated).expect("encode");
    assert_eq!(
        member(&migrated_bytes, "imageMeshes"),
        serde_json::from_str::<Value>(CORPUS).unwrap()
    );
    let plain = decode_storage_project(
        text.replace(&format!(",\"imageMeshes\":{CORPUS}"), "")
            .as_bytes(),
    )
    .expect("v2 without store");
    assert!(plain.image_meshes.missing());
    let plain_bytes = encode_storage_project(&plain).expect("encode");
    assert!(member(&plain_bytes, "imageMeshes").is_null());
}

#[test]
fn documents_reject_a_bad_store_with_typed_codec_kinds_on_both_wires() {
    let dup = r#"{"im_1":{"outline":[],"verts":[],"tris":[]},"im_1":{"outline":[],"verts":[],"tris":[]}}"#;
    let bad_index = triangle_store("").replace("[0,1,2]", "[0,1,3]");
    let extra = triangle_store(r#","bones":[]"#);
    let kind = |text: String, ordered: bool| {
        let result = if ordered {
            decode_ordered_project(text.as_bytes()).map(|_| ())
        } else {
            decode_project(text.as_bytes()).map(|_| ())
        };
        result.expect_err("rejected").kind()
    };
    assert_eq!(
        kind(v1(Some(dup)), false),
        ObjectCodecErrorKind::DuplicateId
    );
    assert_eq!(kind(v2(dup), true), ObjectCodecErrorKind::DuplicateId);
    assert_eq!(
        kind(v1(Some(&bad_index)), false),
        ObjectCodecErrorKind::Invalid
    );
    assert_eq!(kind(v2(&bad_index), true), ObjectCodecErrorKind::Invalid);
    assert_eq!(
        kind(v1(Some(&extra)), false),
        ObjectCodecErrorKind::Unsupported
    );
    assert_eq!(kind(v2(&extra), true), ObjectCodecErrorKind::Unsupported);
    assert!(decode_project(format!("{V1_HEAD},\"imageMeshes\":null}}").as_bytes()).is_err());
}

#[test]
fn encoding_an_inadmissible_store_is_refused_without_bytes() {
    let mut document = decode_project(v1(Some(CORPUS)).as_bytes()).expect("decode");
    let mut entries = store(CORPUS).entries().to_vec();
    entries[0].1.tris[0] = 9;
    document.image_meshes = Presence::Present(ImageMeshStore { entries });
    assert_eq!(
        encode_project(&document).expect_err("refused").kind(),
        ObjectCodecErrorKind::Invalid
    );
}

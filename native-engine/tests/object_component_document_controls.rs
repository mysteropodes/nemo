//! Negative and mutation controls for the document admission of the lane-4 tables
//! (L04-I). Every case starts from the baseline-derived golden document (proved
//! admissible first), changes ONE thing and must be refused with its own rule: the
//! kind AND a fragment of the message are pinned, so a refusal by an unrelated check
//! does not satisfy a case. Admission goes through the ordered codec, which returns
//! the v2 error itself (the storage entry reports the v1 attempt's error instead).
use super::cases::{apply, cases};
use super::tests::{
    admit, document, golden, golden_value, tables, with_member, without_components,
};
use super::*;
use crate::object_codec::{
    decode_ordered_project, decode_storage_project, encode_ordered_project, encode_storage_project,
    ObjectCodecErrorKind,
};
use serde_json::{json, Value};

#[test]
fn the_unmutated_golden_document_is_admissible() {
    admit(&golden_value()).unwrap();
    decode_storage_project(golden().as_bytes()).unwrap();
}

#[test]
fn every_single_mutation_is_refused_by_its_own_rule() {
    let all = cases();
    assert_eq!(all.len(), 37);
    for case in all {
        let mut value = golden_value();
        apply(&mut value, &case.pointer, &case.edit);
        let error = admit(&value).expect_err(case.label);
        assert_eq!(
            error.kind(),
            case.kind,
            "{}: {}",
            case.label,
            error.message()
        );
        assert!(
            error.message().contains(case.needle),
            "{}: {}",
            case.label,
            error.message()
        );
        // A refused document is not admitted by the storage entry either.
        assert!(
            decode_storage_project(value.to_string().as_bytes()).is_err(),
            "{}",
            case.label
        );
    }
}

#[test]
fn a_duplicate_component_symbol_is_refused() {
    let mut value = golden_value();
    let first = value["componentTables"]["components"]["components"][0].clone();
    value["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap()
        .push(first);
    let error = admit(&value).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::DuplicateId);
    assert!(error.message().contains("duplicate component symbolId"));
}

#[test]
fn a_component_without_an_inner_layer_is_refused() {
    let mut value = golden_value();
    value["componentTables"]["components"]["components"][0]["layers"] = json!([]);
    value["componentTables"]["components"]["components"][0]["objects"] = json!([]);
    value["componentTables"]["components"]["components"][0]["exposedProps"] = json!([]);
    let error = admit(&value).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Invalid);
    assert!(error.message().contains("1..4096 inner layers"));
}

#[test]
fn a_duplicator_may_sit_on_a_component_inner_layer() {
    // The same descriptor, moved to a layer of the component context, is admitted:
    // inner layers resolve in their own context, not in the scene list.
    let mut value = golden_value();
    value["componentTables"]["duplicators"]["duplicators"][0]["contextId"] = json!("sym_comp");
    value["componentTables"]["duplicators"]["duplicators"][0]["layerUid"] = json!("ly_comp_ia");
    admit(&value).unwrap();
    // ...but the scene layer name does not resolve in a component context.
    value["componentTables"]["duplicators"]["duplicators"][0]["layerUid"] = json!("ly_toggle");
    let error = admit(&value).unwrap_err();
    assert!(error.message().contains("not a layer of its context"));
}

#[test]
fn a_duplicator_or_effector_layer_may_also_be_a_component_instance() {
    // The baseline has no guard: toggleLayerDuplicator ignores `symbolId` and the
    // renderer applies the duplicator over a placed symbol's effective strokes, so
    // such a document is valid data and is carried unchanged.
    let mut value = golden_value();
    value["componentTables"]["duplicators"]["duplicators"][1]["layerUid"] = json!("ly_comp_loop");
    value["componentTables"]["duplicators"]["effectorLayers"][0]["layerUid"] =
        json!("ly_comp_once");
    admit(&value).unwrap();
    decode_storage_project(value.to_string().as_bytes()).unwrap();
}

#[test]
fn array_bounds_are_enforced_while_decoding() {
    let mut value = golden_value();
    let first = value["componentTables"]["components"]["instances"][0].clone();
    value["componentTables"]["components"]["instances"] = Value::Array(vec![first; 4097]);
    let error = admit(&value).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Invalid);
    assert!(error.message().contains("too many component entries"));
}

#[test]
fn duplicate_json_members_in_the_tables_are_refused() {
    let board = r#""storyboard":{"format":"nemo.native-storyboard","formatVersion":1,"nextId":1,"modules":[]}"#;
    let text = golden().replacen(
        "\"componentTables\":{",
        &format!("\"componentTables\":{{{board},{board},"),
        1,
    );
    let error = decode_ordered_project(text.as_bytes()).unwrap_err();
    assert!(error.message().contains("duplicate field"));
    assert!(decode_storage_project(text.as_bytes()).is_err());
}

#[test]
fn encode_time_admission_refuses_what_decode_would() {
    let broken = without_components(document());
    let error = encode_storage_project(&broken).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Invalid);
    assert!(error.message().contains("names no component"));
    let ordered = decode_ordered_project(golden().as_bytes()).unwrap();
    assert!(encode_ordered_project(&ordered).is_ok());
}

#[test]
fn a_v1_document_that_holds_the_member_is_refused_at_storage_validation() {
    let mut v1 = decode_storage_project(super::tests::V1.as_bytes()).unwrap();
    v1.component_tables = Presence::Present(tables());
    let error = encode_storage_project(&v1).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Unsupported);
    let text = with_member(super::tests::V1, "\"componentTables\":{}");
    assert!(decode_storage_project(text.as_bytes()).is_err());
}

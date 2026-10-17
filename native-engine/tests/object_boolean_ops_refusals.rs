//! L02-I transaction controls: strict payloads, exact receipts, retained request replay, owner pins on sealed
//! candidates, redo kept through a refusal, and every typed refusal leaving state, history and bytes alone.
use super::object_boolean_ops_tests::{
    apply, bytes, dispatch, document, four_apart, frame_records, group_of, ids_of, legacy, ngon,
    op_payload, owner_of, prepare, request, send, square, state, vector, VectorSpec, BLUE, RED,
};
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

type Frozen = (Vec<u8>, (usize, usize), u64);

fn frozen(owner: &NativeObjectHistory) -> Frozen {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}
fn unite_pair() -> Value {
    op_payload("unite", "L0", 0, &["s1", "s2"])
}
fn raw_op(owner: &NativeObjectHistory, text: &str) -> Result<bool, DispatchErrorCode> {
    owner
        .prepare_boolean_op_json(text.as_bytes())
        .map(|c| c.changed())
}

#[test]
fn payloads_are_strict_object_only_and_bounded() {
    let owner = owner_of(&four_apart(1));
    let ok = unite_pair().to_string();
    assert_eq!(raw_op(&owner, &ok), Ok(true));
    let refused = [
        // duplicate member, unknown member, positional array, wrong types
        ok.replacen(
            "\"mode\":\"unite\"",
            "\"mode\":\"unite\",\"mode\":\"unite\"",
            1,
        ),
        ok.replacen("}", ",\"extra\":1}", 1),
        "[\"object.boolean.apply\",\"unite\",\"L0\",\"scene-root\",0,[\"s1\",\"s2\"]]".to_string(),
        ok.replacen("\"frame\":0", "\"frame\":\"0\"", 1),
        ok.replacen("\"frame\":0", "\"frame\":-1", 1),
        ok.replacen("\"frame\":0", "\"frame\":0.5", 1),
        // wrong command, unknown or empty mode, too few / empty / oversized selection
        ok.replacen("object.boolean.apply", "object.boolean.erase", 1),
        ok.replacen("\"unite\"", "\"union\"", 1),
        ok.replacen("\"unite\"", "\"\"", 1),
        op_payload("unite", "L0", 0, &["s1"]).to_string(),
        op_payload("unite", "L0", 0, &[]).to_string(),
        op_payload("unite", "L0", 0, &["s1", ""]).to_string(),
        op_payload("unite", "", 0, &["s1", "s2"]).to_string(),
        op_payload("unite", "L0", 0, &vec!["s1"; 2000]).to_string(),
        "{}".to_string(),
        "not json".to_string(),
    ];
    for text in refused {
        assert_eq!(
            raw_op(&owner, &text),
            Err(DispatchErrorCode::InvalidRequest),
            "{text}"
        );
    }
    // the same bound as the group payloads: 64 KiB for the whole payload
    let long = format!("{}{}", " ".repeat(70_000), ok);
    assert_eq!(
        raw_op(&owner, &long),
        Err(DispatchErrorCode::InvalidRequest)
    );
}

#[test]
fn each_operation_is_dispatched_with_the_exact_compact_receipt() {
    let mut owner = owner_of(&four_apart(2));
    let response = send(&mut owner, "a", "apply", &unite_pair());
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(response["contentRevision"], 1);
    let erase = super::object_boolean_ops_tests::erase_payload(
        "L0",
        0,
        "s3",
        &[(95.0, -5.0), (105.0, -5.0), (105.0, 15.0), (95.0, 15.0)],
    );
    let response = send(&mut owner, "e", "erase", &erase);
    assert_eq!(
        response["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(response["contentRevision"], 2);
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn a_replayed_request_executes_once_and_returns_the_retained_reply() {
    let mut owner = owner_of(&four_apart(1));
    let req = request(&owner, "same", "apply", &unite_pair());
    let first = dispatch(&mut owner, &req);
    let after_first = frozen(&owner);
    let second = dispatch(&mut owner, &req);
    assert_eq!(first, second);
    assert_eq!(
        frozen(&owner),
        after_first,
        "the replay did not execute twice"
    );
    assert_eq!(owner.history_depths(), (1, 0));
    // the same request id with another payload is refused, not replayed
    let mut altered = req.clone();
    altered["payload"]["mode"] = json!("exclude");
    let refused = dispatch(&mut owner, &altered);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(frozen(&owner), after_first);
}

#[test]
fn a_stale_expected_revision_is_refused_and_redo_survives() {
    let mut owner = owner_of(&four_apart(1));
    apply(&mut owner, "apply", &unite_pair()).unwrap();
    let head = owner.content_revision();
    owner.undo(head).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    let mut stale = request(
        &owner,
        "late",
        "apply",
        &op_payload("unite", "L0", 0, &["s3", "s4"]),
    );
    stale["expectedRevision"] = json!(0);
    let before = frozen(&owner);
    let response = dispatch(&mut owner, &stale);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "stale_revision");
    assert_eq!(frozen(&owner), before);
    assert_eq!(owner.history_depths(), (0, 1), "redo kept");
    // a refusal after the undo keeps redo too
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s3", "nope"])
        ),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn a_sealed_candidate_commits_only_against_its_own_origin() {
    let mut owner = owner_of(&four_apart(1));
    let early = prepare(&owner, "apply", &unite_pair()).unwrap();
    let rival = prepare(
        &owner,
        "apply",
        &op_payload("unite", "L0", 0, &["s3", "s4"]),
    )
    .unwrap();
    owner.commit_boolean_edit(rival).unwrap();
    let before = frozen(&owner);
    assert_eq!(
        owner.commit_boolean_edit(early),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(frozen(&owner), before);
    // another owner (another instance) cannot take a candidate prepared elsewhere
    let mut other = NativeObjectHistory::new(
        "other",
        decode_storage_project(four_apart(1).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let foreign = prepare(
        &owner,
        "apply",
        &op_payload("unite", "L0", 0, &["s1", "s2"]),
    )
    .unwrap();
    assert_eq!(
        other.commit_boolean_edit(foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(other.history_depths(), (0, 0));
}

#[test]
fn a_candidate_is_immutable_evidence_of_what_would_happen() {
    let owner = owner_of(&four_apart(1));
    let before = frozen(&owner);
    let candidate = prepare(&owner, "apply", &unite_pair()).unwrap();
    assert!(candidate.changed());
    assert_eq!(
        candidate.created(),
        &["s2".to_string(), "native-stroke:1:0".to_string()]
    );
    // s2 keeps its id as the first island, so only s1 left the frame for good
    assert_eq!(candidate.removed(), &["s1".to_string()]);
    assert_eq!(frozen(&owner), before, "preparation publishes nothing");
}

#[test]
fn refusals_are_typed_and_publish_nothing() {
    let curved = {
        let mut r = legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED);
        r["geometry"]["segments"][1]["handleOut"] = json!({"x":2.5,"y":0.0});
        r
    };
    let stroke_only = vector(
        "L0",
        "s2",
        0,
        &square(50.0, 0.0, 10.0),
        &VectorSpec {
            fill: None,
            stroke: Some(BLUE),
            ..VectorSpec::default()
        },
    );
    let doc = document(
        1,
        vec![
            curved,
            stroke_only,
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = frozen(&owner);
    // a curved ring needs the retained flattener; a stroke-only record needs the stroker
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s1", "s3"])
        ),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s2", "s3"])
        ),
        Err(DispatchErrorCode::Unavailable)
    );
    // unknown stroke, unknown layer, duplicate operand, frame out of range
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s3", "zz"])
        ),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L9", 0, &["s3", "s3"])
        ),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s3", "s3"])
        ),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 7, &["s3", "s3"])
        ),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn a_locked_layer_refuses_every_operation() {
    let mut value = four_apart(1);
    value["layers"][0]["locked"] = json!(true);
    let mut owner = owner_of(&value);
    let before = frozen(&owner);
    assert_eq!(
        apply(&mut owner, "apply", &unite_pair()),
        Err(DispatchErrorCode::Unavailable)
    );
    let bite = [(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)];
    assert_eq!(
        apply(
            &mut owner,
            "erase",
            &super::object_boolean_ops_tests::erase_payload("L0", 0, "s1", &bite)
        ),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn an_empty_result_is_refused_not_inserted_as_an_empty_path() {
    let doc = document(
        1,
        vec![
            legacy("L0", "small", 0, &square(10.0, 10.0, 10.0), RED),
            legacy("L0", "big", 0, &square(0.0, 0.0, 100.0), BLUE),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = frozen(&owner);
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("subtract", "L0", 0, &["small", "big"])
        ),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("intersect", "L0", 0, &["small", "big"])
        ),
        Ok(true),
        "the same operands intersect to the small square"
    );
    assert_ne!(frozen(&owner), before);
}

#[test]
fn a_result_ring_beyond_the_storage_bound_is_refused() {
    // 150 + 150 anchors plus the slit: 302 > 256
    let doc = document(
        1,
        vec![
            legacy("L0", "outer", 0, &ngon(100.0, 100.0, 90.0, 150), RED),
            legacy("L0", "inner", 0, &ngon(100.0, 100.0, 40.0, 150), BLUE),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = frozen(&owner);
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("subtract", "L0", 0, &["outer", "inner"])
        ),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn a_motion_holder_blocks_removing_the_last_drawing_of_an_element() {
    let holders = json!([{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"s1"},
        "properties":[{"property":"position","static":[1.0, 2.0]}]}]);
    let mut value = four_apart(2);
    value["propertyHolders"] = holders;
    let mut owner = owner_of(&value);
    let before = frozen(&owner);
    // s1 is a plain operand: it would vanish while its holder still animates it
    assert_eq!(
        apply(&mut owner, "apply", &unite_pair()),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(frozen(&owner), before);
    // as the style source s1 keeps its id (the first island), so nothing is orphaned
    assert_eq!(
        apply(
            &mut owner,
            "apply",
            &op_payload("unite", "L0", 0, &["s2", "s1"])
        ),
        Ok(true)
    );
    let ids = ids_of(&frame_records(&state(&owner), "L0", 0));
    assert!(ids.contains(&"s1".to_string()));
}

#[test]
fn consumed_strokes_leave_their_groups_in_the_same_revision() {
    let table = group_of("g", "unite", &["s2", "s3"]);
    let doc = document(
        1,
        vec![
            legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
        ],
        Some(table),
        None,
    );
    let mut owner = owner_of(&doc);
    let original = bytes(&owner);
    apply(
        &mut owner,
        "apply",
        &op_payload("unite", "L0", 0, &["s1", "s2"]),
    )
    .unwrap();
    let after = state(&owner);
    // s2 was consumed (and re-created as the style source): the pair group fell apart
    assert_eq!(after["layers"][0]["groups"]["entries"], json!([]));
    assert_eq!(after["layers"][0]["groups"]["members"], json!([]));
    let head = owner.content_revision();
    owner.undo(head).unwrap();
    assert_eq!(bytes(&owner), original);
}

#[test]
fn other_frames_and_layers_are_never_touched() {
    let doc = document(
        2,
        vec![
            legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s1", 1, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 1, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L1", "s1", 0, &square(0.0, 0.0, 10.0), RED),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    apply(&mut owner, "apply", &unite_pair()).unwrap();
    let after = state(&owner);
    assert_eq!(
        frame_records(&after, "L0", 1),
        frame_records(&before, "L0", 1)
    );
    assert_eq!(
        frame_records(&after, "L1", 0),
        frame_records(&before, "L1", 0)
    );
    assert_eq!(
        ids_of(&frame_records(&after, "L0", 0)),
        ["s2", "native-stroke:1:0"]
    );
}

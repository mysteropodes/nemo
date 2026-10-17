//! L02-F refusal controls: malformed raw payloads, envelope refusals, absent
//! targets and the delete-last-layer rule. Every refusal leaves the document
//! bytes, the revision and BOTH history stacks exactly as they were; the owner
//! is primed with a redo entry so a leaked clear would be visible.
use super::object_layer_lifecycle_tests::{
    apply, base, bytes, delete, dispatch, duplicate, owner_of, rename, reorder, request, selection,
    send, value,
};
use super::*;
use serde_json::{json, Value};

fn primed() -> NativeObjectHistory {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "duplicate", &duplicate("L0"));
    owner.undo(1).unwrap();
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (0, 1))
    );
    owner
}
fn frozen(owner: &NativeObjectHistory) -> (Vec<u8>, (usize, usize), u64) {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}
fn prepare(owner: &NativeObjectHistory, op: &str, raw: &[u8]) -> Result<(), DispatchErrorCode> {
    match op {
        "delete" => owner.prepare_layer_delete_json(raw).map(drop),
        "duplicate" => owner.prepare_layer_duplicate_json(raw).map(drop),
        "rename" => owner.prepare_layer_rename_json(raw).map(drop),
        _ => owner.prepare_layer_reorder_json(raw).map(drop),
    }
}

const SEL: &str = r#"{"active":"L0","selected":[],"anchor":null}"#;

#[test]
fn malformed_raw_payloads_are_invalid_requests_and_change_nothing() {
    let delete_dup_nested = r#"{"command":"object.layer.delete","selection":{"active":"L0","selected":[],"active":"L1"}}"#;
    let cases: Vec<(&str, String)> = vec![
        ("delete", delete_dup_nested.to_string()),
        ("delete", format!(r#"{{"command":"object.layer.delete","command":"object.layer.delete","selection":{SEL}}}"#)),
        ("delete", format!(r#"{{"command":"object.layer.delete","command":"object.layer.delete","selection":{SEL}}}"#)),
        ("delete", format!(r#"{{"command":"object.layer.delete","selection":{SEL},"extra":1}}"#)),
        ("delete", format!(r#"{{"command":"object.layer.duplicate","selection":{SEL}}}"#)),
        ("delete", r#"["object.layer.delete"]"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":null}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete"}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":{"active":"L0","selected":"L0"}}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":["L0",[],null]}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":{"active":"","selected":[]}}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":{"active":"L0","selected":[""]}}"#.to_string()),
        ("delete", r#"{"command":"object.layer.delete","selection":{"active":"L0","selected":[],"anchor":""}}"#.to_string()),
        ("duplicate", r#"{"command":"object.layer.duplicate","layerUid":"L0","layerUid":"L1"}"#.to_string()),
        ("duplicate", r#"{"command":"object.layer.duplicate"}"#.to_string()),
        ("duplicate", r#"{"command":"object.layer.duplicate","layerUid":7}"#.to_string()),
        ("duplicate", r#"{"command":"object.layer.duplicate","layerUid":""}"#.to_string()),
        ("duplicate", r#"{"command":"object.layer.duplicate","layerUid":"L0","name":"x"}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"L0","name":"A","name":"B"}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"L0","name":"A","name":"B"}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"L0","name":null}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"L0","name":0}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"L0"}"#.to_string()),
        ("rename", r#"{"command":"object.layer.rename","layerUid":"","name":"A"}"#.to_string()),
        ("rename", r#"{"command":"object.layer.create","layerUid":"L0","name":"A"}"#.to_string()),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":["L0"],"gap":"1","selection":{SEL}}}"#)),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":["L0"],"gap":null,"selection":{SEL}}}"#)),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":[],"gap":1,"selection":{SEL}}}"#)),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":["L0","L0"],"gap":1,"selection":{SEL}}}"#)),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":[""],"gap":1,"selection":{SEL}}}"#)),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":"L0","gap":1,"selection":{SEL}}}"#)),
        ("reorder", r#"{"command":"object.layer.reorder","layerUids":["L0"],"gap":1}"#.to_string()),
        ("reorder", format!(r#"{{"command":"object.layer.reorder","layerUids":["L0"],"gap":1,"selection":{SEL},"extra":true}}"#)),
    ];
    let owner = primed();
    let before = frozen(&owner);
    for (op, raw) in &cases {
        assert_eq!(
            prepare(&owner, op, raw.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest),
            "{op}: {raw}"
        );
        assert_eq!(frozen(&owner), before, "{raw}");
    }
    let mut padded = rename("L0", "Z").to_string();
    padded.push_str(&" ".repeat(4096 - padded.len()));
    assert_eq!(prepare(&owner, "rename", padded.as_bytes()), Ok(()));
    padded.push(' ');
    assert_eq!(
        prepare(&owner, "rename", padded.as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn envelope_and_target_refusals_keep_state_and_both_stacks() {
    let ops = [
        ("delete", delete("L1", &["L1"]), delete("nope", &[])),
        ("duplicate", duplicate("L1"), duplicate("nope")),
        ("rename", rename("L1", "x"), rename("nope", "x")),
        (
            "reorder",
            reorder(&["L1"], 0.0, selection("L2", &[], None)),
            reorder(&["nope"], 0.0, selection("L2", &[], None)),
        ),
    ];
    for (op, good, absent) in ops {
        let mut owner = primed();
        let before = frozen(&owner);
        let envelope = [
            (
                "cancelledBeforeDispatch",
                json!(true),
                "cancelled_before_dispatch",
            ),
            ("expectedRevision", json!(99), "stale_revision"),
            ("instanceId", json!("foreign"), "wrong_instance"),
            ("documentId", json!("foreign"), "wrong_document"),
            ("apiVersion", json!(1), "invalid_request"),
        ];
        for (n, (key, patch, code)) in envelope.into_iter().enumerate() {
            let mut req = request(&owner, &format!("{op}-env-{n}"), op, &good);
            req[key] = patch;
            let reply = dispatch(&mut owner, &req);
            assert_eq!(
                (reply["ok"].clone(), reply["error"]["code"].clone()),
                (json!(false), json!(code)),
                "{op} {key}"
            );
            assert_eq!(frozen(&owner), before, "{op} {key}");
        }
        let mut req = request(&owner, &format!("{op}-no-revision"), op, &good);
        req.as_object_mut().unwrap().remove("expectedRevision");
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["error"]["code"], "invalid_request", "{op}");
        let reply = send(&mut owner, &format!("{op}-absent"), op, &absent);
        assert_eq!(reply["error"]["code"], "not_found", "{op} absent target");
        assert_eq!(frozen(&owner), before, "{op}");
        let mut shaped = good.clone();
        shaped["extra"] = json!(1);
        let reply = send(&mut owner, &format!("{op}-shape"), op, &shaped);
        assert_eq!(
            reply["error"]["code"], "invalid_request",
            "{op} unknown member"
        );
        assert_eq!(frozen(&owner), before, "{op}");
    }
}

#[test]
fn a_one_layer_document_refuses_delete_before_reading_the_selection() {
    let mut single = base(1);
    single["layers"] = json!([single["layers"][0].clone()]);
    single["objects"] = json!([single["objects"][0].clone()]);
    single["propertyHolders"] = json!([
        single["propertyHolders"][0].clone(),
        single["propertyHolders"][1].clone()
    ]);
    let mut owner = owner_of(&single);
    let before = frozen(&owner);
    let payloads = [
        delete("L0", &["L0"]),
        delete("L0", &[]),
        delete("nope", &["also-nope"]),
    ];
    for (n, payload) in payloads.into_iter().enumerate() {
        let reply = send(&mut owner, &format!("last-{n}"), "delete", &payload);
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["error"]["code"], "invalid_request", "{payload}");
        assert_eq!(frozen(&owner), before);
    }
    assert_eq!(value(&owner)["layers"].as_array().unwrap().len(), 1);
}

#[test]
fn delete_names_the_missing_layer_when_the_document_has_several() {
    let mut owner = primed();
    let before = frozen(&owner);
    for (n, payload) in [
        delete("nope", &[]),
        delete("L0", &["L1", "nope"]),
        delete("L0", &["nope"]),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = send(&mut owner, &format!("missing-{n}"), "delete", &payload);
        assert_eq!(reply["error"]["code"], "not_found", "{payload}");
        assert_eq!(frozen(&owner), before);
    }
}

#[test]
fn reorder_gap_rounds_like_math_round_and_clamps_to_the_ends() {
    let names = |owner: &NativeObjectHistory| -> Vec<String> {
        value(owner)["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["layerUid"].as_str().unwrap().to_owned())
            .collect()
    };
    let pick = || selection("L0", &[], None);
    let cases: [(&[&str], f64, [&str; 3]); 6] = [
        (&["L1"], 0.49999999999999994, ["L1", "L0", "L2"]),
        (&["L2"], -0.5, ["L2", "L0", "L1"]),
        (&["L0"], 1e308, ["L1", "L2", "L0"]),
        (&["L2"], -1e308, ["L2", "L0", "L1"]),
        (&["L0"], 2.5, ["L1", "L2", "L0"]),
        (&["L2"], 0.5, ["L0", "L2", "L1"]),
    ];
    for (moving, gap, want) in cases {
        let mut owner = owner_of(&base(2));
        apply(&mut owner, "reorder", &reorder(moving, gap, pick()));
        assert_eq!(names(&owner), want, "{moving:?} {gap}");
    }
}

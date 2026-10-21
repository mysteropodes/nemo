//! Negative controls for original mask metadata admission and the existing owner seal.
use crate::commands::object_fill_owner::vector_edit::mask_edit::tests::{
    dispatch, fixture, footprint, head, make_owner, make_request, payload, send, state, target,
};
use crate::object_codec::decode_storage_project;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[test]
fn mask_storage_rejects_wrong_types_duplicate_tags_and_unknown_fields() {
    for ext in [
        json!({"isMask":1}),
        json!({"isMask":"true"}),
        json!({"maskMode":false}),
        json!({"maskFeather":"3"}),
        json!({"maskFeather":{}}),
        json!({"isMask":true,"unowned":1}),
    ] {
        let mut document = fixture(1);
        document["objects"][0]["vectorExt"] = ext;
        assert!(decode_storage_project(document.to_string().as_bytes()).is_err());
    }
    let wire = fixture(1).to_string();
    let duplicate = wire.replacen("\"isMask\":true", "\"isMask\":true,\"isMask\":false", 1);
    assert_ne!(duplicate, wire);
    assert!(decode_storage_project(duplicate.as_bytes()).is_err());
    let nonfinite = wire.replacen("\"maskFeather\":3", "\"maskFeather\":1e999", 1);
    assert_ne!(nonfinite, wire);
    assert!(decode_storage_project(nonfinite.as_bytes()).is_err());
}

#[test]
fn invalid_or_cross_purpose_panel_intents_cannot_publish_partial_shared_feather() {
    let owner = make_owner("mask-invalid", &fixture(2));
    let before = footprint(&owner);
    let valid = payload(json!({"kind":"feather","value":4})).to_string();
    let invalid = [
        payload(json!({"kind":"mode","mode":"unknown"})).to_string(),
        payload(json!({"kind":"feather","value":null})).to_string(),
        payload(json!({"kind":"feather","value":"4"})).to_string(),
        payload(json!({"kind":"unset","value":4})).to_string(),
        payload(json!({"kind":"feather","value":4,"mode":"add"})).to_string(),
        payload(json!({"kind":"unknown"})).to_string(),
        valid.replacen("\"value\":4", "\"value\":4,\"value\":5", 1),
        valid.replacen("\"maskEdit\":", "\"maskEdit\":null,\"maskEdit\":", 1),
        valid.replacen("object.vector.mask.edit", "object.vector.delete", 1),
        " ".repeat(65_537),
    ];
    for text in invalid {
        assert_eq!(
            owner.prepare_vector_edit_json(text.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        assert_eq!(footprint(&owner), before);
    }
}

#[test]
fn detached_mask_candidates_refuse_foreign_instance_document_and_stale_revision_atomically() {
    let source = fixture(2);
    let mut owner = make_owner("mask-seal", &source);
    let prepare = || {
        owner
            .prepare_vector_edit_json(
                payload(json!({"kind":"feather","value":8}))
                    .to_string()
                    .as_bytes(),
            )
            .unwrap()
    };
    let foreign = prepare();
    let mut other = make_owner("other-instance", &source);
    let before = footprint(&other);
    assert_eq!(
        other.commit_vector_edit(foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(footprint(&other), before);
    let foreign_document = prepare();
    let mut other = make_owner("mask-seal", &source);
    let before = footprint(&other);
    assert_eq!(
        other.commit_vector_edit(foreign_document),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(footprint(&other), before);
    let stale = prepare();
    let origin = head(&owner);
    assert_eq!(
        send(
            &mut owner,
            "advance",
            json!({"kind":"mode","mode":"intersect"})
        )["ok"],
        true
    );
    let before = footprint(&owner);
    assert_eq!(
        owner.commit_vector_edit(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(footprint(&owner), before);
    assert_eq!(serde_json::to_value(origin.document()).unwrap(), source);
}

#[test]
fn actual_dispatch_cancel_owner_and_revision_refusals_preserve_bytes_history_and_redo() {
    let mut owner = make_owner("mask-dispatch", &fixture(1));
    assert_eq!(
        send(&mut owner, "seed", json!({"kind":"feather","value":4}))["ok"],
        true
    );
    owner.undo(1).unwrap();
    let before = footprint(&owner);
    assert_eq!(owner.history_depths(), (0, 1));
    let base = make_request(
        &owner,
        "refused",
        payload(json!({"kind":"feather","value":6})),
    );
    for (id, field, value, code) in [
        (
            "cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("instance", "instanceId", json!("other"), "wrong_instance"),
        (
            "document",
            "documentId",
            json!("other-doc"),
            "wrong_document",
        ),
        ("stale", "expectedRevision", json!(999), "stale_revision"),
    ] {
        let mut request = base.clone();
        request["requestId"] = json!(id);
        request[field] = value;
        let response = dispatch(&mut owner, request);
        assert_eq!(response["error"]["code"], code);
        assert_eq!(footprint(&owner), before);
    }
}

#[test]
fn shared_mask_edit_keeps_effective_drawing_lock_nonmask_unknown_and_reference_guards() {
    for (stroke, code) in [
        ("ink", DispatchErrorCode::Unavailable),
        ("missing", DispatchErrorCode::NotFound),
    ] {
        let owner = make_owner("mask-target", &fixture(1));
        let before = footprint(&owner);
        let mut request = payload(json!({"kind":"feather","value":6}));
        request["stableTarget"] = target("L0", 0, stroke);
        assert_eq!(
            owner
                .prepare_vector_edit_json(request.to_string().as_bytes())
                .unwrap_err(),
            code
        );
        assert_eq!(footprint(&owner), before);
    }
    for reference in [false, true] {
        let mut source = fixture(1);
        let mut request = payload(json!({"kind":"feather","value":6}));
        if reference {
            source["objects"][0]["target"]["frameScope"]["kind"] = json!("reference");
            request["stableTarget"]["frameScope"]["kind"] = json!("reference");
        } else {
            source["layers"][0]["locked"] = json!(true);
        }
        let owner = make_owner("mask-locked", &source);
        let before = footprint(&owner);
        assert_eq!(
            owner
                .prepare_vector_edit_json(request.to_string().as_bytes())
                .unwrap_err(),
            DispatchErrorCode::Unavailable
        );
        assert_eq!(footprint(&owner), before);
        assert_eq!(state(&owner), source);
    }
}

#[test]
fn actual_mask_dispatch_replay_is_once_and_changed_request_id_conflicts_are_atomic() {
    let mut owner = make_owner("mask-receipt", &fixture(2));
    let request = make_request(&owner, "one", payload(json!({"kind":"feather","value":6})));
    let response = dispatch(&mut owner, request.clone());
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["historyEntriesAdded"], 1);
    let before = footprint(&owner);
    let replay = dispatch(&mut owner, request.clone());
    assert_eq!(replay["ok"], true);
    assert_eq!(replay["result"], response["result"]);
    assert_eq!(footprint(&owner), before);
    let mut changed: Value = request;
    changed["payload"]["maskEdit"]["value"] = json!(9);
    let conflict = dispatch(&mut owner, changed);
    assert_eq!(conflict["error"]["code"], "invalid_request");
    assert_eq!(footprint(&owner), before);
}

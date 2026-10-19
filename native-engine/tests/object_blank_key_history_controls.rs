//! Public F7 correlation/replay/refusal controls exercise the existing dispatcher.
use super::*;

#[test]
fn cancelled_blank_and_its_retained_error_never_add_history_after_other_success() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;.;K|K;.;.;.",
        build::Build::V2,
    ));
    let mut cancelled = blank(&o, "cancel-blank", "L0", 1);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let before = state(&o);
    let refusal = dispatch(&mut o, &cancelled);
    assert_eq!(refusal["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(state(&o), before);
    let q = blank(&o, "other-blank", "L0", 2);
    success(&dispatch(&mut o, &q));
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &cancelled), refusal);
    assert_eq!(state(&o), after);
}

#[test]
fn stale_and_changed_body_blank_requests_preserve_document_history_and_replay() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;.;K|K;.;.;.",
        build::Build::V2Extras,
    ));
    let q = blank(&o, "first", "L0", 1);
    let mut stale = blank(&o, "stale", "L0", 2);
    let reply = dispatch(&mut o, &q);
    success(&reply);
    let after = state(&o);
    let failure = dispatch(&mut o, &stale);
    assert_eq!(failure["error"]["code"], "stale_revision");
    assert_eq!(state(&o), after);
    let mut changed = q.clone();
    changed["payload"]["cell"] = json!(2);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), after);
    stale["requestId"] = json!("fresh-id-old-revision");
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    assert_eq!(state(&o), after);
}

#[test]
fn wrong_owner_missing_revision_and_mismatched_command_are_atomic_public_refusals() {
    for kind in [
        "instance", "document", "revision", "command", "field", "layer", "frame",
    ] {
        let mut o = owner(fixture::document(
            "K=a.1.10;.;.;K|K;.;.;.",
            build::Build::V1,
        ));
        let mut q = blank(&o, "refuse", "L0", 1);
        let before = state(&o);
        let expected = match kind {
            "instance" => {
                q["instanceId"] = json!("wrong-instance");
                "wrong_instance"
            }
            "document" => {
                q["documentId"] = json!("wrong-document");
                "wrong_document"
            }
            "revision" => {
                q.as_object_mut().unwrap().remove("expectedRevision");
                "invalid_request"
            }
            "command" => {
                q["payload"]["command"] = json!("object.layer.key.clear");
                "invalid_request"
            }
            "field" => {
                q["payload"]["layerUids"] = json!(["L1"]);
                "invalid_request"
            }
            "layer" => {
                q["payload"]["layerUid"] = json!("missing-layer");
                "not_found"
            }
            _ => {
                q["payload"]["cell"] = json!(4);
                "invalid_request"
            }
        };
        assert_eq!(dispatch(&mut o, &q)["error"]["code"], expected, "{kind}");
        assert_eq!(state(&o), before);
    }
}

#[test]
fn public_blank_keeps_the_exact_4096_byte_request_boundary() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;.;K|K;.;.;.",
        build::Build::V1,
    ));
    let mut q = blank(&o, "bound-a", "missing", 1);
    let size = serde_json::to_vec(&q).unwrap().len();
    q["payload"]["layerUid"] = json!(format!("missing{}", "x".repeat(4096 - size)));
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&typed).unwrap().len(), 4096);
    let before = state(&o);
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "not_found");
    assert_eq!(state(&o), before);
    q["requestId"] = json!("bound-b");
    let layer = q["payload"]["layerUid"].as_str().unwrap().to_owned();
    q["payload"]["layerUid"] = json!(layer + "x");
    let typed: OpacityRequest = serde_json::from_value(q.clone()).unwrap();
    assert_eq!(serde_json::to_vec(&typed).unwrap().len(), 4097);
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
}

#[test]
fn prepared_blank_history_staleness_never_adds_a_second_undo_entry() {
    let mut o = owner(fixture::document(
        "K=a.1.10;.;.;K|K;.;.;.",
        build::Build::V1Ref,
    ));
    let candidate = o
        .prepare_blank_key_json(&fixture::payload("L0", 1))
        .unwrap();
    let q = blank(&o, "first", "L0", 2);
    success(&dispatch(&mut o, &q));
    let before = state(&o);
    assert_eq!(
        o.commit_blank_key(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&o), before);
}

#[test]
fn public_locked_and_orphan_holder_refusals_keep_history_and_retained_source_unchanged() {
    for kind in ["locked", "orphan"] {
        let mut input = fixture::document("K=a.1.10;.;.;K=b.2.20|K;.;.;.", build::Build::V2Extras);
        if kind == "locked" {
            input.layers[0].locked = crate::object_document::Presence::Present(true);
        } else {
            input.property_holders = crate::object_document::Presence::Present(
                serde_json::from_value(json!([
                    {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},
                        "properties":[{"property":"rotation","static":[17]}]}
                ]))
                .unwrap(),
            );
        }
        let mut o = owner(input);
        let before = state(&o);
        let retained = o.acquire_snapshot(0).unwrap();
        let q = blank(&o, "preserve", "L0", 0);
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["error"]["code"], "unavailable");
        assert_eq!(state(&o), before);
        assert_eq!(dispatch(&mut o, &q), reply);
        assert_eq!(state(&o), before);
        assert_eq!(
            encode_storage_project(retained.document()).unwrap(),
            before.2
        );
    }
}

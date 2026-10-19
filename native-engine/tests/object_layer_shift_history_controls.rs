//! Refusal must preserve the same complete native owner, history and retained snapshots.
use super::*;
#[test]
fn cancellation_stale_revision_changed_body_and_replay_never_repeat_or_partially_publish() {
    let mut o = owner(fixture::document());
    let mut cancelled = request(&o, "cancel", fixture::gesture("both", &[2]));
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let stale = request(&o, "stale", fixture::gesture("both", &[1]));
    let before = state(&o);
    let refusal = dispatch(&mut o, &cancelled);
    assert_eq!(refusal["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(state(&o), before);
    let q = request(&o, "accepted", fixture::gesture("both", &[2]));
    let reply = dispatch(&mut o, &q);
    success(&reply, &q, 1);
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &cancelled), refusal);
    assert_eq!(state(&o), after);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    assert_eq!(state(&o), after);
    let mut changed = q.clone();
    changed["payload"]["samples"] = json!([1]);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), after);
}
#[test]
fn wrong_owner_missing_revision_closed_payload_and_context_refusals_are_atomic() {
    for kind in [
        "instance",
        "document",
        "revision",
        "command",
        "extra",
        "context",
        "active",
        "part",
        "sample",
        "selection",
    ] {
        let mut o = owner(fixture::document());
        let before = state(&o);
        let mut q = request(&o, "bad", fixture::gesture("both", &[2]));
        let expected = match kind {
            "instance" => {
                q["instanceId"] = json!("foreign");
                "wrong_instance"
            }
            "document" => {
                q["documentId"] = json!("foreign");
                "wrong_document"
            }
            "revision" => {
                q.as_object_mut().unwrap().remove("expectedRevision");
                "invalid_request"
            }
            "command" => {
                q["payload"]["command"] = json!("object.layer.frames.shift");
                "invalid_request"
            }
            "extra" => {
                q["payload"]["delta"] = json!(2);
                "invalid_request"
            }
            "context" => {
                q["payload"]["contextId"] = json!("nested");
                "unavailable"
            }
            "active" => {
                q["payload"]["activeLayerUid"] = json!("missing");
                "not_found"
            }
            "part" => {
                q["payload"]["part"] = json!("unknown");
                "invalid_request"
            }
            "sample" => {
                q["payload"]["samples"] = json!([0.5]);
                "invalid_request"
            }
            _ => {
                q["payload"]["barSelection"] =
                    json!([{"layerUid":"L0","part":"in"},{"layerUid":"L0","part":"out"}]);
                "invalid_request"
            }
        };
        assert_eq!(dispatch(&mut o, &q)["error"]["code"], expected, "{kind}");
        assert_eq!(state(&o), before);
    }
}
#[test]
fn oversized_ordered_gesture_and_complete_correlated_reply_refuse_before_checkpoint() {
    let mut o = owner(fixture::document());
    let before = state(&o);
    let mut payload = fixture::gesture("both", &[]);
    payload["samples"] = json!(vec![0; 2048]);
    let q = request(&o, "long-gesture", payload);
    assert!(serde_json::to_vec(&q).unwrap().len() > 4096);
    assert_eq!(dispatch(&mut o, &q)["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
    // Exercise the actual dispatch body's independent complete-reply guard as well as public ingress.
    let mut q = request(&o, "correlated", fixture::gesture("both", &[2]));
    q["requestId"] = json!("x".repeat(4096));
    let typed: OpacityRequest = serde_json::from_value(q).unwrap();
    assert!(o.dispatch_layer_shift(&typed).is_err());
    assert_eq!(state(&o), before);
}
#[test]
fn stale_or_foreign_sealed_candidates_cannot_add_an_undo_entry() {
    let input = fixture::document();
    let mut o = owner(input.clone());
    let payload = fixture::gesture("both", &[2]).to_string().into_bytes();
    let stale = o.prepare_layer_range_drag_json(&payload).unwrap();
    let mut foreign = owner(input);
    let foreign_candidate = foreign.prepare_layer_range_drag_json(&payload).unwrap();
    let before = state(&o);
    assert_eq!(
        o.commit_layer_range_drag(foreign_candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(state(&o), before);
    let q = request(&o, "one", fixture::gesture("both", &[1]));
    success(&dispatch(&mut o, &q), &q, 1);
    let after = state(&o);
    assert_eq!(
        o.commit_layer_range_drag(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&o), after);
    let before = state(&foreign);
    let q = request(&foreign, "other", fixture::gesture("both", &[1]));
    success(&dispatch(&mut foreign, &q), &q, 1);
    assert_ne!(state(&foreign), before);
}

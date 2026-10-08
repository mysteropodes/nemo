//! Independent common-application object command/readback oracle.
use crate::{application_mcp::ApplicationMcp, native_object_bootstrap::bootstrap_raw};
use serde_json::{json, Value};

pub(super) fn document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}
pub(super) fn install(state: &ApplicationMcp) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,
        "requestId":"install-client", "instanceId":state.instance_id(),
        "documentJson":document().to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn request(
    receipt: &Value,
    id: &str,
    revision: u64,
    operation: &str,
    payload: Value,
) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"expectedRevision":revision,
        "operation":operation,"payload":payload})
}
pub(super) fn fill(receipt: &Value, id: &str, revision: u64) -> Value {
    request(
        receipt,
        id,
        revision,
        "command.document.object.fill.set",
        json!({"command":"object.fill.set","stableTarget":document()["objects"][0]["target"],
            "fill":{"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5}}),
    )
}
pub(super) fn history(receipt: &Value, id: &str, revision: u64, action: &str) -> Value {
    request(
        receipt,
        id,
        revision,
        &format!("command.document.object.{action}"),
        json!({"command":format!("object.{action}")}),
    )
}
pub(super) fn read(receipt: &Value, id: &str, revision: u64, index: usize) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":receipt["instanceId"],
        "documentId":receipt["documentId"],"operation":"query.document.object",
        "payload":{"atRevision":revision,"stableTarget":document()["objects"][index]["target"]}})
}
pub(super) fn dispatch(state: &ApplicationMcp, request: &Value) -> Value {
    serde_json::to_value(
        crate::application_mcp::object_client::dispatch_raw(state, &request.to_string())
            .expect("common application object client must reach installed owner"),
    )
    .unwrap()
}

#[test]
fn common_application_fill_undo_redo_preserves_full_objects_and_correlated_revisions() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let fill = fill(&receipt, "client-fill", 0);
    let undo = history(&receipt, "client-undo", 1, "undo");
    let redo = history(&receipt, "client-redo", 2, "redo");
    let mut edited = document()["objects"][0].clone();
    edited["fill"] = json!({"kind":"solid","r":0.9,"g":0.1,"b":0.3,"a":0.5});
    for (command, revision, expected) in [
        (&fill, 1, edited.clone()),
        (&undo, 2, document()["objects"][0].clone()),
        (&redo, 3, edited),
    ] {
        let result = dispatch(&state, command);
        assert_eq!(result["ok"], true);
        assert_eq!(result["contentRevision"], revision);
        for key in ["apiVersion", "requestId", "instanceId", "documentId"] {
            assert_eq!(result[key], command[key]);
        }
        assert_eq!(
            dispatch(&state, &read(&receipt, "client-read", revision, 0))["result"]["object"],
            expected
        );
        assert_eq!(
            dispatch(&state, &read(&receipt, "client-sibling", revision, 1))["result"]["object"],
            document()["objects"][1]
        );
    }
}

#[test]
fn exact_receipts_survive_later_moves_and_changed_bodies_never_repeat_effects() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let f = fill(&receipt, "retained-fill", 0);
    let u = history(&receipt, "retained-undo", 1, "undo");
    let r = history(&receipt, "retained-redo", 2, "redo");
    let replies = [
        dispatch(&state, &f),
        dispatch(&state, &u),
        dispatch(&state, &r),
    ];
    let before = dispatch(&state, &read(&receipt, "before-replays", 3, 0));
    for (request, reply) in [&f, &u, &r].into_iter().zip(replies) {
        assert_eq!(dispatch(&state, request), reply);
        let mut changed = request.clone();
        changed["payload"]["extra"] = json!(true);
        assert_eq!(
            dispatch(&state, &changed)["error"]["code"],
            "invalid_request"
        );
        let mut cross = history(&receipt, "cross", 3, "undo");
        cross["requestId"] = request["requestId"].clone();
        assert_eq!(dispatch(&state, &cross)["error"]["code"], "invalid_request");
    }
    assert_eq!(
        dispatch(&state, &read(&receipt, "after-replays", 3, 0))["result"],
        before["result"]
    );
    assert_eq!(
        dispatch(&state, &read(&receipt, "old-pin", 0, 0))["error"]["code"],
        "not_found"
    );
}

#[test]
fn cancelled_missing_stale_and_malformed_failures_remain_exact_after_later_fill() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let mut cancelled = fill(&receipt, "cancelled", 0);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    let mut missing = fill(&receipt, "missing", 0);
    missing.as_object_mut().unwrap().remove("expectedRevision");
    let stale = fill(&receipt, "stale", 9);
    let malformed = request(
        &receipt,
        "malformed",
        0,
        "command.document.object.undo",
        json!({"command":"object.redo"}),
    );
    let requests = [cancelled, missing, stale, malformed];
    let codes = [
        "cancelled_before_dispatch",
        "invalid_request",
        "stale_revision",
        "invalid_request",
    ];
    let replies: Vec<_> = requests
        .iter()
        .zip(codes)
        .map(|(r, code)| {
            let response = dispatch(&state, r);
            assert_eq!(response["error"]["code"], code);
            assert_eq!(response["contentRevision"], 0);
            response
        })
        .collect();
    assert_eq!(dispatch(&state, &fill(&receipt, "fresh", 0))["ok"], true);
    for (r, reply) in requests.iter().zip(replies) {
        assert_eq!(dispatch(&state, r), reply);
    }
    let mut bad = fill(&receipt, "identity-safe", 1);
    bad["instanceId"] = json!("foreign");
    assert_eq!(dispatch(&state, &bad)["error"]["code"], "wrong_instance");
    bad["instanceId"] = receipt["instanceId"].clone();
    bad["documentId"] = json!("foreign");
    assert_eq!(dispatch(&state, &bad)["error"]["code"], "wrong_document");
    assert_eq!(
        dispatch(&state, &fill(&receipt, "identity-safe", 1))["result"]["applied"],
        false
    );
}

#[test]
fn no_op_preserves_redo_empty_failure_is_retained_and_fresh_fill_invalidates_redo() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let empty = history(&receipt, "empty", 0, "undo");
    let failure = dispatch(&state, &empty);
    assert_eq!(failure["error"]["code"], "unavailable");
    dispatch(&state, &fill(&receipt, "fill", 0));
    dispatch(&state, &history(&receipt, "undo", 1, "undo"));
    let mut noop = fill(&receipt, "noop", 2);
    noop["payload"]["fill"] = document()["objects"][0]["fill"].clone();
    let result = dispatch(&state, &noop);
    assert_eq!(result["contentRevision"], 2);
    assert_eq!(
        result["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        dispatch(&state, &history(&receipt, "redo", 2, "redo"))["contentRevision"],
        3
    );
    assert_eq!(dispatch(&state, &empty), failure);
    dispatch(&state, &history(&receipt, "undo-again", 3, "undo"));
    let mut fresh = fill(&receipt, "different", 4);
    fresh["payload"]["fill"]["r"] = json!(0.7);
    assert_eq!(dispatch(&state, &fresh)["contentRevision"], 5);
    assert_eq!(
        dispatch(&state, &history(&receipt, "no-redo", 5, "redo"))["error"]["code"],
        "unavailable"
    );
}

#[test]
fn common_pending_revision_barrier_blocks_raw_client_reexecution_and_other_writes() {
    use crate::application_mcp::object_client::admit_raw;
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let command = fill(&receipt, "external-object", 0);
    let delivery = state
        .revisions
        .dispatch_object(
            state.instance_id(),
            &state.native,
            admit_raw(&command.to_string()).unwrap(),
            true,
        )
        .unwrap();
    assert!(delivery.response.ok);
    assert_eq!(delivery.response.content_revision, 1);
    let retry = dispatch(&state, &command);
    assert_eq!(retry["error"]["details"]["committed"], true);
    assert_eq!(retry["error"]["details"]["retryExecution"], false);
    let fresh = dispatch(&state, &history(&receipt, "blocked-undo", 1, "undo"));
    assert_eq!(fresh["error"]["details"]["committed"], false);
    assert_eq!(
        dispatch(&state, &read(&receipt, "pending-read", 1, 0))["contentRevision"],
        1
    );
    let mut changed = command.clone();
    changed["payload"]["fill"]["r"] = json!(0.5);
    assert_eq!(
        dispatch(&state, &changed)["error"]["code"],
        "invalid_request"
    );
    state
        .control_revision_for_test(
            json!({"action":"disconnect","subscriptionId":binding["subscriptionId"]}),
        )
        .unwrap();
    assert_eq!(
        dispatch(&state, &history(&receipt, "still-blocked", 1, "undo"))["error"]["details"]
            ["retryExecution"],
        false
    );
}

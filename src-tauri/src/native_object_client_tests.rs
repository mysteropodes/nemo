//! Independent common-application object command/readback oracle.
#[path = "native_object_timing_tests.rs"]
mod timing_tests;
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
fn raw_visibility_uses_real_owner_history_replay_and_strict_original_bytes() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let visibility = |id: &str, revision, visible| {
        request(
            &receipt,
            id,
            revision,
            "command.document.object.layer.visibility.set",
            json!({"command":"object.layer.visibility.set","layerUid":"legacy layer:alpha","visible":visible}),
        )
    };
    let serialize = |revision| {
        let query = json!({"apiVersion":2,"requestId":format!("visible-read-{revision}"),
            "instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
            "operation":"query.document.object.serialize","payload":{"atRevision":revision}});
        dispatch(&state, &query)["result"]["documentJson"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let before = serialize(0);
    let hide = visibility("visible-hide", 0, false);
    let applied = dispatch(&state, &hide);
    assert_eq!(applied["ok"], true);
    assert_eq!(applied["contentRevision"], 1);
    let hidden = serialize(1);
    assert_eq!(
        serde_json::from_str::<Value>(&hidden).unwrap()["layers"][0]["visible"],
        false
    );
    let noop = dispatch(&state, &visibility("visible-noop", 1, false));
    assert_eq!(noop["result"]["applied"], false);
    assert_eq!(noop["contentRevision"], 1);
    assert_eq!(
        dispatch(&state, &history(&receipt, "visible-undo", 1, "undo"))["ok"],
        true
    );
    assert_eq!(serialize(2), before);
    let unchanged = dispatch(&state, &visibility("visible-redo-noop", 2, true));
    assert_eq!(unchanged["result"]["applied"], false);
    assert_eq!(
        dispatch(&state, &history(&receipt, "visible-redo", 2, "redo"))["ok"],
        true
    );
    assert_eq!(serialize(3), hidden);
    assert_eq!(dispatch(&state, &hide), applied);
    let mut changed = hide.clone();
    changed["payload"]["visible"] = json!(true);
    assert_eq!(
        dispatch(&state, &changed)["error"]["code"],
        "invalid_request"
    );
    let mut cancelled = visibility("visible-cancel", 3, true);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&state, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(
        dispatch(&state, &visibility("visible-stale", 0, true))["error"]["code"],
        "stale_revision"
    );
    let duplicate = visibility("visible-duplicate", 3, false)
        .to_string()
        .replace("\"visible\":false", "\"visible\":false,\"visible\":true");
    assert!(crate::application_mcp::object_client::dispatch_raw(&state, &duplicate).is_err());
    assert_eq!(serialize(3), hidden);
    let status = crate::application_mcp::object_client::client_status(&state).unwrap();
    assert_eq!(status["contentRevision"], 3);
    assert!(status["operations"]
        .as_array()
        .unwrap()
        .contains(&json!("command.document.object.layer.visibility.set")));
    assert_eq!(status["publicCapabilityAvailable"], true);
    assert_eq!(status["mcpAvailable"], true);
}

#[test]
fn public_status_requires_an_installed_object_owner() {
    let state = ApplicationMcp::default();
    let status = crate::application_mcp::object_client::client_status(&state).unwrap();
    assert_eq!(status["available"], false);
    assert_eq!(status["publicCapabilityAvailable"], false);
    assert_eq!(status["mcpAvailable"], false);
}

#[test]
fn public_visibility_uses_existing_owner_receipts_and_revision_rules() {
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let visibility = |id: &str, revision, visible| {
        request(
            &receipt,
            id,
            revision,
            "command.document.object.layer.visibility.set",
            json!({"command":"object.layer.visibility.set",
                "layerUid":"legacy layer:alpha","visible":visible}),
        )
    };
    let call = |value: Value| {
        serde_json::to_value(
            state
                .dispatch_native(serde_json::from_value(value).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let hide = visibility("public-hide", 0, false);
    let changed = call(hide.clone());
    assert_eq!(changed["ok"], true);
    assert_eq!(changed["contentRevision"], 1);
    assert_eq!(changed["result"]["applied"], true);
    assert_eq!(
        call(hide.clone()),
        changed,
        "exact retry retains its receipt"
    );
    let noop = call(visibility("public-noop", 1, false));
    assert_eq!(noop["ok"], true);
    assert_eq!(noop["contentRevision"], 1);
    assert_eq!(noop["result"]["applied"], false);
    let mut altered = hide;
    altered["payload"]["visible"] = json!(true);
    assert_eq!(call(altered)["error"]["code"], "invalid_request");
    assert_eq!(
        call(visibility("public-stale", 0, true))["error"]["code"],
        "stale_revision"
    );
    let query = read(&receipt, "public-after", 1, 0);
    let result = call(query);
    assert_eq!(result["ok"], true);
    assert_eq!(result["contentRevision"], 1);
    assert_eq!(result["result"]["object"], document()["objects"][0]);
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

#[test]
fn public_creation_uses_one_owner_receipt_history_and_preserves_existing_objects() {
    use nemo_mcp::{contract::NativeApplicationRequest, wire::decode_json_bounded};
    let state = ApplicationMcp::default();
    let receipt = install(&state);
    let create = request(
        &receipt,
        "public-create",
        0,
        "command.document.object.layer.create",
        json!({"command":"object.layer.create","name":"Drawing"}),
    );
    let raw = create.to_string();
    let admitted: NativeApplicationRequest = decode_json_bounded(raw.as_bytes(), 4096).unwrap();
    assert!(admitted.validate_input().is_ok());
    assert!(admitted.validate().is_ok());
    assert!(
        crate::application_mcp::object_client::dispatch_raw(&state, &raw).is_err(),
        "private route cannot bypass operation admission"
    );
    let public = |value: &Value| timing_tests::public(&state, value.clone());
    let created = public(&create);
    assert_eq!(created["ok"], true);
    assert_eq!(created["contentRevision"], 1);
    assert_eq!(created["result"]["applied"], true);
    assert_eq!(created["result"]["historyEntriesAdded"], 1);
    let uid = created["result"]["layerUid"].as_str().unwrap();
    assert!(!uid.is_empty());
    assert!(!document()["layers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|layer| layer["layerUid"] == uid));
    assert_eq!(
        public(&create),
        created,
        "exact replay is the same receipt, not another create"
    );
    let serialize = |revision| {
        let query = json!({"apiVersion":2,"requestId":format!("create-read-{revision}"),
            "instanceId":receipt["instanceId"],"documentId":receipt["documentId"],
            "operation":"query.document.object.serialize","payload":{"atRevision":revision}});
        serde_json::from_str::<Value>(
            dispatch(&state, &query)["result"]["documentJson"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    };
    let after_create = serialize(1);
    assert_eq!(after_create["layers"].as_array().unwrap().len(), 3);
    assert_eq!(after_create["layers"][2]["layerUid"], uid);
    assert_eq!(after_create["layers"][2]["locked"], false);
    assert_eq!(after_create["layers"][2]["color"], "#F0A585");
    assert!(after_create["layers"][2]["parentLayerUid"].is_null());
    assert_eq!(after_create["objects"], document()["objects"]);
    let mut changed = create.clone();
    changed["payload"]["name"] = json!("Different");
    assert_eq!(public(&changed)["error"]["code"], "invalid_request");
    let mut stale = create.clone();
    stale["requestId"] = json!("stale-create");
    assert_eq!(public(&stale)["error"]["code"], "stale_revision");
    let mut cancelled = create.clone();
    cancelled["requestId"] = json!("cancelled-create");
    cancelled["expectedRevision"] = json!(1);
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        public(&cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(
        serialize(1),
        after_create,
        "refusals leave the committed document unchanged"
    );
    let after = dispatch(&state, &read(&receipt, "after-create", 1, 0));
    assert_eq!(after["ok"], true);
    assert_eq!(after["contentRevision"], 1);
    assert_eq!(after["result"]["object"], document()["objects"][0]);
    let undo = dispatch(&state, &history(&receipt, "undo-create", 1, "undo"));
    assert_eq!(undo["ok"], true);
    assert_eq!(undo["contentRevision"], 2);
    assert_eq!(serialize(2)["layers"], document()["layers"]);
    let redo = dispatch(&state, &history(&receipt, "redo-create", 2, "redo"));
    assert_eq!(redo["ok"], true);
    assert_eq!(redo["contentRevision"], 3);
    assert_eq!(serialize(3), after_create);
    // Existing available object operations retain their original identity and content.
    assert_eq!(
        dispatch(&state, &fill(&receipt, "after-create-fill", 3))["contentRevision"],
        4
    );
    assert_eq!(
        dispatch(&state, &history(&receipt, "after-fill-undo", 4, "undo"))["contentRevision"],
        5
    );
    assert_eq!(
        dispatch(&state, &read(&receipt, "after-fill-undo-read", 5, 0))["result"]["object"],
        document()["objects"][0]
    );
}
